use std::collections::HashMap;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use simadmin_protocol::{DeviceApiRequestPayload, DeviceApiResponsePayload};
use zbus::zvariant::{OwnedObjectPath, Value as ZbusValue};
use zbus::{zvariant::OwnedValue, Connection, MessageStream, Proxy};

mod apdu;
mod at;
mod at_apdu;
pub mod lpac;
pub mod mbim_uicc;
mod process;
pub mod qmi_uim;
mod system;
mod usim_auth;

pub use apdu::*;
pub use at::*;
pub use at_apdu::*;
pub use lpac::*;
pub use process::*;
pub use system::*;
pub use usim_auth::*;

const MM_SERVICE: &str = "org.freedesktop.ModemManager1";
const DBUS_PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const MM_MODEM: &str = "org.freedesktop.ModemManager1.Modem";
const MM_MODEM_3GPP: &str = "org.freedesktop.ModemManager1.Modem.Modem3gpp";
const MM_MESSAGING: &str = "org.freedesktop.ModemManager1.Modem.Messaging";
const MM_SIM: &str = "org.freedesktop.ModemManager1.Sim";
const MM_SMS: &str = "org.freedesktop.ModemManager1.Sms";

const MM_MODE_NONE: u32 = 0;
const MM_MODE_2G: u32 = 1 << 1;
const MM_MODE_3G: u32 = 1 << 2;
const MM_MODE_4G: u32 = 1 << 3;
const MM_MODE_5G: u32 = 1 << 4;
const MM_MODE_ANY: u32 = u32::MAX;

type InterfaceProperties = HashMap<String, OwnedValue>;

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("invalid ModemManager object path: {0}")]
    InvalidModemPath(String),
    #[error("unsupported device API route: {0} {1}")]
    UnsupportedRoute(String, String),
    #[error("invalid device API request: {0}")]
    InvalidRequest(String),
    #[error("ModemManager SMS signal stream ended")]
    SmsSignalStreamEnded,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Dbus(#[from] zbus::Error),
}

pub type RuntimeResult<T> = Result<T, RuntimeError>;

/// An explicit modem selection. A context never discovers or silently switches modems.
/// This is the isolation boundary required by a multi-device Host Agent.
#[derive(Clone, Copy)]
pub struct ModemContext<'a> {
    connection: &'a Connection,
    modem_path: &'a str,
}

impl<'a> ModemContext<'a> {
    pub fn new(connection: &'a Connection, modem_path: &'a str) -> RuntimeResult<Self> {
        if !is_modem_path(modem_path) {
            return Err(RuntimeError::InvalidModemPath(modem_path.to_owned()));
        }
        Ok(Self {
            connection,
            modem_path,
        })
    }

    pub fn modem_path(&self) -> &str {
        self.modem_path
    }

    pub async fn device_info(&self) -> RuntimeResult<DeviceInfoResponse> {
        let modem = self.modem_properties().await?;
        let state = modem.get("State").map(extract_i32).unwrap_or(0);
        let imei = get_property(self.connection, self.modem_path, MM_MODEM_3GPP, "Imei")
            .await
            .map(|value| extract_string(&value))
            .unwrap_or_default();
        Ok(DeviceInfoResponse {
            imei,
            manufacturer: property_string(&modem, "Manufacturer"),
            model: property_string(&modem, "Model"),
            revision: modem
                .get("Revision")
                .map(extract_string)
                .filter(|value| !value.is_empty()),
            online: state >= 6,
            powered: state >= 3,
        })
    }

    pub async fn sim_info(&self) -> RuntimeResult<SimInfoResponse> {
        let modem = self.modem_properties().await?;
        let gpp = self.gpp_properties().await?;
        let sim_path = self.sim_path().await?;
        if sim_path.is_empty() || sim_path == "/" {
            return Ok(SimInfoResponse::default());
        }
        let sim = get_all_properties(self.connection, &sim_path, MM_SIM).await?;
        let iccid = normalize_iccid(&property_string(&sim, "SimIdentifier"));
        let imsi = property_string(&sim, "Imsi");
        let mut operator_code = property_string(&sim, "OperatorIdentifier");
        if operator_code.is_empty() {
            operator_code = operator_code_from_imsi(&imsi);
        }
        if operator_code.is_empty() {
            operator_code = property_string(&gpp, "OperatorCode");
        }
        let (mcc, mnc) = split_operator_code(&operator_code);
        let mut phone_numbers = extract_own_numbers(&sim);
        if phone_numbers.is_empty() {
            phone_numbers = extract_own_numbers(&modem);
        }
        if phone_numbers.is_empty() {
            phone_numbers = extract_own_numbers(&gpp);
        }
        let unlock_retries = modem
            .get("UnlockRetries")
            .and_then(|value| HashMap::<u32, u32>::try_from(value.clone()).ok())
            .unwrap_or_default();
        Ok(SimInfoResponse {
            present: true,
            iccid,
            imsi,
            phone_numbers,
            sms_center: extract_smsc(&sim),
            mcc,
            mnc,
            phone_number_is_manual: false,
            sms_center_is_manual: false,
            sim_path,
            modem_path: self.modem_path.to_owned(),
            sim_type: match modem_u32(&sim, "SimType") {
                1 => "physical",
                2 => "esim",
                _ => "unknown",
            }
            .to_owned(),
            esim_status: match modem_u32(&sim, "EsimStatus") {
                1 => "none",
                2 => "no-profiles",
                3 => "with-profiles",
                _ => "unknown",
            }
            .to_owned(),
            active: sim.get("Active").map(extract_bool).unwrap_or(false),
            operator_name: property_string(&sim, "OperatorName"),
            registered_operator_name: property_string(&gpp, "OperatorName"),
            registered_operator_code: property_string(&gpp, "OperatorCode"),
            lock_status: match modem_u32(&modem, "UnlockRequired") {
                1 => "none",
                2 => "sim-pin",
                3 => "sim-pin2",
                4 => "sim-puk",
                5 => "sim-puk2",
                _ => "unknown",
            }
            .to_owned(),
            pin1_retries: unlock_retries.get(&2).copied(),
            puk1_retries: unlock_retries.get(&4).copied(),
            pin2_retries: unlock_retries.get(&3).copied(),
            puk2_retries: unlock_retries.get(&5).copied(),
            carrier_config: property_string(&modem, "CarrierConfiguration"),
            carrier_config_revision: property_string(&modem, "CarrierConfigurationRevision"),
        })
    }

    pub async fn network_info(&self) -> RuntimeResult<NetworkInfoResponse> {
        let modem = self.modem_properties().await?;
        let gpp = self.gpp_properties().await?;
        let operator_code = property_string(&gpp, "OperatorCode");
        let (mcc, mnc) = split_operator_code_optional(&operator_code);
        Ok(NetworkInfoResponse {
            operator_name: property_string(&gpp, "OperatorName"),
            registration_status: registration_label(modem_u32(&gpp, "RegistrationState"))
                .to_owned(),
            technology_preference: access_technology_label(modem_u32(&modem, "AccessTechnologies")),
            signal_strength: signal_quality(&modem),
            mcc,
            mnc,
        })
    }

    pub async fn data_connection(&self) -> RuntimeResult<DataConnectionResponse> {
        let state = self
            .modem_properties()
            .await?
            .get("State")
            .map(extract_i32)
            .unwrap_or(0);
        Ok(DataConnectionResponse {
            active: state >= 11,
        })
    }

    pub async fn airplane_mode(&self) -> RuntimeResult<AirplaneModeResponse> {
        let state = self
            .modem_properties()
            .await?
            .get("State")
            .map(extract_i32)
            .unwrap_or(0);
        Ok(AirplaneModeResponse {
            enabled: matches!(state, 3 | 4),
            powered: state >= 3,
            online: state >= 6,
        })
    }

    pub async fn radio_mode(&self) -> RuntimeResult<RadioModeResponse> {
        let current =
            get_property(self.connection, self.modem_path, MM_MODEM, "CurrentModes").await?;
        let supported =
            get_property(self.connection, self.modem_path, MM_MODEM, "SupportedModes").await?;
        let (allowed, preferred) =
            <(u32, u32)>::try_from(current).unwrap_or((MM_MODE_NONE, MM_MODE_NONE));
        let pairs = Vec::<(u32, u32)>::try_from(supported).unwrap_or_default();
        let modem = self.modem_properties().await?;
        Ok(RadioModeResponse {
            mode: normalize_mode(allowed, preferred),
            technology_preference: access_technology_label(modem_u32(&modem, "AccessTechnologies")),
            supported_modes: supported_mode_labels(&pairs),
        })
    }

    pub async fn execute_api(
        &self,
        request: &DeviceApiRequestPayload,
    ) -> RuntimeResult<DeviceApiResponsePayload> {
        let method = request.method.trim().to_ascii_uppercase();
        let route = request.path.split('?').next().unwrap_or(&request.path);
        if method != "GET" {
            return Err(RuntimeError::UnsupportedRoute(method, route.to_owned()));
        }
        let body = match route {
            "/device" => ok_envelope(self.device_info().await?),
            "/sim" => ok_envelope(self.sim_info().await?),
            "/network" => ok_envelope(self.network_info().await?),
            "/data" => ok_envelope(self.data_connection().await?),
            "/airplane-mode" => ok_envelope(self.airplane_mode().await?),
            "/radio-mode" => ok_envelope(self.radio_mode().await?),
            "/network/signal-strength" | "/signal" => {
                let strength = self.network_info().await?.signal_strength;
                ok_envelope(json!({ "strength": strength }))
            }
            "/work-mode" => ok_envelope(json!({
                "mode": "sim",
                "worker_running": false,
            })),
            _ => return Err(RuntimeError::UnsupportedRoute(method, route.to_owned())),
        };
        Ok(DeviceApiResponsePayload { status: 200, body })
    }

    pub async fn received_sms(&self) -> RuntimeResult<Vec<ReceivedSms>> {
        let paths = self.sms_paths().await?;
        let mut messages = Vec::new();
        for path in paths {
            if let Some(message) = self.received_sms_at(&path).await? {
                messages.push(message);
            }
        }
        Ok(messages)
    }

    pub async fn sms_paths(&self) -> RuntimeResult<Vec<String>> {
        let proxy = Proxy::new(self.connection, MM_SERVICE, self.modem_path, MM_MESSAGING).await?;
        let paths: Vec<OwnedObjectPath> = proxy.call("List", &()).await?;
        Ok(paths.into_iter().map(|path| path.to_string()).collect())
    }

    pub async fn send_sms(&self, phone_number: &str, content: &str) -> RuntimeResult<String> {
        let phone_number = phone_number.trim();
        if phone_number.is_empty() || content.is_empty() {
            return Err(RuntimeError::InvalidRequest(
                "SMS phone number and content are required".into(),
            ));
        }
        let proxy = Proxy::new(self.connection, MM_SERVICE, self.modem_path, MM_MESSAGING).await?;
        let mut properties: HashMap<String, ZbusValue<'_>> = HashMap::new();
        properties.insert("number".to_owned(), ZbusValue::new(phone_number));
        properties.insert("text".to_owned(), ZbusValue::new(content));
        let sms_path: OwnedObjectPath = proxy.call("Create", &(properties,)).await?;
        let sms = Proxy::new(self.connection, MM_SERVICE, sms_path.as_str(), MM_SMS).await?;
        sms.call::<_, _, ()>("Send", &()).await?;
        Ok(sms_path.to_string())
    }

    pub async fn delete_sms(&self, sms_path: &str) -> RuntimeResult<()> {
        if !sms_path.starts_with("/org/freedesktop/ModemManager1/SMS/") {
            return Err(RuntimeError::InvalidRequest(format!(
                "invalid ModemManager SMS path: {sms_path}"
            )));
        }
        let path = zbus::zvariant::ObjectPath::try_from(sms_path)
            .map_err(|_| RuntimeError::InvalidRequest("invalid SMS object path".into()))?;
        let proxy = Proxy::new(self.connection, MM_SERVICE, self.modem_path, MM_MESSAGING).await?;
        proxy.call::<_, _, ()>("Delete", &(path,)).await?;
        Ok(())
    }

    pub async fn received_sms_at(&self, sms_path: &str) -> RuntimeResult<Option<ReceivedSms>> {
        let properties = get_all_properties(self.connection, sms_path, MM_SMS).await?;
        if modem_u32(&properties, "State") != 3 {
            return Ok(None);
        }
        let text = property_string(&properties, "Text");
        let data = properties
            .get("Data")
            .and_then(|value| Vec::<u8>::try_from(value.clone()).ok())
            .filter(|value| !value.is_empty())
            .map(|value| String::from_utf8_lossy(&value).into_owned())
            .unwrap_or_default();
        let timestamp = ["Timestamp", "Time", "ReceivedTimestamp"]
            .iter()
            .map(|name| property_string(&properties, name))
            .find(|value| !value.is_empty())
            .unwrap_or_default();
        Ok(Some(ReceivedSms {
            path: sms_path.to_owned(),
            number: property_string(&properties, "Number"),
            content: if text.is_empty() { data } else { text },
            timestamp,
            sms_center: extract_smsc(&properties),
        }))
    }

    async fn modem_properties(&self) -> zbus::Result<InterfaceProperties> {
        get_all_properties(self.connection, self.modem_path, MM_MODEM).await
    }

    async fn gpp_properties(&self) -> zbus::Result<InterfaceProperties> {
        get_all_properties(self.connection, self.modem_path, MM_MODEM_3GPP).await
    }

    async fn sim_path(&self) -> zbus::Result<String> {
        let value = get_property(self.connection, self.modem_path, MM_MODEM, "Sim").await?;
        Ok(zbus::zvariant::ObjectPath::try_from(value.clone())
            .map(|path| path.to_string())
            .unwrap_or_else(|_| extract_string(&value)))
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct DeviceInfoResponse {
    pub imei: String,
    pub manufacturer: String,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    pub online: bool,
    pub powered: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct SimInfoResponse {
    pub present: bool,
    pub iccid: String,
    pub imsi: String,
    pub phone_numbers: Vec<String>,
    pub sms_center: String,
    pub mcc: String,
    pub mnc: String,
    pub phone_number_is_manual: bool,
    pub sms_center_is_manual: bool,
    pub sim_path: String,
    pub modem_path: String,
    pub sim_type: String,
    pub esim_status: String,
    pub active: bool,
    pub operator_name: String,
    pub registered_operator_name: String,
    pub registered_operator_code: String,
    pub lock_status: String,
    pub pin1_retries: Option<u32>,
    pub puk1_retries: Option<u32>,
    pub pin2_retries: Option<u32>,
    pub puk2_retries: Option<u32>,
    pub carrier_config: String,
    pub carrier_config_revision: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct NetworkInfoResponse {
    pub operator_name: String,
    pub registration_status: String,
    pub technology_preference: String,
    pub signal_strength: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mnc: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct DataConnectionResponse {
    pub active: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct AirplaneModeResponse {
    pub enabled: bool,
    pub powered: bool,
    pub online: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct RadioModeResponse {
    pub mode: String,
    pub technology_preference: String,
    pub supported_modes: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct ReceivedSms {
    pub path: String,
    pub number: String,
    pub content: String,
    pub timestamp: String,
    pub sms_center: String,
}

/// Waits for received-SMS signals from every ModemManager modem on this bus.
/// Callers should rescan their explicitly bound modem contexts after each wakeup.
pub async fn watch_received_sms(
    connection: &Connection,
    mut on_received: impl FnMut() + Send,
) -> RuntimeResult<()> {
    let rule =
        format!("type='signal',sender='{MM_SERVICE}',interface='{MM_MESSAGING}',member='Added'");
    let dbus = Proxy::new(
        connection,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )
    .await?;
    dbus.call::<_, _, ()>("AddMatch", &(&rule,)).await?;
    let mut stream = MessageStream::from(connection);
    while let Some(message) = stream.next().await {
        let message = message?;
        let is_received = message
            .body()
            .deserialize::<(zbus::zvariant::ObjectPath<'_>, bool)>()
            .map(|(_, received)| received)
            .unwrap_or(false);
        if is_received {
            on_received();
        }
    }
    let _ = dbus.call::<_, _, ()>("RemoveMatch", &(&rule,)).await;
    Err(RuntimeError::SmsSignalStreamEnded)
}

fn is_modem_path(path: &str) -> bool {
    path.strip_prefix("/org/freedesktop/ModemManager1/Modem/")
        .is_some_and(|id| !id.is_empty() && id.bytes().all(|value| value.is_ascii_digit()))
}

async fn get_all_properties(
    connection: &Connection,
    path: &str,
    interface: &str,
) -> zbus::Result<InterfaceProperties> {
    let proxy = Proxy::new(connection, MM_SERVICE, path, DBUS_PROPERTIES).await?;
    proxy.call("GetAll", &(interface,)).await
}

async fn get_property(
    connection: &Connection,
    path: &str,
    interface: &str,
    property: &str,
) -> zbus::Result<OwnedValue> {
    let proxy = Proxy::new(connection, MM_SERVICE, path, DBUS_PROPERTIES).await?;
    proxy.call("Get", &(interface, property)).await
}

fn extract_string(value: &OwnedValue) -> String {
    String::try_from(value.clone()).unwrap_or_default()
}

fn extract_i32(value: &OwnedValue) -> i32 {
    i32::try_from(value.clone()).unwrap_or_default()
}

fn extract_u32(value: &OwnedValue) -> u32 {
    u32::try_from(value.clone()).unwrap_or_default()
}

fn extract_bool(value: &OwnedValue) -> bool {
    bool::try_from(value.clone()).unwrap_or_default()
}

fn property_string(properties: &InterfaceProperties, name: &str) -> String {
    properties.get(name).map(extract_string).unwrap_or_default()
}

fn modem_u32(properties: &InterfaceProperties, name: &str) -> u32 {
    properties.get(name).map(extract_u32).unwrap_or_default()
}

fn signal_quality(properties: &InterfaceProperties) -> u8 {
    properties
        .get("SignalQuality")
        .and_then(|value| <(u32, bool)>::try_from(value.clone()).ok())
        .map(|(quality, _)| quality.min(100) as u8)
        .unwrap_or_default()
}

fn extract_string_list(value: &OwnedValue) -> Vec<String> {
    Vec::<String>::try_from(value.clone()).unwrap_or_else(|_| {
        let value = extract_string(value);
        if value.is_empty() {
            Vec::new()
        } else {
            vec![value]
        }
    })
}

fn extract_own_numbers(properties: &InterfaceProperties) -> Vec<String> {
    let mut values = Vec::new();
    for name in [
        "OwnNumbers",
        "OwnNumber",
        "PhoneNumbers",
        "PhoneNumber",
        "MSISDN",
        "Msisdn",
        "SubscriberNumber",
        "own-numbers",
        "own-number",
        "phone-numbers",
        "phone-number",
        "msisdn",
        "subscriber-number",
        "telephone-numbers",
        "telephone-number",
    ] {
        if let Some(value) = properties.get(name) {
            values.extend(extract_string_list(value));
        }
    }
    values = values
        .into_iter()
        .filter_map(|value| normalize_phone_number(&value))
        .collect();
    values.sort();
    values.dedup();
    values
}

fn extract_smsc(properties: &InterfaceProperties) -> String {
    for name in [
        "SMSC",
        "Smsc",
        "SmsCenter",
        "DefaultSmsc",
        "DefaultSmsCenter",
    ] {
        let value = normalize_smsc(&property_string(properties, name));
        if !value.is_empty() {
            return value;
        }
    }
    String::new()
}

fn normalize_phone_number(value: &str) -> Option<String> {
    let trimmed = value
        .trim()
        .trim_matches(|character| matches!(character, '"' | '\'' | ',' | ';'))
        .trim();
    let trimmed = trimmed.strip_prefix("tel:").unwrap_or(trimmed);
    let mut normalized = String::new();
    for character in trimmed.chars() {
        if character.is_ascii_digit() || (character == '+' && normalized.is_empty()) {
            normalized.push(character);
        }
    }
    let digits = normalized.strip_prefix('+').unwrap_or(&normalized);
    ((4..=20).contains(&digits.len())
        && digits.bytes().all(|value| value.is_ascii_digit())
        && digits.bytes().any(|value| value != b'0'))
    .then_some(normalized)
}

fn normalize_smsc(value: &str) -> String {
    let value = value
        .trim()
        .trim_matches(|character| matches!(character, '"' | '\'' | ',' | ';'))
        .trim();
    let digits = value.strip_prefix('+').unwrap_or(value);
    if (4..=20).contains(&digits.len())
        && digits.bytes().all(|byte| byte.is_ascii_digit())
        && digits.bytes().any(|byte| byte != b'0')
    {
        value.to_owned()
    } else {
        String::new()
    }
}

fn normalize_iccid(value: &str) -> String {
    value
        .trim()
        .chars()
        .filter(char::is_ascii_digit)
        .take(19)
        .collect()
}

fn operator_code_from_imsi(imsi: &str) -> String {
    let digits = imsi.trim();
    if digits.len() < 5 || !digits.bytes().all(|value| value.is_ascii_digit()) {
        return String::new();
    }
    if digits.starts_with("460") {
        digits[..5].to_owned()
    } else if digits.len() >= 6 {
        digits[..6].to_owned()
    } else {
        String::new()
    }
}

fn split_operator_code(code: &str) -> (String, String) {
    if code.len() < 5 {
        return (String::new(), String::new());
    }
    (code[..3].to_owned(), code[3..].to_owned())
}

fn split_operator_code_optional(code: &str) -> (Option<String>, Option<String>) {
    let (mcc, mnc) = split_operator_code(code);
    if mcc.is_empty() {
        (None, None)
    } else {
        (Some(mcc), Some(mnc))
    }
}

fn registration_label(value: u32) -> &'static str {
    match value {
        0 => "idle",
        1 | 6 | 9 => "registered",
        2 => "searching",
        3 => "denied",
        5 | 7 | 10 => "roaming",
        8 => "attached",
        _ => "unknown",
    }
}

fn access_technology_label(value: u32) -> String {
    for (mask, label) in [
        (1 << 17, "nb-iot"),
        (1 << 16, "cat-m"),
        (1 << 15, "nr"),
        (1 << 14, "lte-advanced"),
        (1 << 13, "lte"),
        (1 << 12, "evdob"),
        (1 << 11, "evdoa"),
        (1 << 10, "evdo0"),
        (1 << 9, "1xrtt"),
        (1 << 8, "hspa+"),
        (1 << 7, "hspa"),
        (1 << 6, "hsupa"),
        (1 << 5, "hsdpa"),
        (1 << 4, "umts"),
        (1 << 3, "edge"),
        (1 << 2, "gprs"),
        (1 << 1, "gsm-compact"),
        (1, "pots"),
    ] {
        if value & mask != 0 {
            return label.to_owned();
        }
    }
    "unknown".to_owned()
}

fn normalize_mode(allowed: u32, preferred: u32) -> String {
    if allowed == MM_MODE_5G || (preferred == MM_MODE_5G && allowed & MM_MODE_4G == 0) {
        "nr".to_owned()
    } else if allowed == MM_MODE_4G || (preferred == MM_MODE_4G && allowed & MM_MODE_5G == 0) {
        "lte".to_owned()
    } else {
        "auto".to_owned()
    }
}

fn supported_mode_labels(pairs: &[(u32, u32)]) -> Vec<String> {
    let mut modes = Vec::new();
    if pairs.iter().any(|(allowed, _)| {
        *allowed == MM_MODE_ANY
            || *allowed & (MM_MODE_2G | MM_MODE_3G | MM_MODE_4G | MM_MODE_5G) != 0
    }) {
        modes.push("auto".to_owned());
    }
    if pairs.iter().any(|(allowed, preferred)| {
        *allowed == MM_MODE_4G || (*preferred == MM_MODE_4G && *allowed & MM_MODE_5G == 0)
    }) {
        modes.push("lte".to_owned());
    }
    if pairs.iter().any(|(allowed, preferred)| {
        *allowed == MM_MODE_5G || (*preferred == MM_MODE_5G && *allowed & MM_MODE_4G == 0)
    }) {
        modes.push("nr".to_owned());
    }
    modes.sort();
    modes.dedup();
    modes
}

fn ok_envelope<T: Serialize>(data: T) -> Value {
    json!({
        "status": "ok",
        "message": "Success",
        "data": data,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modem_path_is_explicit_and_strict() {
        assert!(is_modem_path("/org/freedesktop/ModemManager1/Modem/0"));
        assert!(is_modem_path("/org/freedesktop/ModemManager1/Modem/42"));
        assert!(!is_modem_path("/org/freedesktop/ModemManager1/Modem/"));
        assert!(!is_modem_path("/org/freedesktop/ModemManager1/Modem/a"));
    }

    #[test]
    fn sim_identity_normalization_matches_device_contract() {
        assert_eq!(
            normalize_iccid("8986000000000000001F"),
            "8986000000000000001"
        );
        assert_eq!(operator_code_from_imsi("460020123456789"), "46002");
        assert_eq!(operator_code_from_imsi("001010123456789"), "001010");
    }
}
