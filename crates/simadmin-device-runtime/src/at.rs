use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use simadmin_protocol::CapabilityManifest;

pub const AT_SNAPSHOT_COMMANDS: &[&str] = &[
    "AT+CGMI",
    "AT+CGMM",
    "AT+CGMR",
    "AT+CGSN",
    "AT+CPIN?",
    "AT+CCID",
    "AT+CIMI",
    "AT+CNUM",
    "AT+CSCA?",
    "AT+COPS?",
    "AT+CEREG?",
    "AT+CREG?",
    "AT+CSQ",
    "AT+CGATT?",
    "AT+CGACT?",
    "AT+CGDCONT?",
    "AT+CFUN?",
    "AT+QTEMP",
];

pub const AT_CAPABILITY_COMMANDS: &[&str] = &[
    "AT+CPIN?",
    "AT+CMGF=?",
    "AT+CFUN=?",
    "AT+CGATT=?",
    "AT+CGDCONT=?",
    "AT+CUSD=?",
    "AT+CSIM=?",
    "AT+CCHO=?",
    "AT+CGLA=?",
    "AT+CCHC=?",
    "AT+QTEMP",
];

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AtModemSnapshot {
    pub imei: Option<String>,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub revision: Option<String>,
    pub sim_present: bool,
    pub sim_state: String,
    pub iccid: Option<String>,
    pub imsi: Option<String>,
    pub phone_number: Option<String>,
    pub sms_center: Option<String>,
    pub operator_name: Option<String>,
    pub operator_code: Option<String>,
    pub network_type: Option<String>,
    pub registration_status: String,
    pub signal_percent: Option<u8>,
    pub signal_dbm: Option<i16>,
    pub data_attached: bool,
    pub data_active: bool,
    pub airplane_mode: bool,
    pub apn: Option<String>,
    pub temperatures: Vec<AtTemperature>,
}

impl AtModemSnapshot {
    pub fn sim_snapshot(&self) -> Value {
        json!({
            "present": self.sim_present,
            "state": self.sim_state,
            "iccid": self.iccid,
            "imsi": self.imsi,
            "phone_number": self.phone_number,
            "sms_center": self.sms_center,
            "operator_code": self.operator_code,
            "operator_name": self.operator_name,
        })
    }

    pub fn cellular_snapshot(&self) -> Value {
        json!({
            "registration_status": self.registration_status,
            "network_type": self.network_type,
            "signal_percent": self.signal_percent,
            "signal_dbm": self.signal_dbm,
            "data_attached": self.data_attached,
            "data_active": self.data_active,
            "data_enabled": self.data_attached,
            "airplane_mode": self.airplane_mode,
            "apn": self.apn,
            "temperatures": self.temperatures,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AtTemperature {
    pub label: String,
    pub temperature: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AtSmsMessage {
    pub index: u32,
    pub status: String,
    pub phone_number: String,
    pub timestamp: String,
    pub content: String,
}

pub fn response_succeeded(response: &str) -> bool {
    response.lines().any(|line| line.trim() == "OK")
        && !response.contains("+CME ERROR")
        && !response.contains("+CMS ERROR")
}

pub fn capabilities_from_at_probe(responses: &HashMap<String, String>) -> Vec<String> {
    let supports = |command: &str| {
        responses
            .get(command)
            .is_some_and(|value| response_succeeded(value))
    };
    let mut capabilities = vec!["network".to_owned()];
    if supports("AT+CPIN?") {
        capabilities.push("sim".into());
    }
    if supports("AT+CMGF=?") {
        capabilities.extend(["sms", "sms_send", "sms_receive"].map(str::to_owned));
    }
    if supports("AT+CFUN=?") {
        capabilities.extend(["airplane_mode", "baseband_restart"].map(str::to_owned));
    }
    if supports("AT+CGATT=?") {
        capabilities.push("data_control".into());
    }
    if supports("AT+CGDCONT=?") {
        capabilities.push("apn_control".into());
    }
    if supports("AT+CUSD=?") {
        capabilities.push("ussd".into());
    }
    if supports("AT+CCHO=?") && supports("AT+CGLA=?") && supports("AT+CCHC=?") {
        capabilities.push("sim_apdu".into());
    }
    if supports("AT+QTEMP") {
        capabilities.push("temperature".into());
    }
    capabilities.sort();
    capabilities.dedup();
    capabilities
}

pub fn capability_manifest_from_at_probe(
    responses: &HashMap<String, String>,
) -> CapabilityManifest {
    let capabilities = capabilities_from_at_probe(responses);
    let mut manifest = CapabilityManifest::from_legacy(&capabilities);
    manifest
        .attributes
        .insert("backend".into(), Value::String("direct_at".into()));
    if capabilities.iter().any(|value| value == "sim_apdu") {
        manifest
            .attributes
            .insert("sim_apdu_transport".into(), Value::String("at_csim".into()));
    }
    manifest
}

pub fn parse_at_snapshot(responses: &HashMap<String, String>) -> AtModemSnapshot {
    let identity = |command: &str| {
        responses
            .get(command)
            .and_then(|response| first_value_line(response, command))
    };
    let imei = responses
        .get("AT+CGSN")
        .and_then(|response| find_digits(response, 14, 16));
    let sim_state =
        prefixed_value(responses.get("AT+CPIN?"), "+CPIN:").unwrap_or_else(|| "UNKNOWN".into());
    let iccid = responses
        .get("AT+CCID")
        .and_then(|response| find_digits(response, 18, 22));
    let imsi = responses
        .get("AT+CIMI")
        .and_then(|response| find_digits(response, 14, 16));
    let cnum = prefixed_value(responses.get("AT+CNUM"), "+CNUM:")
        .map(|value| csv_fields(&value))
        .unwrap_or_default();
    let phone_number = cnum.iter().find(|value| valid_phone_number(value)).cloned();
    let sms_center = prefixed_value(responses.get("AT+CSCA?"), "+CSCA:")
        .and_then(|value| csv_fields(&value).into_iter().next())
        .filter(|value| !value.is_empty());

    let cops = prefixed_value(responses.get("AT+COPS?"), "+COPS:")
        .map(|value| csv_fields(&value))
        .unwrap_or_default();
    let operator_name = cops.get(2).cloned().filter(|value| !value.is_empty());
    let network_type = cops
        .get(3)
        .and_then(|value| value.parse::<u8>().ok())
        .map(access_technology);
    let operator_code = operator_name
        .as_deref()
        .filter(|value| {
            (5..=6).contains(&value.len()) && value.bytes().all(|byte| byte.is_ascii_digit())
        })
        .map(str::to_owned);
    let registration_code = prefixed_value(responses.get("AT+CEREG?"), "+CEREG:")
        .or_else(|| prefixed_value(responses.get("AT+CREG?"), "+CREG:"))
        .and_then(|value| {
            csv_fields(&value)
                .into_iter()
                .filter_map(|field| field.parse::<u8>().ok())
                .next_back()
        });
    let registration_status = registration_code
        .map(registration_label)
        .unwrap_or("unknown")
        .to_owned();
    let signal = prefixed_value(responses.get("AT+CSQ"), "+CSQ:")
        .and_then(|value| csv_fields(&value).into_iter().next())
        .and_then(|value| value.parse::<u8>().ok())
        .filter(|value| *value <= 31);
    let data_attached =
        prefixed_value(responses.get("AT+CGATT?"), "+CGATT:").as_deref() == Some("1");
    let data_active = responses.get("AT+CGACT?").is_some_and(|response| {
        response.lines().map(str::trim).any(|line| {
            line.strip_prefix("+CGACT:")
                .map(csv_fields)
                .is_some_and(|fields| fields.get(1).is_some_and(|value| value == "1"))
        })
    });
    let apn = responses.get("AT+CGDCONT?").and_then(|response| {
        response.lines().map(str::trim).find_map(|line| {
            let fields = csv_fields(line.strip_prefix("+CGDCONT:")?.trim());
            fields.get(2).cloned().filter(|value| !value.is_empty())
        })
    });
    let airplane_mode = prefixed_value(responses.get("AT+CFUN?"), "+CFUN:")
        .is_some_and(|value| matches!(value.as_str(), "0" | "4"));
    let temperatures = responses
        .get("AT+QTEMP")
        .map(|response| parse_temperatures(response))
        .unwrap_or_default();

    AtModemSnapshot {
        imei,
        manufacturer: identity("AT+CGMI"),
        model: identity("AT+CGMM"),
        revision: identity("AT+CGMR"),
        sim_present: !matches!(sim_state.as_str(), "UNKNOWN" | "NOT INSERTED"),
        sim_state,
        iccid,
        imsi,
        phone_number,
        sms_center,
        operator_name,
        operator_code,
        network_type,
        registration_status,
        signal_percent: signal.map(|value| ((u16::from(value) * 100 + 15) / 31) as u8),
        signal_dbm: signal.map(|value| -113 + i16::from(value) * 2),
        data_attached,
        data_active,
        airplane_mode,
        apn,
        temperatures,
    }
}

pub fn parse_at_sms_list(response: &str) -> Vec<AtSmsMessage> {
    let mut messages = Vec::new();
    let mut current: Option<AtSmsMessage> = None;
    for raw_line in response.lines() {
        let line = raw_line.trim();
        if let Some(header) = line.strip_prefix("+CMGL:") {
            if let Some(message) = current.take() {
                messages.push(message);
            }
            let fields = csv_fields(header.trim());
            let Some(index) = fields.first().and_then(|value| value.parse::<u32>().ok()) else {
                continue;
            };
            current = Some(AtSmsMessage {
                index,
                status: fields.get(1).cloned().unwrap_or_default(),
                phone_number: fields.get(2).cloned().unwrap_or_default(),
                timestamp: fields.get(4).cloned().unwrap_or_default(),
                content: String::new(),
            });
            continue;
        }
        if matches!(line, "OK" | "ERROR") || line.starts_with("AT+CMGL") {
            continue;
        }
        if let Some(message) = current.as_mut() {
            if !message.content.is_empty() {
                message.content.push('\n');
            }
            message.content.push_str(line);
        }
    }
    if let Some(message) = current {
        messages.push(message);
    }
    messages
}

fn first_value_line(response: &str, command: &str) -> Option<String> {
    response
        .lines()
        .map(str::trim)
        .find(|line| {
            !line.is_empty()
                && *line != command
                && !matches!(*line, "OK" | "ERROR")
                && !line.starts_with('+')
        })
        .map(str::to_owned)
}

fn prefixed_value(response: Option<&String>, prefix: &str) -> Option<String> {
    response?.lines().map(str::trim).find_map(|line| {
        line.strip_prefix(prefix)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    })
}

fn find_digits(response: &str, minimum: usize, maximum: usize) -> Option<String> {
    response
        .split(|character: char| !character.is_ascii_digit())
        .find(|value| {
            (minimum..=maximum).contains(&value.len())
                && value.bytes().any(|digit| digit != value.as_bytes()[0])
        })
        .map(str::to_owned)
}

fn csv_fields(value: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for character in value.chars() {
        match character {
            '"' => quoted = !quoted,
            ',' if !quoted => {
                fields.push(current.trim().to_owned());
                current.clear();
            }
            _ => current.push(character),
        }
    }
    fields.push(current.trim().to_owned());
    fields
}

fn valid_phone_number(value: &str) -> bool {
    let digits = value
        .chars()
        .filter(char::is_ascii_digit)
        .collect::<String>();
    digits.len() >= 5 && digits.bytes().any(|digit| digit != b'0')
}

fn access_technology(value: u8) -> String {
    match value {
        0 => "GSM",
        2 => "UTRAN",
        3 => "GSM EDGE",
        4 => "HSDPA",
        5 => "HSUPA",
        6 => "HSPA+",
        7 => "LTE",
        9 | 13 => "5G NR",
        _ => "UNKNOWN",
    }
    .into()
}

fn registration_label(value: u8) -> &'static str {
    match value {
        1 => "home",
        2 => "searching",
        3 => "denied",
        5 => "roaming",
        8 => "emergency",
        _ => "idle",
    }
}

fn parse_temperatures(response: &str) -> Vec<AtTemperature> {
    let mut values = response
        .lines()
        .filter(|line| line.contains("QTEMP"))
        .flat_map(|line| {
            line.split(|character: char| {
                !(character.is_ascii_digit() || matches!(character, '.' | '-'))
            })
        })
        .filter_map(|value| value.parse::<f32>().ok())
        .filter(|value| (-50.0..=180.0).contains(value))
        .collect::<Vec<_>>();
    values.dedup_by(|left, right| (*left - *right).abs() < f32::EPSILON);
    values
        .into_iter()
        .enumerate()
        .map(|(index, temperature)| AtTemperature {
            label: if index == 0 {
                "基带".into()
            } else {
                format!("传感器 {}", index + 1)
            },
            temperature,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_modem_snapshot_without_vendor_assumptions() {
        let responses = HashMap::from([
            ("AT+CGMI".into(), "AT+CGMI\r\nQuectel\r\nOK".into()),
            ("AT+CGMM".into(), "EC25\r\nOK".into()),
            ("AT+CGMR".into(), "EC25EFAR06A08M4G\r\nOK".into()),
            ("AT+CGSN".into(), "860123456789012\r\nOK".into()),
            ("AT+CPIN?".into(), "+CPIN: READY\r\nOK".into()),
            ("AT+CCID".into(), "+CCID: 89860123456789012345\r\nOK".into()),
            ("AT+CIMI".into(), "460011234567890\r\nOK".into()),
            (
                "AT+CNUM".into(),
                "+CNUM: \"\",\"13800138000\",129\r\nOK".into(),
            ),
            (
                "AT+CSCA?".into(),
                "+CSCA: \"+8613010112500\",145\r\nOK".into(),
            ),
            (
                "AT+COPS?".into(),
                "+COPS: 0,0,\"CHN-UNICOM\",7\r\nOK".into(),
            ),
            ("AT+CEREG?".into(), "+CEREG: 0,1\r\nOK".into()),
            ("AT+CSQ".into(), "+CSQ: 20,99\r\nOK".into()),
            ("AT+CGATT?".into(), "+CGATT: 1\r\nOK".into()),
            ("AT+CGACT?".into(), "+CGACT: 1,1\r\nOK".into()),
            (
                "AT+CGDCONT?".into(),
                "+CGDCONT: 1,\"IP\",\"3gnet\",\"0.0.0.0\",0,0\r\nOK".into(),
            ),
            ("AT+CFUN?".into(), "+CFUN: 1\r\nOK".into()),
            ("AT+QTEMP".into(), "+QTEMP: \"modem\",47.5\r\nOK".into()),
        ]);

        let snapshot = parse_at_snapshot(&responses);
        assert_eq!(snapshot.imei.as_deref(), Some("860123456789012"));
        assert_eq!(snapshot.phone_number.as_deref(), Some("13800138000"));
        assert_eq!(snapshot.network_type.as_deref(), Some("LTE"));
        assert_eq!(snapshot.registration_status, "home");
        assert_eq!(snapshot.signal_percent, Some(65));
        assert_eq!(snapshot.signal_dbm, Some(-73));
        assert!(snapshot.data_active);
        assert_eq!(snapshot.apn.as_deref(), Some("3gnet"));
        assert_eq!(snapshot.temperatures[0].temperature, 47.5);
    }

    #[test]
    fn capabilities_only_include_successfully_probed_operations() {
        let responses = HashMap::from([
            ("AT+CPIN?".into(), "+CPIN: READY\r\nOK".into()),
            ("AT+CMGF=?".into(), "+CMGF: (0,1)\r\nOK".into()),
            ("AT+CSIM=?".into(), "ERROR".into()),
        ]);
        let capabilities = capabilities_from_at_probe(&responses);
        assert!(capabilities.contains(&"sim".into()));
        assert!(capabilities.contains(&"sms_receive".into()));
        assert!(!capabilities.contains(&"sim_apdu".into()));
    }

    #[test]
    fn parses_multiline_sms_list() {
        let messages = parse_at_sms_list(
            "AT+CMGL=\"REC UNREAD\"\r\n+CMGL: 7,\"REC UNREAD\",\"10010\",\"\",\"26/08/28,12:30:00+32\"\r\n余额提醒\r\n+CMGL: 8,\"REC READ\",\"10086\",\"\",\"26/08/28,12:31:00+32\"\r\n套餐信息\r\nOK",
        );
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].index, 7);
        assert_eq!(messages[0].phone_number, "10010");
        assert_eq!(messages[0].content, "余额提醒");
    }
}
