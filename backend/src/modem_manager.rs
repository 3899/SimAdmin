//! ModemManager integration for modem, SIM, network and SMS control.

use std::collections::HashMap;
#[cfg(unix)]
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use tokio::process::Command;
use tracing::{info, warn};
use zbus::{
    zvariant::{OwnedObjectPath, OwnedValue, Value},
    Connection, Proxy,
};

use crate::{
    config::{ApnConfig, ConfigManager},
    db::Database,
};
use crate::{
    models::{
        AirplaneModeResponse, ApnContext, ApnListResponse, BandLockRequest, BandLockStatus,
        BasebandRestartResponse, BasebandRestartStep, CallInfo, CallListResponse,
        CallSettingsResponse, CellInfo, CellLocationInfo, CellLocationResponse, CellsResponse,
        DeviceInfoResponse, NetworkInfoResponse, OperatorInfo, OperatorListResponse, RadioMode,
        RadioModeResponse, ServingCell, SetApnRequest, SignalStrengthResponse, SimInfoResponse,
    },
    serial::with_serial,
    system_event::{
        codes as system_event_codes, severity as system_event_severity,
        status as system_event_status, SystemEventEmitter,
    },
};

const MM_SERVICE: &str = "org.freedesktop.ModemManager1";
const MM_ROOT_PATH: &str = "/org/freedesktop/ModemManager1";

const DBUS_PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const DBUS_OBJECT_MANAGER: &str = "org.freedesktop.DBus.ObjectManager";

const MM_MODEM: &str = "org.freedesktop.ModemManager1.Modem";
const MM_MODEM_3GPP: &str = "org.freedesktop.ModemManager1.Modem.Modem3gpp";
const MM_MODEM_SIMPLE: &str = "org.freedesktop.ModemManager1.Modem.Simple";
const MM_MESSAGING: &str = "org.freedesktop.ModemManager1.Modem.Messaging";
const MM_VOICE: &str = "org.freedesktop.ModemManager1.Modem.Voice";
const MM_CALL: &str = "org.freedesktop.ModemManager1.Call";
const MM_SIM: &str = "org.freedesktop.ModemManager1.Sim";
const MM_SMS: &str = "org.freedesktop.ModemManager1.Sms";

const MM_MODE_NONE: u32 = 0;
const MM_MODE_2G: u32 = 1 << 1;
const MM_MODE_3G: u32 = 1 << 2;
const MM_MODE_4G: u32 = 1 << 3;
const MM_MODE_5G: u32 = 1 << 4;
const MM_MODE_ANY: u32 = u32::MAX;
const MODEM_SCAN_THRESHOLD: u32 = 3;
const MODEM_RESTART_THRESHOLD: u32 = 5;
const MODEM_RECOVERY_COOLDOWN_SECS: u64 = 300;
const MODEM_DISCOVERY_TIMEOUT_SECS: u64 = 5;
const MODEM_DISCOVERY_FAILURE_CACHE_SECS: u64 = 30;
const OPERATOR_SCAN_REQUEST_TIMEOUT_SECS: u64 = 45;
const OPERATOR_SCAN_CACHE_POLL_SECS: u64 = 20;
const NETWORK_REGISTER_TIMEOUT_SECS: u64 = 45;
const SEARCHING_REGISTER_THRESHOLD: u32 = 4;
const SEARCHING_RADIO_RESET_THRESHOLD: u32 = 8;
const DATA_CONNECT_RETRY_COOLDOWN_SECS: u64 = 120;
const NM_CREATED_PROFILE_NAME: &str = "simadmin-modem";
const MM_MODEM_STATE_REGISTERED: i32 = 8;
const MM_MODEM_STATE_DISCONNECTING: i32 = 9;
const MM_MODEM_STATE_CONNECTING: i32 = 10;
const MM_MODEM_STATE_CONNECTED: i32 = 11;
const MM_MODEM_PORT_TYPE_AT: u32 = 3;
const MM_MODEM_PORT_TYPE_QMI: u32 = 6;
const MM_MODEM_PORT_TYPE_MBIM: u32 = 7;
const MODEM_HELPER_COMMAND_TIMEOUT_SECS: u64 = 3;
const MODEM_AT_COMMAND_TIMEOUT_SECS: u64 = 2;
const SMSC_HELPER_FALLBACK_TIMEOUT_SECS: u64 = 4;
const SMSC_BACKGROUND_AT_TIMEOUT_SECS: u64 = 10;

#[allow(dead_code)]
static SMSC_BACKGROUND_RUNNING: AtomicBool = AtomicBool::new(false);
static SIM_DETAILS_BACKGROUND_RUNNING: AtomicBool = AtomicBool::new(false);

type InterfaceProperties = HashMap<String, OwnedValue>;
type ManagedObjects = HashMap<OwnedObjectPath, HashMap<String, InterfaceProperties>>;

static MODEM_DISCOVERY_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static MODEM_DISCOVERY_FAILURE: std::sync::Mutex<Option<(Instant, String)>> =
    std::sync::Mutex::new(None);
static BASEBAND_RESTART_STEPS: std::sync::Mutex<Vec<BasebandRestartStep>> =
    std::sync::Mutex::new(Vec::new());
static BASEBAND_RESTART_RUNNING: AtomicBool = AtomicBool::new(false);
static BASEBAND_RESTART_REGISTRATION: std::sync::Mutex<Option<String>> =
    std::sync::Mutex::new(None);

#[derive(Debug, Clone, Default)]
struct SimpleConnectSettings {
    apn: Option<String>,
    user: Option<String>,
    password: Option<String>,
    ip_type: Option<u32>,
    allowed_auth: Option<u32>,
    source: Option<&'static str>,
}

impl SimpleConnectSettings {
    fn has_apn(&self) -> bool {
        self.apn
            .as_deref()
            .is_some_and(|apn| !apn.trim().is_empty())
    }

    fn fill_missing_from(&mut self, other: SimpleConnectSettings) {
        if self.apn.is_none() {
            self.apn = other.apn;
            if self.apn.is_some() && self.source.is_none() {
                self.source = other.source;
            }
        }
        if self.user.is_none() {
            self.user = other.user;
        }
        if self.password.is_none() {
            self.password = other.password;
        }
        if self.ip_type.is_none() {
            self.ip_type = other.ip_type;
        }
        if self.allowed_auth.is_none() {
            self.allowed_auth = other.allowed_auth;
        }
    }
}

fn non_empty_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn property_string(props: &InterfaceProperties, key: &str) -> Option<String> {
    props
        .get(key)
        .map(extract_string)
        .and_then(non_empty_string)
}

fn apn_protocol_to_mm_ip_type(protocol: &str) -> Option<u32> {
    match protocol.trim().to_ascii_lowercase().as_str() {
        "" => None,
        "ip" | "ipv4" => Some(1),
        "ipv6" => Some(2),
        "dual" | "ipv4v6" => Some(3),
        _ => Some(3),
    }
}

fn apn_config_to_simple_connect_settings(config: &ApnConfig) -> SimpleConnectSettings {
    let apn = non_empty_string(config.apn.clone());
    let source = apn.as_ref().map(|_| "config");
    let mut settings = SimpleConnectSettings {
        apn,
        user: non_empty_string(config.username.clone()),
        password: non_empty_string(config.password.clone()),
        ip_type: apn_protocol_to_mm_ip_type(&config.protocol),
        allowed_auth: None,
        source,
    };
    let auth_method = config.auth_method.trim();
    if (!auth_method.eq_ignore_ascii_case("chap")
        || settings.user.is_some()
        || settings.password.is_some())
        && !auth_method.is_empty()
    {
        settings.allowed_auth = apn_auth_method_to_mm_allowed_auth(auth_method);
    }
    settings
}

async fn get_all_properties(
    conn: &Connection,
    path: &str,
    interface: &str,
) -> zbus::Result<InterfaceProperties> {
    let proxy = Proxy::new(conn, MM_SERVICE, path, DBUS_PROPERTIES).await?;
    proxy.call("GetAll", &(interface,)).await
}

async fn get_property(
    conn: &Connection,
    path: &str,
    interface: &str,
    property: &str,
) -> zbus::Result<OwnedValue> {
    let proxy = Proxy::new(conn, MM_SERVICE, path, DBUS_PROPERTIES).await?;
    proxy.call("Get", &(interface, property)).await
}

fn extract_string(value: &OwnedValue) -> String {
    String::try_from(value.clone()).unwrap_or_default()
}

fn extract_string_or_number(value: &OwnedValue) -> String {
    if let Ok(text) = String::try_from(value.clone()) {
        return text;
    }
    if let Ok(number) = u32::try_from(value.clone()) {
        return number.to_string();
    }
    if let Ok(number) = i32::try_from(value.clone()) {
        return number.to_string();
    }
    String::new()
}

fn extract_u32(value: &OwnedValue) -> u32 {
    u32::try_from(value.clone()).unwrap_or(0)
}

fn extract_i32(value: &OwnedValue) -> i32 {
    i32::try_from(value.clone()).unwrap_or(0)
}

fn extract_bool(value: &OwnedValue) -> bool {
    bool::try_from(value.clone()).unwrap_or(false)
}

fn extract_f64(value: &OwnedValue) -> f64 {
    f64::try_from(value.clone()).unwrap_or(0.0)
}

fn extract_u32_array(value: &OwnedValue) -> Vec<u32> {
    Vec::<u32>::try_from(value.clone()).unwrap_or_default()
}

fn extract_string_list(value: &OwnedValue) -> Vec<String> {
    if let Ok(v) = Vec::<String>::try_from(value.clone()) {
        return v;
    }
    let s = extract_string(value);
    if s.is_empty() {
        Vec::new()
    } else {
        vec![s]
    }
}

fn first_quoted_value(text: &str) -> Option<String> {
    let start = text.find('"')?;
    let tail = &text[start + 1..];
    let end = tail.find('"')?;
    Some(tail[..end].to_string())
}

fn is_plausible_smsc(value: &str) -> bool {
    let value = value.trim();
    let digits = value.strip_prefix('+').unwrap_or(value);
    (4..=20).contains(&digits.len())
        && digits.chars().all(|c| c.is_ascii_digit())
        && digits.chars().any(|c| c != '0')
}

fn normalize_smsc(value: &str) -> String {
    let value = value
        .trim()
        .trim_matches(|c| matches!(c, '"' | '\'' | ',' | ';'))
        .trim();
    if is_plausible_smsc(value) {
        value.to_string()
    } else {
        String::new()
    }
}

fn push_smsc_candidate(candidates: &mut Vec<String>, candidate: &str) {
    let normalized = normalize_smsc(candidate);
    if !normalized.is_empty() && !candidates.contains(&normalized) {
        candidates.push(normalized);
    }
}

fn extract_smsc_from_text(text: &str) -> String {
    let mut plus_candidates = Vec::new();
    let mut numeric_candidates = Vec::new();
    let mut chars = text.char_indices().peekable();

    while let Some((start, ch)) = chars.next() {
        if ch != '+' && !ch.is_ascii_digit() {
            continue;
        }
        let mut end = start + ch.len_utf8();
        while let Some(&(idx, next)) = chars.peek() {
            if next.is_ascii_digit() {
                chars.next();
                end = idx + next.len_utf8();
            } else {
                break;
            }
        }
        if ch == '+' {
            push_smsc_candidate(&mut plus_candidates, &text[start..end]);
        } else {
            push_smsc_candidate(&mut numeric_candidates, &text[start..end]);
        }
    }

    plus_candidates
        .into_iter()
        .next()
        .or_else(|| numeric_candidates.into_iter().next())
        .unwrap_or_default()
}

fn parse_smsc_from_at_output(output: &str) -> String {
    for line in output.lines() {
        let Some(start) = line.find("+CSCA:") else {
            continue;
        };
        let trimmed = line[start..].trim();
        if let Some(value) = first_quoted_value(trimmed) {
            let normalized = normalize_smsc(&value);
            if !normalized.is_empty() {
                return normalized;
            }
        }
        let value = trimmed
            .trim_start_matches("+CSCA:")
            .split(',')
            .next()
            .unwrap_or_default();
        let normalized = normalize_smsc(value);
        if !normalized.is_empty() {
            return normalized;
        }
    }
    extract_smsc_from_text(output)
}

fn extract_smsc_property(props: &HashMap<String, OwnedValue>) -> String {
    for key in [
        "SMSC",
        "Smsc",
        "SmsCenter",
        "DefaultSmsc",
        "DefaultSmsCenter",
    ] {
        if let Some(value) = props.get(key) {
            let smsc = normalize_smsc(&extract_string(value));
            if !smsc.is_empty() {
                return smsc;
            }
        }
    }
    String::new()
}

// ── EF_SMSP (AT+CRSM) SMSC 解析 ─────────────────────────────────────────

/// 从十六进制字符串解析字节数组。
fn decode_hex(hex: &str) -> Vec<u8> {
    let hex = hex.trim();
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    let mut chars = hex.chars();
    while let (Some(hi), Some(lo)) = (chars.next(), chars.next()) {
        if let (Some(h), Some(l)) = (hi.to_digit(16), lo.to_digit(16)) {
            bytes.push((h as u8) << 4 | l as u8);
        } else {
            break;
        }
    }
    bytes
}

/// 从 +CRSM GET RESPONSE 的 FCP 数据中提取线性定长文件的记录长度。
///
/// 输入示例: `+CRSM: 97,12,"62198205422100280583026F428A01"`
/// FCP 中 tag 0x82 (File Descriptor) 长度 5 时格式为:
///   [0x42(线性定长)] [数据编码] [记录长度高] [记录长度低] [记录数]
fn parse_crsm_fcp_record_length(output: &str) -> usize {
    for line in output.lines() {
        let trimmed = line.trim();
        // 寻找 +CRSM: 行
        let crsm_start = match trimmed.find("+CRSM:") {
            Some(pos) => pos,
            None => continue,
        };
        let data_part = &trimmed[crsm_start + "+CRSM:".len()..];
        // 提取引号中的十六进制数据
        let hex_data = match first_quoted_value(data_part) {
            Some(hex) => hex,
            None => {
                // 无引号时取逗号分隔的第三个字段
                let fields: Vec<&str> = data_part.split(',').collect();
                match fields.get(2) {
                    Some(field) => field.trim().trim_matches('"').to_string(),
                    None => continue,
                }
            }
        };
        let fcp = decode_hex(&hex_data);
        // 跳过 FCP 模板外层 (tag 0x62 + length)，进入内层 TLV
        let inner = if fcp.len() >= 2 && fcp[0] == 0x62 {
            let inner_len = fcp[1] as usize;
            if 2 + inner_len <= fcp.len() {
                &fcp[2..2 + inner_len]
            } else {
                &fcp[2..]
            }
        } else {
            &fcp[..]
        };
        // 在内层 TLV 中查找 tag 0x82 (File Descriptor)
        let mut i = 0;
        while i + 1 < inner.len() {
            let tag = inner[i];
            let len = inner[i + 1] as usize;
            if tag == 0x82 && len >= 5 && i + 2 + len <= inner.len() {
                // inner[i+2] = file descriptor byte (0x42 = linear fixed)
                // inner[i+3] = data coding byte
                // inner[i+4..i+6] = record length (big-endian u16)
                let record_len = ((inner[i + 4] as usize) << 8) | (inner[i + 5] as usize);
                if record_len >= 28 && record_len <= 256 {
                    return record_len;
                }
            }
            i += 2 + len;
        }
    }
    0
}

/// 从 BCD 半字节交换编码的字节中解码电话号码。
///
/// BCD 编码: 每个字节的低 4 位在前、高 4 位在后, 0xF 为填充。
/// 示例: [0x68, 0x31, 0x08, 0x10, 0x05, 0xF0] → "8613800100500"
fn decode_bcd_digits(bytes: &[u8]) -> String {
    let mut digits = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        let lo = byte & 0x0F;
        let hi = (byte >> 4) & 0x0F;
        if lo <= 9 {
            digits.push((b'0' + lo) as char);
        }
        if hi <= 9 {
            digits.push((b'0' + hi) as char);
        }
    }
    digits
}

/// 从 EF_SMSP 记录的 TS-SCA 字段解码 SMSC 地址。
///
/// TS-SCA 格式 (12 字节):
///   [长度] [类型 (0x91=国际/0x81=国内)] [BCD 编码号码 (最多 10 字节)]
///   长度 = 后续有效字节数 (含类型字节)
///   长度为 0x00 或 0xFF 表示未设置。
fn decode_smsc_from_ts_sca(sca: &[u8]) -> String {
    if sca.is_empty() {
        return String::new();
    }
    let len = sca[0] as usize;
    if len == 0 || len == 0xFF || len < 2 || 1 + len > sca.len() {
        return String::new();
    }
    let type_byte = sca[1];
    let digit_bytes = &sca[2..1 + len];
    let number = decode_bcd_digits(digit_bytes);
    if number.is_empty() || number.chars().all(|c| c == '0') {
        return String::new();
    }
    let formatted = if type_byte == 0x91 {
        format!("+{number}")
    } else {
        number
    };
    normalize_smsc(&formatted)
}

/// 从 +CRSM READ RECORD 响应中解析 SMSC 地址。
///
/// 输入示例: `+CRSM: 144,0,"FFFF...07916831081005F0FFFFFF"`
/// EF_SMSP 记录结构 (3GPP TS 31.102 4.2.27):
///   [Alpha ID: record_len - 28 字节]
///   [参数指示: 1 字节]
///   [TP-DA: 12 字节]
///   [TS-SCA: 12 字节]  ← SMSC 在这里
///   [TP-PID: 1 字节]
///   [TP-DCS: 1 字节]
///   [TP-VP: 1 字节]
fn parse_smsc_from_crsm_record(output: &str, record_len: usize) -> String {
    for line in output.lines() {
        let trimmed = line.trim();
        let crsm_start = match trimmed.find("+CRSM:") {
            Some(pos) => pos,
            None => continue,
        };
        let data_part = &trimmed[crsm_start + "+CRSM:".len()..];
        let fields: Vec<&str> = data_part.splitn(3, ',').collect();
        // 检查 SW1=144(0x90) 表示成功
        let sw1: u32 = fields
            .first()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        if sw1 != 144 && sw1 != 145 {
            continue;
        }
        let hex_data = match fields.get(2) {
            Some(field) => field.trim().trim_matches('"').to_string(),
            None => continue,
        };
        let record = decode_hex(&hex_data);
        if record.len() < record_len {
            continue;
        }
        // TS-SCA 起始位置: record_len - 28(固定部分) + 1(参数指示) + 12(TP-DA) = record_len - 15
        let sca_offset = record_len - 15;
        if sca_offset + 12 > record.len() {
            continue;
        }
        let smsc = decode_smsc_from_ts_sca(&record[sca_offset..sca_offset + 12]);
        if !smsc.is_empty() {
            return smsc;
        }
    }
    String::new()
}

fn is_plausible_phone_number(value: &str) -> bool {
    let value = value.trim();
    let digits = value.strip_prefix('+').unwrap_or(value);
    (4..=20).contains(&digits.len())
        && digits.chars().all(|c| c.is_ascii_digit())
        && digits.chars().any(|c| c != '0')
}

fn normalize_phone_number(value: &str) -> String {
    let value = value
        .trim()
        .trim_matches(|c| matches!(c, '"' | '\'' | ',' | ';'))
        .trim()
        .strip_prefix("tel:")
        .unwrap_or_else(|| value.trim());

    let mut normalized = String::new();
    for ch in value.chars() {
        if ch == '+' && normalized.is_empty() {
            normalized.push(ch);
        } else if ch.is_ascii_digit() {
            normalized.push(ch);
        }
    }

    if is_plausible_phone_number(&normalized) {
        normalized
    } else {
        String::new()
    }
}

fn push_phone_number_candidate(candidates: &mut Vec<String>, candidate: &str) {
    let normalized = normalize_phone_number(candidate);
    if !normalized.is_empty() && !candidates.contains(&normalized) {
        candidates.push(normalized);
    }
}

fn normalize_phone_numbers(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut phone_numbers = Vec::new();
    for value in values {
        push_phone_number_candidate(&mut phone_numbers, &value);
    }
    phone_numbers
}

fn extract_quoted_values(text: &str) -> Vec<String> {
    let mut values = Vec::new();
    for quote in ['"', '\''] {
        let mut rest = text;
        while let Some(start) = rest.find(quote) {
            rest = &rest[start + quote.len_utf8()..];
            let Some(end) = rest.find(quote) else {
                break;
            };
            values.push(rest[..end].to_string());
            rest = &rest[end + quote.len_utf8()..];
        }
    }
    values
}

fn split_at_response_fields(data: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for ch in data.chars() {
        if ch == '"' {
            in_quotes = !in_quotes;
            current.push(ch);
        } else if ch == ',' && !in_quotes {
            fields.push(current.trim().to_string());
            current.clear();
        } else {
            current.push(ch);
        }
    }
    fields.push(current.trim().to_string());
    fields
}

fn parse_own_numbers_from_at_output(output: &str) -> Vec<String> {
    let mut phone_numbers = Vec::new();
    for line in output.lines() {
        let Some(start) = line.find("+CNUM:") else {
            continue;
        };
        let data = &line[start + "+CNUM:".len()..];
        let fields = split_at_response_fields(data);
        if let Some(number) = fields.get(1) {
            push_phone_number_candidate(&mut phone_numbers, number);
        }
    }
    phone_numbers
}

fn collect_phone_number_candidates_from_line(candidates: &mut Vec<String>, line: &str) {
    for value in extract_quoted_values(line) {
        push_phone_number_candidate(candidates, &value);
    }

    if let Some((_, value)) = line.split_once(':') {
        for token in value.split(|c: char| c.is_whitespace() || matches!(c, ',' | ';')) {
            push_phone_number_candidate(candidates, token);
        }
    }
}

fn parse_own_numbers_from_labeled_text(output: &str) -> Vec<String> {
    let mut phone_numbers = Vec::new();
    let mut phone_section = false;

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            phone_section = false;
            continue;
        }

        let lower = trimmed.to_ascii_lowercase();
        let is_phone_label = lower.contains("msisdn")
            || lower.contains("own number")
            || lower.contains("phone number")
            || lower.contains("telephone number");

        if is_phone_label {
            collect_phone_number_candidates_from_line(&mut phone_numbers, trimmed);
            phone_section = lower.contains("numbers");
            continue;
        }

        if phone_section && (trimmed.starts_with('[') || trimmed.starts_with('-')) {
            collect_phone_number_candidates_from_line(&mut phone_numbers, trimmed);
            continue;
        }

        phone_section = false;
    }

    phone_numbers
}

fn extract_own_numbers_property(props: &HashMap<String, OwnedValue>) -> Vec<String> {
    let mut phone_numbers = Vec::new();
    for key in [
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
        if let Some(value) = props.get(key) {
            for number in extract_string_list(value) {
                push_phone_number_candidate(&mut phone_numbers, &number);
            }
        }
    }
    phone_numbers
}

fn operator_code_from_imsi(imsi: &str) -> String {
    let digits = imsi.trim();
    if digits.len() < 5 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return String::new();
    }

    // Mainland China MCC 460 uses two-digit MNCs. Using six IMSI digits would
    // mislabel China Mobile 46002 as MNC 027/026/etc.
    if digits.starts_with("460") {
        return digits[..5].to_string();
    }

    if digits.len() >= 6 {
        digits[..6].to_string()
    } else {
        String::new()
    }
}

#[derive(Debug, Clone, Default)]
pub struct SimIdentity {
    pub iccid: String,
    pub imsi: String,
    pub operator_id: String,
}

fn sim_identity_keys(identity: &SimIdentity) -> Vec<String> {
    let mut keys = Vec::new();
    if !identity.iccid.is_empty() {
        keys.push(format!("iccid:{}", identity.iccid));
    }
    if !identity.imsi.is_empty() {
        keys.push(format!("imsi:{}", identity.imsi));
    }
    if !identity.operator_id.is_empty() {
        keys.push(format!("operator:{}", identity.operator_id));
    }
    keys
}

fn smsc_identity_keys(identity: &SimIdentity) -> Vec<String> {
    if !identity.iccid.is_empty() {
        vec![format!("iccid:{}", identity.iccid)]
    } else {
        sim_identity_keys(identity)
    }
}

fn own_number_identity_key(identity: &SimIdentity) -> Option<String> {
    if identity.iccid.is_empty() {
        None
    } else {
        Some(format!("iccid:{}", identity.iccid))
    }
}

fn sms_storage_identity_key(identity: &SimIdentity) -> Option<String> {
    own_number_identity_key(identity)
}

pub fn cache_smsc_for_identity(
    db: &Database,
    identity: &SimIdentity,
    sms_center: &str,
    source: &str,
) {
    let sms_center = normalize_smsc(sms_center);
    let Some(identity_key) = smsc_identity_keys(identity).into_iter().next() else {
        return;
    };
    let _ = db.upsert_smsc_cache(
        &identity_key,
        &identity.iccid,
        &identity.imsi,
        &identity.operator_id,
        &sms_center,
        source,
    );
}

fn smsc_cache_entry_for_identity(
    db: &Database,
    identity: &SimIdentity,
) -> Option<crate::db::SmscCacheEntry> {
    let keys = smsc_identity_keys(identity);
    if keys.is_empty() {
        return None;
    }
    db.get_smsc_cache(&keys).ok().flatten()
}

#[allow(dead_code)]
fn cached_smsc_for_identity(db: &Database, identity: &SimIdentity) -> String {
    smsc_cache_entry_for_identity(db, identity)
        .map(|entry| normalize_smsc(&entry.sms_center))
        .unwrap_or_default()
}

pub fn cache_own_numbers_for_identity(
    db: &Database,
    identity: &SimIdentity,
    phone_numbers: &[String],
    source: &str,
) {
    let phone_numbers = normalize_phone_numbers(phone_numbers.iter().cloned());
    let Some(identity_key) = own_number_identity_key(identity) else {
        return;
    };
    let _ = db.upsert_own_number_cache(
        &identity_key,
        &identity.iccid,
        &identity.imsi,
        &identity.operator_id,
        &phone_numbers,
        source,
    );
}

fn own_number_cache_entry_for_identity(
    db: &Database,
    identity: &SimIdentity,
) -> Option<crate::db::OwnNumberCacheEntry> {
    let Some(identity_key) = own_number_identity_key(identity) else {
        return None;
    };
    db.get_own_number_cache(&[identity_key]).ok().flatten()
}

#[allow(dead_code)]
fn cached_own_numbers_for_identity(db: &Database, identity: &SimIdentity) -> Vec<String> {
    own_number_cache_entry_for_identity(db, identity)
        .map(|entry| normalize_phone_numbers(entry.phone_numbers))
        .unwrap_or_default()
}

fn sms_storage_cache_entry_for_identity(
    db: &Database,
    identity: &SimIdentity,
) -> Option<crate::db::SmsStorageCacheEntry> {
    let Some(identity_key) = sms_storage_identity_key(identity) else {
        return None;
    };
    db.get_sms_storage_cache(&[identity_key]).ok().flatten()
}

/// 缓存的存储条目若无 total（此前抓取失败写入的 "empty" 占位），视为不完整。
/// 仅用于后台刷新流程内部的重试判断；不影响 sim_details_cache_missing 的
/// 自动触发语义（空占位在 ICCID 变化前不重复触发探测）。
fn sms_storage_cache_incomplete(db: &Database, identity: &SimIdentity) -> bool {
    sms_storage_cache_entry_for_identity(db, identity)
        .map(|entry| entry.sms_total.is_none())
        .unwrap_or(true)
}

fn cache_sms_storage_for_identity(
    db: &Database,
    identity: &SimIdentity,
    sms_used: Option<u32>,
    sms_total: Option<u32>,
    source: &str,
) {
    let Some(identity_key) = sms_storage_identity_key(identity) else {
        return;
    };
    let _ = db.upsert_sms_storage_cache(
        &identity_key,
        &identity.iccid,
        &identity.imsi,
        &identity.operator_id,
        sms_used,
        sms_total,
        source,
    );
}

pub fn sim_details_cache_missing(db: &Database, identity: &SimIdentity) -> bool {
    if identity.iccid.is_empty() {
        return false;
    }
    own_number_cache_entry_for_identity(db, identity).is_none()
        || smsc_cache_entry_for_identity(db, identity).is_none()
        || sms_storage_cache_entry_for_identity(db, identity).is_none()
}

fn extract_mode_pairs(value: &OwnedValue) -> Vec<(u32, u32)> {
    Vec::<(u32, u32)>::try_from(value.clone()).unwrap_or_default()
}

fn parse_hex_u32(value: &str) -> u32 {
    u32::from_str_radix(value.trim(), 16).unwrap_or(0)
}

fn parse_cell_metric(value: Option<&OwnedValue>) -> String {
    value
        .map(extract_f64)
        .map(|metric| format!("{:.0}", metric * 100.0))
        .unwrap_or_default()
}

/// ModemManager GetCellInfo 各家驱动键名不一，做多键回退以利列表与基站定位解析。
fn detect_cell_tech(cell: &HashMap<String, OwnedValue>) -> &'static str {
    let looks_nr_key = cell.keys().any(|k| {
        matches!(
            k.as_str(),
            "ssb-frequency" | "physical-cell-id-nr" | "ss-rsrp"
        )
    });
    if cell.contains_key("nrarfcn") || cell.contains_key("nr-arfcn") || looks_nr_key {
        "nr"
    } else if cell.contains_key("earfcn")
        || cell.contains_key("lte-arfcn")
        || cell.contains_key("dl-earfcn")
        || cell.contains_key("dl_earfcn")
    {
        "lte"
    } else if cell.contains_key("frequency-fdd-dl") || cell.contains_key("frequency-tdd") {
        "umts"
    } else if cell.contains_key("arfcn") {
        "lte"
    } else {
        "gsm"
    }
}

fn cell_pci_string(cell: &HashMap<String, OwnedValue>) -> String {
    for key in [
        "physical-ci",
        "physical-cell-id",
        "physical-cell-id-nr",
        "phys-cell-id",
        "pci",
        "base-station-id",
        "nr-physical-cell-id",
    ] {
        if let Some(v) = cell.get(key) {
            let s = extract_string_or_number(v);
            if !s.is_empty() {
                return s;
            }
        }
    }
    String::new()
}

fn first_u32_string(cell: &HashMap<String, OwnedValue>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = cell.get(*key) {
            let n = extract_u32(value);
            return Some(n.to_string());
        }
    }
    None
}

/// 国内三大运营商 + 广电；按 MCC/MNC 映射中文名。
fn china_mobile_operator_cn(mcc: &str, mnc: &str) -> Option<&'static str> {
    match (mcc.trim(), mnc.trim()) {
        ("460", "00" | "02" | "04" | "07" | "08" | "16" | "20") => Some("中国移动"),
        ("460", "01" | "06" | "09" | "10") => Some("中国联通"),
        ("460", "03" | "05" | "11" | "12") => Some("中国电信"),
        ("460", "15") => Some("中国广电"),
        _ => None,
    }
}

fn english_operator_aliases_cn(name: &str) -> Option<String> {
    let n = name.to_uppercase();
    if n.contains("CHINA MOBILE")
        || n.contains(" CMCC ")
        || n == "CMCC"
        || (n.starts_with("CHINA ") && n.contains("MOBILE"))
    {
        return Some("中国移动".into());
    }
    if n.contains("CHINA UNICOM")
        || n.contains("CHN-UNICOM")
        || n.contains("CUNICOM")
        || n.contains("CUCC")
        || n == "UNICOM"
        || (n.contains("UNICOM") && (n.contains("CHN") || n.contains("CHINA") || n.contains("460")))
    {
        return Some("中国联通".into());
    }
    if n.contains("CHINA TELECOM") || n.contains("CTCC") || n.contains("CHINATELECOM") {
        return Some("中国电信".into());
    }
    if n.contains("BROADCAST") || n.contains(" CHN-CBN") || n == "CBN" {
        return Some("中国广电".into());
    }
    None
}

/// 在当前网络 + 英文名场景下替换为三大运营商/广电的中文展示名。
fn localize_operator_display(mcc: &str, mnc: &str, name: &str) -> String {
    if let Some(cn) = china_mobile_operator_cn(mcc, mnc) {
        return cn.to_string();
    }
    if let Some(cn) = english_operator_aliases_cn(name) {
        return cn;
    }
    name.to_string()
}

fn normalize_mode(allowed: u32, preferred: u32) -> String {
    if allowed == MM_MODE_5G || (preferred == MM_MODE_5G && allowed & MM_MODE_4G == 0) {
        return "nr".to_string();
    }
    if allowed == MM_MODE_4G || (preferred == MM_MODE_4G && allowed & MM_MODE_5G == 0) {
        return "lte".to_string();
    }
    "auto".to_string()
}

fn supported_mode_labels(pairs: &[(u32, u32)]) -> Vec<String> {
    let mut modes = Vec::new();

    if pairs.iter().any(|(allowed, preferred)| {
        *allowed == MM_MODE_4G || (*preferred == MM_MODE_4G && *allowed & MM_MODE_5G == 0)
    }) {
        modes.push("lte".to_string());
    }
    if pairs.iter().any(|(allowed, preferred)| {
        *allowed == MM_MODE_5G || (*preferred == MM_MODE_5G && *allowed & MM_MODE_4G == 0)
    }) {
        modes.push("nr".to_string());
    }
    if pairs.iter().any(|(allowed, _)| {
        (*allowed & (MM_MODE_2G | MM_MODE_3G | MM_MODE_4G | MM_MODE_5G) != 0)
            || *allowed == MM_MODE_ANY
    }) {
        modes.insert(0, "auto".to_string());
    }

    modes.sort();
    modes.dedup();
    modes
}

fn choose_mode_pair(target: &RadioMode, supported: &[(u32, u32)]) -> Option<(u32, u32)> {
    match target {
        RadioMode::LteOnly => supported.iter().copied().find(|(allowed, preferred)| {
            *allowed == MM_MODE_4G && (*preferred == MM_MODE_NONE || *preferred == MM_MODE_4G)
        }),
        RadioMode::NrOnly => supported.iter().copied().find(|(allowed, preferred)| {
            *allowed == MM_MODE_5G && (*preferred == MM_MODE_NONE || *preferred == MM_MODE_5G)
        }),
        RadioMode::Auto => supported
            .iter()
            .copied()
            .find(|(allowed, _)| (*allowed & MM_MODE_4G != 0) && (*allowed & MM_MODE_5G != 0))
            .or_else(|| {
                supported
                    .iter()
                    .copied()
                    .find(|(allowed, _)| *allowed == MM_MODE_ANY)
            })
            .or_else(|| {
                supported
                    .iter()
                    .copied()
                    .find(|(allowed, _)| *allowed & MM_MODE_4G != 0)
            }),
    }
}

fn band_label(id: u32) -> String {
    match id {
        0 => "Unknown".to_string(),
        1 => "EGSM".to_string(),
        2 => "DCS".to_string(),
        3 => "PCS".to_string(),
        4 => "G850".to_string(),
        5 => "WCDMA B1".to_string(),
        6 => "WCDMA B3".to_string(),
        7 => "WCDMA B4".to_string(),
        8 => "WCDMA B6".to_string(),
        9 => "WCDMA B5".to_string(),
        10 => "WCDMA B8".to_string(),
        11 => "WCDMA B9".to_string(),
        12 => "WCDMA B2".to_string(),
        13 => "WCDMA B7".to_string(),
        14 => "WCDMA B10".to_string(),
        15 => "WCDMA B11".to_string(),
        16 => "WCDMA B12".to_string(),
        17 => "WCDMA B13".to_string(),
        18 => "WCDMA B14".to_string(),
        19 => "WCDMA B19".to_string(),
        20 => "WCDMA B20".to_string(),
        21 => "WCDMA B21".to_string(),
        22 => "WCDMA B22".to_string(),
        23 => "WCDMA B25".to_string(),
        24 => "WCDMA B26".to_string(),
        25 => "WCDMA B32".to_string(),
        // ModemManager keeps EUTRAN and NGRAN enum values aligned with band numbers:
        // EUTRAN_1 = 31, NGRAN_1 = 301.
        31..=115 => format!("LTE B{}", id - 30),
        301..=561 => format!("NR n{}", id - 300),
        _ => format!("MM Band {id}"),
    }
}

fn band_matches_tech(id: u32, tech: &str) -> bool {
    match tech {
        "nr" => (301..=561).contains(&id),
        "lte" => (31..=115).contains(&id),
        "umts" => (5..=25).contains(&id),
        "gsm" => (1..=4).contains(&id),
        _ => false,
    }
}

fn single_current_band_label(current_bands: &[u32], tech: &str) -> Option<String> {
    let matching = current_bands
        .iter()
        .copied()
        .filter(|id| band_matches_tech(*id, tech))
        .collect::<Vec<_>>();

    if matching.len() == 1 {
        return Some(band_label(matching[0]));
    }

    None
}

async fn list_modem_paths(conn: &Connection) -> zbus::Result<Vec<String>> {
    let proxy = Proxy::new(conn, MM_SERVICE, MM_ROOT_PATH, DBUS_OBJECT_MANAGER).await?;
    let managed_objects: ManagedObjects = proxy.call("GetManagedObjects", &()).await?;

    let mut modem_paths: Vec<String> = managed_objects
        .into_iter()
        .filter_map(|(path, interfaces)| {
            interfaces.contains_key(MM_MODEM).then(|| path.to_string())
        })
        .collect();
    modem_paths.sort();
    Ok(modem_paths)
}

fn no_modem_error(detail: impl Into<String>) -> zbus::Error {
    zbus::fdo::Error::Failed(detail.into()).into()
}

fn recent_modem_discovery_failure() -> Option<String> {
    let Ok(guard) = MODEM_DISCOVERY_FAILURE.lock() else {
        return None;
    };
    let Some((recorded_at, detail)) = guard.as_ref() else {
        return None;
    };
    (recorded_at.elapsed() < Duration::from_secs(MODEM_DISCOVERY_FAILURE_CACHE_SECS))
        .then(|| detail.clone())
}

fn record_modem_discovery_failure(detail: String) {
    if let Ok(mut guard) = MODEM_DISCOVERY_FAILURE.lock() {
        *guard = Some((Instant::now(), detail));
    }
}

fn clear_modem_discovery_failure() {
    if let Ok(mut guard) = MODEM_DISCOVERY_FAILURE.lock() {
        *guard = None;
    }
}

pub async fn find_modem_path(conn: &Connection) -> zbus::Result<String> {
    if let Some(path) = list_modem_paths(conn).await?.into_iter().next() {
        clear_modem_discovery_failure();
        return Ok(path);
    }
    if let Some(detail) = recent_modem_discovery_failure() {
        return Err(no_modem_error(detail));
    }

    let _guard = MODEM_DISCOVERY_LOCK.lock().await;
    if let Some(path) = list_modem_paths(conn).await?.into_iter().next() {
        clear_modem_discovery_failure();
        return Ok(path);
    }
    if let Some(detail) = recent_modem_discovery_failure() {
        return Err(no_modem_error(detail));
    }

    let scan_result = run_recovery_command("mmcli", &["--scan-modems"]).await;
    let deadline = Instant::now() + Duration::from_secs(MODEM_DISCOVERY_TIMEOUT_SECS);
    loop {
        if let Some(path) = list_modem_paths(conn).await?.into_iter().next() {
            clear_modem_discovery_failure();
            return Ok(path);
        }
        if Instant::now() >= deadline {
            let detail = match scan_result {
                Ok(ref output) => format!("No ModemManager modem found after scan: {output}"),
                Err(ref err) => format!("No ModemManager modem found; scan failed: {err}"),
            };
            record_modem_discovery_failure(detail.clone());
            return Err(no_modem_error(detail));
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn get_sim_path(conn: &Connection, modem_path: &str) -> zbus::Result<String> {
    let value = get_property(conn, modem_path, MM_MODEM, "Sim").await?;
    let path = zbus::zvariant::ObjectPath::try_from(value.clone())
        .map(|v| v.to_string())
        .unwrap_or_else(|_| extract_string(&value));
    Ok(path)
}

pub async fn current_sim_identity(conn: &Connection) -> Option<SimIdentity> {
    let modem_path = find_modem_path(conn).await.ok()?;
    let gpp_props = get_all_properties(conn, &modem_path, MM_MODEM_3GPP)
        .await
        .unwrap_or_default();
    let sim_path = get_sim_path(conn, &modem_path).await.ok()?;
    if sim_path.is_empty() || sim_path == "/" {
        return None;
    }
    let sim_props = get_all_properties(conn, &sim_path, MM_SIM)
        .await
        .unwrap_or_default();
    let iccid = crate::utils::normalize_iccid(
        &sim_props
            .get("SimIdentifier")
            .map(extract_string)
            .unwrap_or_default(),
    );
    let imsi = sim_props
        .get("Imsi")
        .map(extract_string)
        .unwrap_or_default();
    let mut operator_id = sim_props
        .get("OperatorIdentifier")
        .map(extract_string)
        .unwrap_or_default();
    if operator_id.is_empty() {
        operator_id = operator_code_from_imsi(&imsi);
    }
    if operator_id.is_empty() {
        operator_id = gpp_props
            .get("OperatorCode")
            .map(extract_string)
            .unwrap_or_default();
    }
    Some(SimIdentity {
        iccid,
        imsi,
        operator_id,
    })
}

fn mm_state_to_string(state: i32) -> &'static str {
    match state {
        -1 => "failed",
        0 => "unknown",
        1 => "initializing",
        2 => "locked",
        3 => "disabled",
        4 => "disabling",
        5 => "enabling",
        6 => "enabled",
        7 => "searching",
        8 => "registered",
        9 => "disconnecting",
        10 => "connecting",
        11 => "connected",
        _ => "unknown",
    }
}

fn mm_registration_to_string(registration: u32) -> &'static str {
    match registration {
        0 => "idle",
        1 | 6 | 9 => "registered",
        2 => "searching",
        3 => "denied",
        4 => "unknown",
        5 | 7 | 10 => "roaming",
        8 => "attached",
        _ => "unknown",
    }
}

fn mm_access_tech_to_string(tech: u32) -> String {
    if tech & (1 << 17) != 0 {
        return "nb-iot".to_string();
    }
    if tech & (1 << 16) != 0 {
        return "cat-m".to_string();
    }
    if tech & (1 << 15) != 0 {
        return "nr".to_string();
    }
    if tech & (1 << 14) != 0 {
        return "lte-advanced".to_string();
    }
    if tech & (1 << 13) != 0 {
        return "lte".to_string();
    }
    if tech & (1 << 12) != 0 {
        return "evdob".to_string();
    }
    if tech & (1 << 11) != 0 {
        return "evdoa".to_string();
    }
    if tech & (1 << 10) != 0 {
        return "evdo0".to_string();
    }
    if tech & (1 << 9) != 0 {
        return "1xrtt".to_string();
    }
    if tech & (1 << 8) != 0 {
        return "hspa+".to_string();
    }
    if tech & (1 << 7) != 0 {
        return "hspa".to_string();
    }
    if tech & (1 << 6) != 0 {
        return "hsupa".to_string();
    }
    if tech & (1 << 5) != 0 {
        return "hsdpa".to_string();
    }
    if tech & (1 << 4) != 0 {
        return "umts".to_string();
    }
    if tech & (1 << 3) != 0 {
        return "edge".to_string();
    }
    if tech & (1 << 2) != 0 {
        return "gprs".to_string();
    }
    if tech & (1 << 1) != 0 {
        return "gsm-compact".to_string();
    }
    if tech & 1 != 0 {
        return "pots".to_string();
    }
    "unknown".to_string()
}

pub async fn get_device_info_data(conn: &Connection) -> zbus::Result<DeviceInfoResponse> {
    let modem_path = find_modem_path(conn).await?;
    let modem_props = get_all_properties(conn, &modem_path, MM_MODEM).await?;

    let manufacturer = modem_props
        .get("Manufacturer")
        .map(extract_string)
        .unwrap_or_default();
    let model = modem_props
        .get("Model")
        .map(extract_string)
        .unwrap_or_default();
    let revision = modem_props.get("Revision").map(extract_string);
    let state = modem_props.get("State").map(extract_i32).unwrap_or(0);
    let imei = match get_property(conn, &modem_path, MM_MODEM_3GPP, "Imei").await {
        Ok(value) => extract_string(&value),
        Err(_) => String::new(),
    };

    Ok(DeviceInfoResponse {
        imei,
        manufacturer,
        model,
        revision,
        online: state >= 6,
        powered: state >= 3,
    })
}

#[allow(dead_code)]
async fn messaging_smsc_fallback(conn: &Connection, modem_path: &str) -> String {
    let Ok(props) = get_all_properties(conn, modem_path, MM_MESSAGING).await else {
        return String::new();
    };
    extract_smsc_property(&props)
}

#[allow(dead_code)]
async fn sms_object_smsc(conn: &Connection, sms_path: &str) -> String {
    let Ok(props) = get_all_properties(conn, sms_path, MM_SMS).await else {
        return String::new();
    };
    extract_smsc_property(&props)
}

#[allow(dead_code)]
async fn existing_sms_smsc_fallback(conn: &Connection, modem_path: &str) -> String {
    let Ok(proxy) = Proxy::new(conn, MM_SERVICE, modem_path, MM_MESSAGING).await else {
        return String::new();
    };
    let Ok(paths) = proxy.call::<_, _, Vec<OwnedObjectPath>>("List", &()).await else {
        return String::new();
    };
    for sms_path in paths.into_iter().take(30) {
        let smsc = sms_object_smsc(conn, sms_path.as_str()).await;
        if !smsc.is_empty() {
            return smsc;
        }
    }
    String::new()
}

async fn run_modem_helper_command(program: &str, args: Vec<String>) -> Result<String, String> {
    let output = tokio::time::timeout(
        Duration::from_secs(MODEM_HELPER_COMMAND_TIMEOUT_SECS),
        Command::new(program).args(args).output(),
    )
    .await
    .map_err(|_| "timeout".to_string())?
    .map_err(|err| err.to_string())?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.status.success() {
        Ok(stdout)
    } else if stderr.is_empty() {
        Err(format!("{program} exited with status {}", output.status))
    } else {
        Err(stderr)
    }
}

async fn simple_status_own_numbers_fallback(conn: &Connection, modem_path: &str) -> Vec<String> {
    let Ok(proxy) = Proxy::new(conn, MM_SERVICE, modem_path, MM_MODEM_SIMPLE).await else {
        return Vec::new();
    };
    let Ok(status) = proxy
        .call::<_, _, HashMap<String, OwnedValue>>("GetStatus", &())
        .await
    else {
        return Vec::new();
    };
    extract_own_numbers_property(&status)
}

async fn mbim_subscriber_own_numbers_fallback(conn: &Connection, modem_path: &str) -> Vec<String> {
    let Some(device) = mbim_control_device(conn, modem_path).await else {
        return Vec::new();
    };
    let args = vec![
        "-d".to_string(),
        device,
        "-p".to_string(),
        "--query-subscriber-ready-status".to_string(),
    ];
    let Ok(output) = with_serial(async { run_modem_helper_command("mbimcli", args).await }).await
    else {
        return Vec::new();
    };
    parse_own_numbers_from_labeled_text(&output)
}

async fn qmi_dms_own_numbers_fallback(conn: &Connection, modem_path: &str) -> Vec<String> {
    let Some(device) = qmi_control_device(conn, modem_path).await else {
        return Vec::new();
    };
    let args = vec![
        "-p".to_string(),
        "-d".to_string(),
        device,
        "--dms-get-msisdn".to_string(),
    ];
    let Ok(output) = with_serial(async { run_modem_helper_command("qmicli", args).await }).await
    else {
        return Vec::new();
    };
    parse_own_numbers_from_labeled_text(&output)
}

async fn qmi_atr_own_numbers_fallback(conn: &Connection, modem_path: &str) -> Vec<String> {
    let Some(device) = qmi_control_device(conn, modem_path).await else {
        return Vec::new();
    };
    let args = vec![
        "-p".to_string(),
        "-d".to_string(),
        device,
        "--atr-send=AT+CNUM".to_string(),
    ];
    let Ok(output) = with_serial(async { run_modem_helper_command("qmicli", args).await }).await
    else {
        return Vec::new();
    };
    parse_own_numbers_from_at_output(&output)
}

async fn mbim_at_tunnel_own_numbers_fallback(conn: &Connection, modem_path: &str) -> Vec<String> {
    let Some(device) = mbim_control_device(conn, modem_path).await else {
        return Vec::new();
    };
    for option in [
        "--fibocom-set-at-command=AT+CNUM",
        "--compal-query-at-command=AT+CNUM",
        "--intel-at-tunnel-set-at-command=AT+CNUM",
    ] {
        let args = vec![
            "-d".to_string(),
            device.clone(),
            "-p".to_string(),
            option.to_string(),
        ];
        let Ok(output) =
            with_serial(async { run_modem_helper_command("mbimcli", args).await }).await
        else {
            continue;
        };
        let phone_numbers = parse_own_numbers_from_at_output(&output);
        if !phone_numbers.is_empty() {
            return phone_numbers;
        }
    }
    Vec::new()
}

async fn direct_at_own_numbers_fallback(conn: &Connection) -> Vec<String> {
    let Ok(output) = with_serial(async { run_direct_at_command(conn, "AT+CNUM").await }).await
    else {
        return Vec::new();
    };
    parse_own_numbers_from_at_output(&output)
}

async fn active_protocol_own_numbers_fallback(conn: &Connection, modem_path: &str) -> Vec<String> {
    let phone_numbers = mbim_subscriber_own_numbers_fallback(conn, modem_path).await;
    if !phone_numbers.is_empty() {
        return phone_numbers;
    }
    let phone_numbers = qmi_dms_own_numbers_fallback(conn, modem_path).await;
    if !phone_numbers.is_empty() {
        return phone_numbers;
    }
    let phone_numbers = qmi_atr_own_numbers_fallback(conn, modem_path).await;
    if !phone_numbers.is_empty() {
        return phone_numbers;
    }
    let phone_numbers = mbim_at_tunnel_own_numbers_fallback(conn, modem_path).await;
    if !phone_numbers.is_empty() {
        return phone_numbers;
    }
    direct_at_own_numbers_fallback(conn).await
}

async fn mbim_sms_config_smsc_fallback(conn: &Connection, modem_path: &str) -> String {
    let Some(device) = mbim_control_device(conn, modem_path).await else {
        return String::new();
    };
    let args = vec![
        "-d".to_string(),
        device,
        "-p".to_string(),
        "--sms-query-configuration".to_string(),
    ];
    let Ok(output) = with_serial(async { run_modem_helper_command("mbimcli", args).await }).await
    else {
        return String::new();
    };
    extract_smsc_from_text(&output)
}

async fn qmi_wms_smsc_fallback(conn: &Connection, modem_path: &str) -> String {
    let Some(device) = qmi_control_device(conn, modem_path).await else {
        return String::new();
    };
    let args = vec![
        "-p".to_string(),
        "-d".to_string(),
        device,
        "--wms-get-smsc-address".to_string(),
    ];
    let Ok(output) = with_serial(async { run_modem_helper_command("qmicli", args).await }).await
    else {
        return String::new();
    };
    extract_smsc_from_text(&output)
}

async fn qmi_atr_smsc_fallback(conn: &Connection, modem_path: &str) -> String {
    let Some(device) = qmi_control_device(conn, modem_path).await else {
        return String::new();
    };
    let args = vec![
        "-p".to_string(),
        "-d".to_string(),
        device,
        "--atr-send=AT+CSCA?".to_string(),
    ];
    let Ok(output) = with_serial(async { run_modem_helper_command("qmicli", args).await }).await
    else {
        return String::new();
    };
    parse_smsc_from_at_output(&output)
}

async fn mbim_at_tunnel_smsc_fallback(conn: &Connection, modem_path: &str) -> String {
    let Some(device) = mbim_control_device(conn, modem_path).await else {
        return String::new();
    };
    for option in [
        "--fibocom-set-at-command=AT+CSCA?",
        "--compal-query-at-command=AT+CSCA?",
        "--intel-at-tunnel-set-at-command=AT+CSCA?",
    ] {
        let args = vec![
            "-d".to_string(),
            device.clone(),
            "-p".to_string(),
            option.to_string(),
        ];
        let Ok(output) =
            with_serial(async { run_modem_helper_command("mbimcli", args).await }).await
        else {
            continue;
        };
        let smsc = parse_smsc_from_at_output(&output);
        if !smsc.is_empty() {
            return smsc;
        }
    }
    String::new()
}

async fn direct_at_smsc_fallback(conn: &Connection) -> String {
    let Ok(output) = with_serial(async { run_direct_at_command(conn, "AT+CSCA?").await }).await
    else {
        return String::new();
    };
    parse_smsc_from_at_output(&output)
}

/// 通过 AT+CRSM 读取 SIM 卡 EF_SMSP 文件中的 SMSC 地址。
/// 优先使用 ModemManager Modem.Command D-Bus 接口（需要 debug 模式），
/// 直接串口作为 fallback。
async fn direct_at_ef_smsp_fallback(conn: &Connection) -> String {
    let modem_path = match find_modem_path(conn).await {
        Ok(path) => path,
        Err(err) => {
            warn!(error = %err, "EF_SMSP fallback: cannot find modem path");
            return String::new();
        }
    };

    // 步骤 1: GET RESPONSE 获取 EF_SMSP 文件信息
    let info_output = match send_at_via_modem_command(conn, &modem_path, "AT+CRSM=192,28482,0,0,15")
        .await
    {
        Ok(output) => output,
        Err(err) => {
            warn!(error = %err, "EF_SMSP fallback step 1 (Modem.Command) failed, trying direct serial");
            // 直接串口兜底
            match with_serial(async {
                run_direct_at_command_draining(conn, "AT+CRSM=192,28482,0,0,15").await
            })
            .await
            {
                Ok(output) => output,
                Err(err) => {
                    warn!(error = %err, "EF_SMSP fallback step 1 (direct serial) also failed");
                    return String::new();
                }
            }
        }
    };

    let record_len = parse_crsm_fcp_record_length(&info_output);
    if record_len == 0 {
        warn!(
            output = %info_output,
            "EF_SMSP fallback: cannot parse record length from FCP response"
        );
        return String::new();
    }
    info!(
        record_len = record_len,
        "EF_SMSP record length parsed from FCP"
    );

    // 步骤 2: READ RECORD 读取第一条记录
    let read_cmd = format!("AT+CRSM=178,28482,1,4,{record_len}");
    let record_output = match send_at_via_modem_command(conn, &modem_path, &read_cmd).await {
        Ok(output) => output,
        Err(err) => {
            warn!(error = %err, "EF_SMSP fallback step 2 (Modem.Command) failed, trying direct serial");
            match with_serial(async { run_direct_at_command_draining(conn, &read_cmd).await }).await
            {
                Ok(output) => output,
                Err(err) => {
                    warn!(error = %err, "EF_SMSP fallback step 2 (direct serial) also failed");
                    return String::new();
                }
            }
        }
    };

    let smsc = parse_smsc_from_crsm_record(&record_output, record_len);
    if smsc.is_empty() {
        warn!(
            output = %record_output,
            record_len = record_len,
            "EF_SMSP fallback: parsed empty SMSC from record"
        );
    }
    smsc
}

/// 通过 ModemManager D-Bus Modem.Command 发送 AT 指令。
/// 在发送前动态将 ModemManager 日志级别提权到 DEBUG，发送后恢复为 INFO，
/// 从而绕过普通模式下的 Unauthorized 错误，且不长期影响系统性能。
async fn send_at_via_modem_command(
    conn: &Connection,
    modem_path: &str,
    command: &str,
) -> Result<String, String> {
    // 动态提权到 DEBUG 模式
    let mm_proxy = Proxy::new(
        conn,
        MM_SERVICE,
        "/org/freedesktop/ModemManager1",
        "org.freedesktop.ModemManager1",
    )
    .await;
    if let Ok(proxy) = &mm_proxy {
        let _ = proxy.call::<_, _, ()>("SetLogging", &("DEBUG")).await;
    }

    let result = tokio::time::timeout(
        Duration::from_secs(SMSC_BACKGROUND_AT_TIMEOUT_SECS),
        with_serial(async {
            let proxy = Proxy::new(conn, MM_SERVICE, modem_path, MM_MODEM).await?;
            proxy
                .call::<_, _, String>(
                    "Command",
                    &(command, SMSC_BACKGROUND_AT_TIMEOUT_SECS as u32),
                )
                .await
        }),
    )
    .await;

    // 无论成功、失败或超时，立即恢复为 INFO 模式
    if let Ok(proxy) = &mm_proxy {
        let _ = proxy.call::<_, _, ()>("SetLogging", &("INFO")).await;
    }

    match result {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(err)) => Err(format!("Modem.Command error: {err}")),
        Err(_) => Err("Modem.Command timeout".to_string()),
    }
}

/// 后台异步获取 SMSC 并写入缓存。
/// 优先使用 AT+CRSM 读 EF_SMSP（可靠且快速）。
/// 需要 ModemManager 以 --debug 模式运行以支持 Modem.Command 接口。
#[allow(dead_code)]
pub async fn background_fetch_smsc(conn: &Connection, db: &Database) {
    if SMSC_BACKGROUND_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }

    let result = background_fetch_smsc_inner(conn, db).await;
    SMSC_BACKGROUND_RUNNING.store(false, Ordering::SeqCst);
    result
}

#[allow(dead_code)]
async fn background_fetch_smsc_inner(conn: &Connection, db: &Database) {
    let identity = current_sim_identity(conn).await;
    let identity = match identity {
        Some(id) if !id.iccid.is_empty() || !id.imsi.is_empty() => id,
        _ => {
            info!("Background SMSC fetch skipped: no SIM identity");
            return;
        }
    };

    // 已有缓存则跳过
    let cached = cached_smsc_for_identity(db, &identity);
    if !cached.is_empty() {
        return;
    }

    info!("Background SMSC fetch starting");

    // 方法 1: AT+CRSM 读取 EF_SMSP（最可靠）
    let smsc = direct_at_ef_smsp_fallback(conn).await;
    if !smsc.is_empty() {
        cache_smsc_for_identity(db, &identity, &smsc, "ef_smsp");
        info!(smsc = %smsc, "Background SMSC fetch succeeded via EF_SMSP");
        return;
    }

    // 方法 2: AT+CSCA? 通过 Modem.Command
    let modem_path = find_modem_path(conn).await.ok();
    if let Some(ref path) = modem_path {
        match send_at_via_modem_command(conn, path, "AT+CSCA?").await {
            Ok(output) => {
                let smsc = parse_smsc_from_at_output(&output);
                if !smsc.is_empty() {
                    cache_smsc_for_identity(db, &identity, &smsc, "background_at");
                    info!(smsc = %smsc, "Background SMSC fetch succeeded via AT+CSCA?");
                    return;
                }
            }
            Err(err) => {
                warn!(error = %err, "Background SMSC fetch: AT+CSCA? via Modem.Command failed");
            }
        }
    }

    info!("Background SMSC fetch: all methods exhausted");
}

pub async fn refresh_sim_details_background(conn: &Connection, db: &Database, force: bool) {
    if SIM_DETAILS_BACKGROUND_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }

    refresh_sim_details_background_inner(conn, db, force).await;
    SIM_DETAILS_BACKGROUND_RUNNING.store(false, Ordering::SeqCst);
}

async fn refresh_sim_details_background_inner(conn: &Connection, db: &Database, force: bool) {
    let Some(identity) = current_sim_identity(conn).await else {
        info!("SIM details refresh skipped: no SIM identity");
        return;
    };
    if identity.iccid.is_empty() && identity.imsi.is_empty() {
        info!("SIM details refresh skipped: empty SIM identity");
        return;
    }

    let modem_path = find_modem_path(conn).await.ok();

    if force || own_number_cache_entry_for_identity(db, &identity).is_none() {
        let mut phone_numbers = Vec::new();
        if let Some(path) = modem_path.as_deref() {
            phone_numbers = simple_status_own_numbers_fallback(conn, path).await;
            if phone_numbers.is_empty() {
                phone_numbers = active_protocol_own_numbers_fallback(conn, path).await;
            }
        }
        if phone_numbers.is_empty() {
            phone_numbers = direct_at_own_numbers_fallback(conn).await;
        }
        phone_numbers = normalize_phone_numbers(phone_numbers);
        let source = if phone_numbers.is_empty() {
            "empty"
        } else {
            "background"
        };
        cache_own_numbers_for_identity(db, &identity, &phone_numbers, source);
    }

    if force || smsc_cache_entry_for_identity(db, &identity).is_none() {
        let mut sms_center = direct_at_ef_smsp_fallback(conn).await;
        let mut source = "ef_smsp";
        if sms_center.is_empty() {
            if let Some(path) = modem_path.as_deref() {
                if let Ok(output) = send_at_via_modem_command(conn, path, "AT+CSCA?").await {
                    sms_center = parse_smsc_from_at_output(&output);
                    source = "background_at";
                }
            }
        }
        if sms_center.is_empty() {
            if let Some(path) = modem_path.as_deref() {
                sms_center = active_protocol_smsc_fallback(conn, path).await;
                source = "protocol";
            }
        }
        if sms_center.is_empty() {
            source = "empty";
        }
        cache_smsc_for_identity(db, &identity, &sms_center, source);
    }

    if force || sms_storage_cache_incomplete(db, &identity) {
        let mut storage = None;
        if let Some(path) = modem_path.as_deref() {
            if let Ok(output) = send_at_via_modem_command(conn, path, "AT+CPMS?").await {
                storage = parse_sms_storage_info(&output);
            }
        }
        match storage {
            Some((used, total)) => {
                cache_sms_storage_for_identity(db, &identity, Some(used), Some(total), "cpms");
            }
            None => {
                cache_sms_storage_for_identity(db, &identity, None, None, "empty");
            }
        }
    }
}

async fn modem_command_smsc_fallback(conn: &Connection, modem_path: &str) -> String {
    let result = tokio::time::timeout(
        Duration::from_secs(MODEM_AT_COMMAND_TIMEOUT_SECS),
        with_serial(async {
            let proxy = Proxy::new(conn, MM_SERVICE, modem_path, MM_MODEM).await?;
            proxy
                .call::<_, _, String>(
                    "Command",
                    &("AT+CSCA?", MODEM_AT_COMMAND_TIMEOUT_SECS as u32),
                )
                .await
        }),
    )
    .await;

    match result {
        Ok(Ok(output)) => parse_smsc_from_at_output(&output),
        _ => String::new(),
    }
}

async fn helper_protocol_smsc_fallback(conn: &Connection, modem_path: &str) -> String {
    let smsc = mbim_sms_config_smsc_fallback(conn, modem_path).await;
    if !smsc.is_empty() {
        return smsc;
    }
    let smsc = qmi_wms_smsc_fallback(conn, modem_path).await;
    if !smsc.is_empty() {
        return smsc;
    }
    let smsc = qmi_atr_smsc_fallback(conn, modem_path).await;
    if !smsc.is_empty() {
        return smsc;
    }
    let smsc = mbim_at_tunnel_smsc_fallback(conn, modem_path).await;
    if !smsc.is_empty() {
        return smsc;
    }
    direct_at_smsc_fallback(conn).await
}

async fn active_protocol_smsc_fallback(conn: &Connection, modem_path: &str) -> String {
    let smsc = modem_command_smsc_fallback(conn, modem_path).await;
    if !smsc.is_empty() {
        return smsc;
    }

    tokio::time::timeout(
        Duration::from_secs(SMSC_HELPER_FALLBACK_TIMEOUT_SECS),
        helper_protocol_smsc_fallback(conn, modem_path),
    )
    .await
    .unwrap_or_default()
}

fn parse_sms_storage_count(field: &str) -> Option<u32> {
    let cleaned: String = field
        .trim_matches('"')
        .trim_matches('\'')
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    cleaned.parse::<u32>().ok()
}

/// 解析 AT+CPMS? 回复中的短信存储用量。
/// 优先取 SIM 卡存储（"SM"）；部分模组默认存储为模组内存（"ME"），
/// 回复中不含 "SM" 三元组，此时回退取第一个有效存储。
fn parse_sms_storage_info(at_output: &str) -> Option<(u32, u32)> {
    let pos = at_output.find("+CPMS:")?;
    let line = &at_output[pos + 6..];
    let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
    let mut fallback = None;
    for chunk in parts.chunks(3) {
        if chunk.len() >= 3 {
            let mem = chunk[0].trim_matches('"').trim_matches('\'');
            let used = parse_sms_storage_count(chunk[1]);
            let total = parse_sms_storage_count(chunk[2]);
            if let (Some(u), Some(t)) = (used, total) {
                if mem == "SM" {
                    return Some((u, t));
                }
                if fallback.is_none() {
                    fallback = Some((u, t));
                }
            }
        }
    }
    fallback
}

pub async fn get_sim_info_data_with_cache(
    conn: &Connection,
    db: Option<&Database>,
) -> zbus::Result<SimInfoResponse> {
    let modem_path = find_modem_path(conn).await?;
    let modem_props = get_all_properties(conn, &modem_path, MM_MODEM).await?;
    let gpp_props = get_all_properties(conn, &modem_path, MM_MODEM_3GPP).await?;
    let sim_path = get_sim_path(conn, &modem_path).await?;

    if sim_path.is_empty() || sim_path == "/" {
        return Ok(SimInfoResponse {
            present: false,
            ..Default::default()
        });
    }

    let sim_props = get_all_properties(conn, &sim_path, MM_SIM).await?;
    let iccid = crate::utils::normalize_iccid(
        &sim_props
            .get("SimIdentifier")
            .map(extract_string)
            .unwrap_or_default(),
    );
    let imsi = sim_props
        .get("Imsi")
        .map(extract_string)
        .unwrap_or_default();

    let mut operator_id = sim_props
        .get("OperatorIdentifier")
        .map(extract_string)
        .unwrap_or_default();
    if operator_id.is_empty() {
        operator_id = operator_code_from_imsi(&imsi);
    }
    if operator_id.is_empty() {
        operator_id = gpp_props
            .get("OperatorCode")
            .map(extract_string)
            .unwrap_or_default();
    }
    let identity = SimIdentity {
        iccid: iccid.clone(),
        imsi: imsi.clone(),
        operator_id: operator_id.clone(),
    };
    let (mcc, mnc) = split_operator_code(&operator_id);

    let mut phone_number_is_manual = false;
    let mut sms_center_is_manual = false;

    let mut phone_numbers = extract_own_numbers_property(&sim_props);
    if phone_numbers.is_empty() {
        phone_numbers = extract_own_numbers_property(&modem_props);
    }
    if phone_numbers.is_empty() {
        phone_numbers = extract_own_numbers_property(&gpp_props);
    }
    if !phone_numbers.is_empty() {
        if let Some(db) = db {
            cache_own_numbers_for_identity(db, &identity, &phone_numbers, "dbus");
        }
    }
    if phone_numbers.is_empty() {
        if let Some(db) = db {
            if let Some(entry) = own_number_cache_entry_for_identity(db, &identity) {
                phone_numbers = normalize_phone_numbers(entry.phone_numbers);
                if entry.source == "manual" {
                    phone_number_is_manual = true;
                }
            }
        }
    }
    phone_numbers.sort();
    phone_numbers.dedup();

    let mut sms_center = extract_smsc_property(&sim_props);
    if !sms_center.is_empty() {
        if let Some(db) = db {
            cache_smsc_for_identity(db, &identity, &sms_center, "dbus");
        }
    }
    if sms_center.is_empty() {
        if let Some(db) = db {
            if let Some(entry) = smsc_cache_entry_for_identity(db, &identity) {
                sms_center = normalize_smsc(&entry.sms_center);
                if entry.source == "manual" {
                    sms_center_is_manual = true;
                }
            }
        }
    }

    // --- 新增诊断属性提取 ---
    let sim_type_u = sim_props.get("SimType").map(extract_u32).unwrap_or(0);
    let sim_type = match sim_type_u {
        1 => "physical".to_string(),
        2 => "esim".to_string(),
        _ => "unknown".to_string(),
    };

    let esim_status_u = sim_props.get("EsimStatus").map(extract_u32).unwrap_or(0);
    let esim_status = match esim_status_u {
        1 => "none".to_string(),
        2 => "no-profiles".to_string(),
        3 => "with-profiles".to_string(),
        _ => "unknown".to_string(),
    };

    let active = sim_props.get("Active").map(extract_bool).unwrap_or(false);
    let operator_name = sim_props
        .get("OperatorName")
        .map(extract_string)
        .unwrap_or_default();

    let registered_operator_name = gpp_props
        .get("OperatorName")
        .map(extract_string)
        .unwrap_or_default();
    let registered_operator_code = gpp_props
        .get("OperatorCode")
        .map(extract_string)
        .unwrap_or_default();

    let lock_status_u = modem_props
        .get("UnlockRequired")
        .map(extract_u32)
        .unwrap_or(0);
    let lock_status = match lock_status_u {
        1 => "none".to_string(),
        2 => "sim-pin".to_string(),
        3 => "sim-pin2".to_string(),
        4 => "sim-puk".to_string(),
        5 => "sim-puk2".to_string(),
        _ => "unknown".to_string(),
    };

    let unlock_retries = modem_props
        .get("UnlockRetries")
        .and_then(|val| HashMap::<u32, u32>::try_from(val.clone()).ok())
        .unwrap_or_default();
    let pin1_retries = unlock_retries.get(&2).cloned();
    let pin2_retries = unlock_retries.get(&3).cloned();
    let puk1_retries = unlock_retries.get(&4).cloned();
    let puk2_retries = unlock_retries.get(&5).cloned();

    let carrier_config = modem_props
        .get("CarrierConfiguration")
        .map(extract_string)
        .unwrap_or_default();
    let carrier_config_revision = modem_props
        .get("CarrierConfigurationRevision")
        .map(extract_string)
        .unwrap_or_default();

    let mut sms_used = None;
    let mut sms_total = None;
    if let Some(db) = db {
        if let Some(entry) = sms_storage_cache_entry_for_identity(db, &identity) {
            sms_used = entry.sms_used;
            sms_total = entry.sms_total;
        }
    }

    Ok(SimInfoResponse {
        present: true,
        iccid,
        imsi,
        phone_numbers,
        sms_center,
        mcc,
        mnc,
        phone_number_is_manual,
        sms_center_is_manual,
        sim_path,
        modem_path,
        sim_type,
        esim_status,
        active,
        operator_name,
        registered_operator_name,
        registered_operator_code,
        lock_status,
        pin1_retries,
        puk1_retries,
        pin2_retries,
        puk2_retries,
        carrier_config,
        carrier_config_revision,
        sms_used,
        sms_total,
    })
}

pub async fn get_network_info_data(conn: &Connection) -> zbus::Result<NetworkInfoResponse> {
    let modem_path = find_modem_path(conn).await?;
    let modem_props = get_all_properties(conn, &modem_path, MM_MODEM).await?;
    let gpp_props = get_all_properties(conn, &modem_path, MM_MODEM_3GPP).await?;

    let operator_code = gpp_props
        .get("OperatorCode")
        .map(extract_string)
        .unwrap_or_default();
    let (mcc, mnc) = if operator_code.len() >= 5 {
        (
            Some(operator_code[..3].to_string()),
            Some(operator_code[3..].to_string()),
        )
    } else {
        (None, None)
    };

    let signal_strength = modem_props
        .get("SignalQuality")
        .and_then(|value| {
            <(u32, bool)>::try_from(value.clone())
                .ok()
                .map(|(q, _)| q as u8)
        })
        .unwrap_or(0);

    let op_raw = gpp_props
        .get("OperatorName")
        .map(extract_string)
        .unwrap_or_default();
    let mcc_s = mcc.clone().unwrap_or_default();
    let mnc_s = mnc.clone().unwrap_or_default();

    Ok(NetworkInfoResponse {
        operator_name: localize_operator_display(&mcc_s, &mnc_s, &op_raw),
        registration_status: mm_registration_to_string(
            gpp_props
                .get("RegistrationState")
                .map(extract_u32)
                .unwrap_or(0),
        )
        .to_string(),
        technology_preference: mm_access_tech_to_string(
            modem_props
                .get("AccessTechnologies")
                .map(extract_u32)
                .unwrap_or(0),
        ),
        signal_strength,
        mcc,
        mnc,
    })
}

fn is_get_cellinfo_unsupported(err: &zbus::Error) -> bool {
    let msg = format!("{err}");
    ((msg.contains("UnknownMethod") || msg.contains("No such method"))
        && msg.contains("GetCellInfo"))
        || msg.contains("org.freedesktop.ModemManager1.Error.Core.Unsupported")
        || (msg.contains("Cannot get cell info") && msg.contains("operation not supported"))
}

#[allow(dead_code)]
fn is_disconnect_invalid_handle(err: &zbus::Error) -> bool {
    let msg = format!("{err}");
    msg.contains("org.freedesktop.libqmi.Error.Protocol.InvalidHandle")
        || msg.contains("QMI protocol error (9): 'InvalidHandle'")
}

fn signal_metric_to_centi_db(text: &str) -> String {
    text.split_whitespace()
        .next()
        .and_then(|v| v.parse::<f64>().ok())
        .map(|v| (v * 100.0).round() as i32)
        .map(|v| v.to_string())
        .unwrap_or_default()
}

fn parse_u32_auto(text: &str) -> u32 {
    let value = text.trim().trim_start_matches("0x");
    if value.is_empty() {
        return 0;
    }
    if value
        .chars()
        .any(|c| c.is_ascii_hexdigit() && c.is_ascii_alphabetic())
    {
        u32::from_str_radix(value, 16).unwrap_or(0)
    } else {
        value.parse().unwrap_or(0)
    }
}

fn value_after_colon(line: &str) -> Option<String> {
    let (_, value) = line.split_once(':')?;
    let value = value.trim();
    if let Some(rest) = value.strip_prefix('\'') {
        let end = rest.find('\'')?;
        return Some(rest[..end].to_string());
    }
    Some(
        value
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_string(),
    )
}

fn qmicli_lte_band_label(line: &str) -> String {
    let Some(start) = line.find("E-UTRA band ") else {
        return String::new();
    };
    let rest = &line[start + "E-UTRA band ".len()..];
    let band = rest
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if band.is_empty() {
        String::new()
    } else {
        format!("B{band}")
    }
}

fn qmi_device_path(port: &str) -> String {
    if port.starts_with("/dev/") {
        port.to_string()
    } else {
        format!("/dev/{port}")
    }
}

#[cfg(unix)]
fn qmi_device_exists(path: &str) -> bool {
    fs::metadata(path).is_ok()
}

#[cfg(not(unix))]
fn qmi_device_exists(_path: &str) -> bool {
    false
}

#[cfg(unix)]
fn find_qmi_device_path() -> Option<String> {
    let mut candidates = vec!["/dev/wwan0qmi0".to_string()];
    if let Ok(entries) = fs::read_dir("/dev") {
        let mut qmi_ports = Vec::new();
        let mut cdc_wdm_ports = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let path = format!("/dev/{name}");
            if name.starts_with("wwan") && name.contains("qmi") {
                qmi_ports.push(path);
            } else if name.starts_with("cdc-wdm") {
                cdc_wdm_ports.push(path);
            }
        }
        qmi_ports.sort();
        cdc_wdm_ports.sort();
        candidates.extend(qmi_ports);
        candidates.extend(cdc_wdm_ports);
    }
    candidates.into_iter().find(|path| qmi_device_exists(path))
}

#[cfg(not(unix))]
fn find_qmi_device_path() -> Option<String> {
    None
}

async fn wait_for_qmi_device_path(preferred: Option<&str>, timeout: Duration) -> Option<String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(path) = preferred.filter(|path| qmi_device_exists(path)) {
            return Some(path.to_string());
        }
        if let Some(path) = find_qmi_device_path() {
            return Some(path);
        }
        if Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn qmicli_sim_power(device: &str, enabled: bool) -> Result<String, String> {
    let action = if enabled {
        "--uim-sim-power-on=1"
    } else {
        "--uim-sim-power-off=1"
    };
    let args = vec![
        "-p".to_string(),
        "-d".to_string(),
        device.to_string(),
        action.to_string(),
    ];
    run_recovery_command_owned("qmicli", &args, Duration::from_secs(20)).await
}

fn looks_like_qmi_control_port(port: &str) -> bool {
    let p = port.to_ascii_lowercase();
    p.contains("qmi") || p.contains("cdc-wdm")
}

fn looks_like_mbim_control_port(port: &str) -> bool {
    port.to_ascii_lowercase().contains("mbim")
}

fn looks_like_at_port(port: &str) -> bool {
    port.to_ascii_lowercase().contains("at")
}

fn modem_device_path(port: &str) -> String {
    if port.starts_with("/dev/") {
        port.to_string()
    } else {
        format!("/dev/{port}")
    }
}

async fn at_command_device(conn: &Connection) -> Result<String, String> {
    let modem_path = find_modem_path(conn).await.map_err(|err| err.to_string())?;
    let value = get_property(conn, &modem_path, MM_MODEM, "Ports")
        .await
        .map_err(|err| err.to_string())?;
    let ports = Vec::<(String, u32)>::try_from(value).unwrap_or_default();
    ports
        .iter()
        .find(|(_, port_type)| *port_type == MM_MODEM_PORT_TYPE_AT)
        .or_else(|| ports.iter().find(|(port, _)| looks_like_at_port(port)))
        .map(|(port, _)| modem_device_path(port))
        .ok_or_else(|| "未找到 AT 端口（例如 /dev/wwan0at0）".to_string())
}

async fn modem_ports(conn: &Connection, modem_path: &str) -> Vec<(String, u32)> {
    let Ok(value) = get_property(conn, modem_path, MM_MODEM, "Ports").await else {
        return Vec::new();
    };
    Vec::<(String, u32)>::try_from(value).unwrap_or_default()
}

async fn qmi_control_device(conn: &Connection, modem_path: &str) -> Option<String> {
    let ports = modem_ports(conn, modem_path).await;
    if let Some((port, _)) = ports
        .iter()
        .find(|(_, port_type)| *port_type == MM_MODEM_PORT_TYPE_QMI)
    {
        return Some(qmi_device_path(port));
    }

    if let Ok(value) = get_property(conn, modem_path, MM_MODEM, "PrimaryPort").await {
        let primary = extract_string(&value);
        if looks_like_qmi_control_port(&primary) {
            return Some(qmi_device_path(&primary));
        }
    }

    ports
        .iter()
        .map(|(port, _)| port)
        .find(|port| looks_like_qmi_control_port(port))
        .map(|port| qmi_device_path(port))
}

async fn mbim_control_device(conn: &Connection, modem_path: &str) -> Option<String> {
    let ports = modem_ports(conn, modem_path).await;
    if let Some((port, _)) = ports
        .iter()
        .find(|(_, port_type)| *port_type == MM_MODEM_PORT_TYPE_MBIM)
    {
        return Some(qmi_device_path(port));
    }

    if let Ok(value) = get_property(conn, modem_path, MM_MODEM, "PrimaryPort").await {
        let primary = extract_string(&value);
        if looks_like_mbim_control_port(&primary) {
            return Some(qmi_device_path(&primary));
        }
    }

    ports
        .iter()
        .map(|(port, _)| port)
        .find(|port| looks_like_mbim_control_port(port))
        .map(|port| qmi_device_path(port))
}

fn finish_qmicli_cell(
    cells: &mut Vec<CellInfo>,
    current: &mut Option<CellInfo>,
    serving_pci: &str,
) {
    let Some(mut cell) = current.take() else {
        return;
    };
    if !serving_pci.is_empty() && cell.pci == serving_pci {
        cell.is_serving = true;
    }
    cells.push(cell);
}

fn parse_qmicli_cell_location_output(output: &str) -> CellsResponse {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Section {
        Other,
        IntraLte,
        InterLte,
    }

    let mut section = Section::Other;
    let mut serving_cell = ServingCell {
        tech: "lte".to_string(),
        ..Default::default()
    };
    let mut cells = Vec::new();
    let mut current_cell: Option<CellInfo> = None;
    let mut current_earfcn = String::new();
    let mut current_band = String::new();
    let mut intra_earfcn = String::new();
    let mut intra_band = String::new();
    let mut serving_pci = String::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Intrafrequency LTE Info") {
            finish_qmicli_cell(&mut cells, &mut current_cell, &serving_pci);
            section = Section::IntraLte;
            current_earfcn.clear();
            current_band.clear();
            continue;
        }
        if trimmed.starts_with("Interfrequency LTE Info") {
            finish_qmicli_cell(&mut cells, &mut current_cell, &serving_pci);
            section = Section::InterLte;
            current_earfcn.clear();
            current_band.clear();
            continue;
        }
        if trimmed.starts_with("LTE Info Neighboring") || trimmed.starts_with("LTE Timing") {
            finish_qmicli_cell(&mut cells, &mut current_cell, &serving_pci);
            section = Section::Other;
            continue;
        }
        if section == Section::Other {
            continue;
        }

        if trimmed.starts_with("Frequency [") {
            finish_qmicli_cell(&mut cells, &mut current_cell, &serving_pci);
            current_earfcn.clear();
            current_band.clear();
            continue;
        }
        if trimmed.starts_with("Cell [") {
            finish_qmicli_cell(&mut cells, &mut current_cell, &serving_pci);
            let earfcn = if current_earfcn.is_empty() {
                intra_earfcn.clone()
            } else {
                current_earfcn.clone()
            };
            let band = if current_band.is_empty() {
                intra_band.clone()
            } else {
                current_band.clone()
            };
            current_cell = Some(CellInfo {
                tech: "lte".to_string(),
                band,
                arfcn: earfcn.clone(),
                earfcn,
                cell_type: "LTE".to_string(),
                ..Default::default()
            });
            continue;
        }

        if trimmed.starts_with("Tracking Area Code:") {
            if let Some(value) = value_after_colon(trimmed) {
                serving_cell.tac = parse_u32_auto(&value);
            }
            continue;
        }
        if trimmed.starts_with("Global Cell ID:") {
            if let Some(value) = value_after_colon(trimmed) {
                serving_cell.cell_id = parse_u32_auto(&value);
            }
            continue;
        }
        if trimmed.starts_with("Serving Cell ID:") {
            if let Some(value) = value_after_colon(trimmed) {
                serving_pci = value;
            }
            continue;
        }
        if trimmed.starts_with("EUTRA Absolute RF Channel Number:") {
            if let Some(value) = value_after_colon(trimmed) {
                current_earfcn = value;
                current_band = qmicli_lte_band_label(trimmed);
                if section == Section::IntraLte {
                    intra_earfcn = current_earfcn.clone();
                    intra_band = current_band.clone();
                }
            }
            continue;
        }

        let Some(cell) = current_cell.as_mut() else {
            continue;
        };
        if trimmed.starts_with("Physical Cell ID:") {
            if let Some(value) = value_after_colon(trimmed) {
                cell.pci = value;
                if !serving_pci.is_empty() && cell.pci == serving_pci {
                    cell.is_serving = true;
                    cell.cell_id = serving_cell.cell_id;
                }
            }
        } else if trimmed.starts_with("RSRQ:") {
            if let Some(value) = value_after_colon(trimmed) {
                cell.rsrq = signal_metric_to_centi_db(&value);
            }
        } else if trimmed.starts_with("RSRP:") {
            if let Some(value) = value_after_colon(trimmed) {
                cell.rsrp = signal_metric_to_centi_db(&value);
            }
        }
    }
    finish_qmicli_cell(&mut cells, &mut current_cell, &serving_pci);

    if !serving_pci.is_empty() && !cells.iter().any(|cell| cell.is_serving) {
        if let Some(first) = cells.first_mut() {
            first.is_serving = true;
            first.cell_id = serving_cell.cell_id;
        }
    }

    CellsResponse {
        serving_cell,
        cells,
    }
}

async fn get_cells_data_qmicli(
    conn: &Connection,
    modem_path: &str,
) -> Result<CellsResponse, String> {
    let device = qmi_control_device(conn, modem_path)
        .await
        .ok_or_else(|| "No QMI control port found".to_string())?;
    let output = run_recovery_command(
        "qmicli",
        &["-p", "-d", &device, "--nas-get-cell-location-info"],
    )
    .await?;
    Ok(parse_qmicli_cell_location_output(&output))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn treats_only_data_attach_transitions_as_connection_in_progress() {
        assert!(!data_connection_transition_in_progress(
            MM_MODEM_STATE_REGISTERED
        ));
        assert!(data_connection_transition_in_progress(
            MM_MODEM_STATE_DISCONNECTING
        ));
        assert!(data_connection_transition_in_progress(
            MM_MODEM_STATE_CONNECTING
        ));
        assert!(!data_connection_transition_in_progress(
            MM_MODEM_STATE_CONNECTED
        ));
    }

    #[test]
    fn parses_qmicli_lte_intra_and_interfrequency_cells() {
        let output = r#"Intrafrequency LTE Info
        Tracking Area Code: '9611'
        Global Cell ID: '39096369'
        EUTRA Absolute RF Channel Number: '1850' (E-UTRA band 3: 1800+)
        Serving Cell ID: '213'
        Cell [0]:
                Physical Cell ID: '213'
                RSRQ: '-12.9' dB
                RSRP: '-89.8' dBm
        Cell [1]:
                Physical Cell ID: '184'
                RSRQ: '-8.6' dB
                RSRP: '-84.4' dBm
Interfrequency LTE Info
        Frequency [0]:
                EUTRA Absolute RF Channel Number: '100' (E-UTRA band 1: 2100)
                Cell [0]:
                        Physical Cell ID: '76'
                        RSRQ: '-20.0' dB
                        RSRP: '-102.5' dBm
LTE Timing Advance: 'unavailable'"#;

        let parsed = parse_qmicli_cell_location_output(output);

        assert_eq!(parsed.serving_cell.tech, "lte");
        assert_eq!(parsed.serving_cell.tac, 9611);
        assert_eq!(parsed.serving_cell.cell_id, 39096369);
        assert_eq!(parsed.cells.len(), 3);
        assert!(parsed.cells[0].is_serving);
        assert_eq!(parsed.cells[0].band, "B3");
        assert_eq!(parsed.cells[0].earfcn, "1850");
        assert_eq!(parsed.cells[0].pci, "213");
        assert_eq!(parsed.cells[0].rsrp, "-8980");
        assert_eq!(parsed.cells[1].pci, "184");
        assert_eq!(parsed.cells[2].band, "B1");
        assert_eq!(parsed.cells[2].earfcn, "100");
        assert_eq!(parsed.cells[2].pci, "76");
    }

    #[test]
    fn parses_qmicli_packet_statistics_bytes() {
        let output = "[/dev/wwan0qmi0] Connection statistics:
\tTX packets OK: 10
\tRX packets OK: 13
\tTX bytes OK: 840
\tRX bytes OK: 1092";

        assert_eq!(
            parse_qmicli_packet_statistics(output),
            Some(BearerTrafficStats {
                rx_bytes: 1092,
                tx_bytes: 840,
                rx_packets: 13,
                tx_packets: 10,
            })
        );
    }

    #[test]
    fn parses_smsc_from_direct_at_output() {
        let output = "AT+CSCA?\r\r\n+CSCA: \"+10000\",145\r\n\r\nOK\r\n";

        assert_eq!(parse_smsc_from_at_output(output), "+10000");
    }

    #[test]
    fn parses_smsc_from_mmcli_wrapped_output() {
        let output = "response: '+CSCA: \"+10000\",145'";

        assert_eq!(parse_smsc_from_at_output(output), "+10000");
    }

    #[test]
    fn parses_sms_storage_info_correctly() {
        let output_1 = "+CPMS: \"SM\",2,30,\"ME\",5,50,\"SM\",2,30";
        assert_eq!(parse_sms_storage_info(output_1), Some((2, 30)));

        let output_2 = "+CPMS: \"ME\",5,50,\"SM\",10,30\n\rOK";
        assert_eq!(parse_sms_storage_info(output_2), Some((10, 30)));

        // 无 SM 三元组时回退取 ME 存储（如 QMI 直连的 Quectel 模组）
        let output_me_only = "+CPMS: \"ME\",5,50";
        assert_eq!(parse_sms_storage_info(output_me_only), Some((5, 50)));
    }

    #[test]
    fn extracts_smsc_from_protocol_output() {
        let output = "SMSC Address\n  Type: 'international'\n  Number: '+10001'";

        assert_eq!(extract_smsc_from_text(output), "+10001");
    }

    #[test]
    fn parses_own_numbers_from_at_cnum_output() {
        let output = "AT+CNUM\r\r\n+CNUM: \"Voice\",\"+10002\",145\r\n+CNUM: \"Data\",\"10003\",129\r\n\r\nOK\r\n";

        assert_eq!(
            parse_own_numbers_from_at_output(output),
            vec!["+10002".to_string(), "10003".to_string()]
        );
    }

    #[test]
    fn parses_own_numbers_from_wrapped_at_cnum_output() {
        let output = "response: '+CNUM: \"\",\"+10004\",145'";

        assert_eq!(
            parse_own_numbers_from_at_output(output),
            vec!["+10004".to_string()]
        );
    }

    #[test]
    fn parses_own_numbers_from_qmi_msisdn_output() {
        let output = "[/dev/cdc-wdm0] Successfully got MSISDN:\n\tMSISDN: '+10005'";

        assert_eq!(
            parse_own_numbers_from_labeled_text(output),
            vec!["+10005".to_string()]
        );
    }

    #[test]
    fn parses_own_numbers_from_mbim_subscriber_output() {
        let output = "Subscriber ready status retrieved:\n  Ready state: 'initialized'\n  Subscriber ID: '001010'\n  Telephone numbers: (1)\n    [0]: '+10006'";

        assert_eq!(
            parse_own_numbers_from_labeled_text(output),
            vec!["+10006".to_string()]
        );
    }

    #[test]
    fn normalizes_and_rejects_invalid_own_numbers() {
        assert_eq!(normalize_phone_number(" tel:+1 (000) 7 "), "+10007");
        assert_eq!(normalize_phone_number("145"), "");
        assert_eq!(normalize_phone_number("000000"), "");
    }

    #[test]
    fn binds_own_number_cache_to_iccid_only() {
        let identity = SimIdentity {
            iccid: "TEST_ICCID_001".to_string(),
            imsi: "001010".to_string(),
            operator_id: "00101".to_string(),
        };
        assert_eq!(
            own_number_identity_key(&identity),
            Some("iccid:TEST_ICCID_001".to_string())
        );

        let identity_without_iccid = SimIdentity {
            iccid: String::new(),
            imsi: "001010".to_string(),
            operator_id: "00101".to_string(),
        };
        assert_eq!(own_number_identity_key(&identity_without_iccid), None);
    }

    #[test]
    fn binds_smsc_cache_to_iccid_when_available() {
        let identity = SimIdentity {
            iccid: "TEST_ICCID_001".to_string(),
            imsi: "001010".to_string(),
            operator_id: "00101".to_string(),
        };
        assert_eq!(
            smsc_identity_keys(&identity),
            vec!["iccid:TEST_ICCID_001".to_string()]
        );

        let identity_without_iccid = SimIdentity {
            iccid: String::new(),
            imsi: "001010".to_string(),
            operator_id: "00101".to_string(),
        };
        assert_eq!(
            smsc_identity_keys(&identity_without_iccid),
            vec!["imsi:001010".to_string(), "operator:00101".to_string()]
        );
    }

    #[test]
    fn empty_sim_detail_cache_counts_as_present_until_iccid_changes() {
        let db = crate::db::Database::new(std::path::PathBuf::from(":memory:")).unwrap();
        let identity = SimIdentity {
            iccid: "TEST_ICCID_001".to_string(),
            imsi: "001010".to_string(),
            operator_id: "00101".to_string(),
        };
        assert!(sim_details_cache_missing(&db, &identity));

        cache_own_numbers_for_identity(&db, &identity, &[], "empty");
        cache_smsc_for_identity(&db, &identity, "", "empty");
        cache_sms_storage_for_identity(&db, &identity, None, None, "empty");
        assert!(!sim_details_cache_missing(&db, &identity));

        let changed_identity = SimIdentity {
            iccid: "TEST_ICCID_002".to_string(),
            ..identity
        };
        assert!(sim_details_cache_missing(&db, &changed_identity));
    }

    #[test]
    fn sim_detail_refresh_is_not_auto_triggered_without_iccid() {
        let db = crate::db::Database::new(std::path::PathBuf::from(":memory:")).unwrap();
        let identity = SimIdentity {
            iccid: String::new(),
            imsi: "001010".to_string(),
            operator_id: "00101".to_string(),
        };
        assert!(!sim_details_cache_missing(&db, &identity));
    }

    #[test]
    fn rejects_short_smsc_candidates_from_protocol_output() {
        let output = "SMSC Address\n  Type: '145'\n  State: '3'";

        assert_eq!(extract_smsc_from_text(output), "");
    }

    #[test]
    fn derives_china_operator_code_from_imsi_with_two_digit_mnc() {
        assert_eq!(operator_code_from_imsi("460020"), "46002");
        assert_eq!(
            split_operator_code(&operator_code_from_imsi("460020")),
            ("460".into(), "02".into())
        );
    }

    #[test]
    fn derives_non_china_operator_code_with_three_digit_mnc_fallback() {
        assert_eq!(operator_code_from_imsi("001010"), "001010");
        assert_eq!(
            split_operator_code(&operator_code_from_imsi("001010")),
            ("001".into(), "010".into())
        );
    }

    #[test]
    fn rejects_invalid_imsi_for_operator_derivation() {
        assert_eq!(operator_code_from_imsi(""), "");
        assert_eq!(operator_code_from_imsi("4600"), "");
        assert_eq!(operator_code_from_imsi("46002abc"), "");
    }

    // ── EF_SMSP / AT+CRSM 解析测试 ──

    #[test]
    fn parses_crsm_fcp_record_length_china_sim() {
        // 实测数据: 中国移动/联通, record_length = 40 (0x28), 5 条记录
        let output = "+CRSM: 97,12,\"62198205422100280583026F428A01\"";
        assert_eq!(parse_crsm_fcp_record_length(output), 40);
    }

    #[test]
    fn parses_crsm_fcp_record_length_austria_esim() {
        // 实测数据: 奥地利 eSIM, record_length = 42 (0x2A), 1 条记录
        let output = "+CRSM: 97,12,\"621982054221002A0183026F428A01\"";
        assert_eq!(parse_crsm_fcp_record_length(output), 42);
    }

    #[test]
    fn parses_crsm_fcp_record_length_with_at_echo() {
        let output =
            "AT+CRSM=192,28482,0,0,15\r\r\n+CRSM: 97,12,\"62198205422100280583026F428A01\"\r\n\r\nOK\r\n";
        assert_eq!(parse_crsm_fcp_record_length(output), 40);
    }

    #[test]
    fn returns_zero_for_invalid_crsm_fcp() {
        assert_eq!(parse_crsm_fcp_record_length("ERROR"), 0);
        assert_eq!(parse_crsm_fcp_record_length("+CRSM: 106,130"), 0);
        assert_eq!(parse_crsm_fcp_record_length(""), 0);
    }

    #[test]
    fn decodes_bcd_china_mobile_smsc() {
        // +8613800290500: BCD 编码 68 31 08 20 09 05 F0
        let digits = decode_bcd_digits(&[0x68, 0x31, 0x08, 0x20, 0x09, 0x05, 0xF0]);
        assert_eq!(digits, "8613800290500");
    }

    #[test]
    fn decodes_bcd_uk_smsc() {
        // +447870002308: BCD 编码 44 87 07 00 32 80
        let digits = decode_bcd_digits(&[0x44, 0x87, 0x07, 0x00, 0x32, 0x80]);
        assert_eq!(digits, "447870002308");
    }

    #[test]
    fn decodes_smsc_from_ts_sca_international() {
        // 长度=8, 类型=0x91(国际), BCD: 68 31 08 20 09 05 F0 → +8613800290500
        // 长度 = 1(类型字节) + 7(BCD字节) = 8
        let sca = [
            0x08, 0x91, 0x68, 0x31, 0x08, 0x20, 0x09, 0x05, 0xF0, 0xFF, 0xFF, 0xFF,
        ];
        assert_eq!(decode_smsc_from_ts_sca(&sca), "+8613800290500");
    }

    #[test]
    fn decodes_smsc_from_ts_sca_empty() {
        let sca = [0xFF; 12];
        assert_eq!(decode_smsc_from_ts_sca(&sca), "");

        let sca = [0x00; 12];
        assert_eq!(decode_smsc_from_ts_sca(&sca), "");
    }

    #[test]
    fn parses_smsc_from_crsm_record_china_mobile() {
        // 40 字节记录, TS-SCA 从偏移 25 开始 (40 - 15 = 25)
        // Alpha ID (12 bytes) + Parameter Indicators (1) + TP-DA (12) + TS-SCA (12) + PID+DCS+VP (3)
        let mut record_hex = "FF".repeat(12); // Alpha ID
        record_hex += "FF"; // Parameter Indicators
        record_hex += &"FF".repeat(12); // TP-DA
        record_hex += "0891683108200905F0FFFFFF"; // TS-SCA: len=8, type=0x91, +8613800290500
        record_hex += "FFFFFF"; // PID + DCS + VP
        let output = format!("+CRSM: 144,0,\"{record_hex}\"");
        assert_eq!(parse_smsc_from_crsm_record(&output, 40), "+8613800290500");
    }

    #[test]
    fn parses_smsc_from_crsm_record_empty_sca() {
        let record_hex = "FF".repeat(40);
        let output = format!("+CRSM: 144,0,\"{record_hex}\"");
        assert_eq!(parse_smsc_from_crsm_record(&output, 40), "");
    }

    #[test]
    fn parses_smsc_from_crsm_record_with_at_echo() {
        let mut record_hex = "FF".repeat(12);
        record_hex += "FF";
        record_hex += &"FF".repeat(12);
        record_hex += "0891683108200905F0FFFFFF";
        record_hex += "FFFFFF";
        let output =
            format!("AT+CRSM=178,28482,1,4,40\r\r\n+CRSM: 144,0,\"{record_hex}\"\r\n\r\nOK\r\n");
        assert_eq!(parse_smsc_from_crsm_record(&output, 40), "+8613800290500");
    }

    #[test]
    fn maps_known_china_operator_codes_to_default_apn() {
        assert_eq!(default_apn_for_operator_code("46001"), Some("3gnet"));
        assert_eq!(default_apn_for_operator_code("460-03"), Some("ctnet"));
        assert_eq!(default_apn_for_operator_code("460020"), Some("cmnet"));
        assert_eq!(default_apn_for_operator_code("00101"), None);
    }

    #[test]
    fn maps_apn_protocol_to_modemmanager_ip_type() {
        assert_eq!(apn_protocol_to_mm_ip_type("ip"), Some(1));
        assert_eq!(apn_protocol_to_mm_ip_type("ipv6"), Some(2));
        assert_eq!(apn_protocol_to_mm_ip_type("dual"), Some(3));
        assert_eq!(apn_protocol_to_mm_ip_type(""), None);
    }

    #[test]
    fn maps_supported_physical_band_selection_to_modemmanager_ids() {
        let req = BandLockRequest {
            lte_fdd_bands: vec![1, 3],
            lte_tdd_bands: vec![],
            nr_fdd_bands: vec![],
            nr_tdd_bands: vec![],
        };

        let mapped = accumulate_mm_ids_from_physical_bands(&req, &[31, 33, 35, 38]).unwrap();

        assert_eq!(mapped, vec![31, 33]);
    }

    #[test]
    fn rejects_partially_unsupported_physical_band_selection() {
        let req = BandLockRequest {
            lte_fdd_bands: vec![1, 8],
            lte_tdd_bands: vec![],
            nr_fdd_bands: vec![],
            nr_tdd_bands: vec![78],
        };

        let unsupported = accumulate_mm_ids_from_physical_bands(&req, &[31]).unwrap_err();

        assert_eq!(
            unsupported,
            vec!["LTE B8".to_string(), "NR n78".to_string()]
        );
    }

    #[test]
    fn parses_sms_storage_from_sim_triple() {
        let output = r#"+CPMS: "SM",3,50,"SM",3,50,"SM",3,50"#;
        assert_eq!(parse_sms_storage_info(output), Some((3, 50)));
    }

    #[test]
    fn parses_sms_storage_falls_back_to_modem_memory() {
        // Quectel QMI 模组默认存储为 ME，回复中不含 SM 三元组
        let output = r#"+CPMS: "ME",0,255,"ME",0,255,"ME",0,255"#;
        assert_eq!(parse_sms_storage_info(output), Some((0, 255)));
    }

    #[test]
    fn parses_sms_storage_prefers_sim_over_modem_memory() {
        let output = r#"+CPMS: "ME",7,255,"SM",3,50,"ME",7,255"#;
        assert_eq!(parse_sms_storage_info(output), Some((3, 50)));
    }

    #[test]
    fn parses_sms_storage_rejects_garbage() {
        assert_eq!(parse_sms_storage_info("ERROR"), None);
        assert_eq!(parse_sms_storage_info("+CPMS: \"ME\",x,y"), None);
    }

    fn apn_ctx(
        path: &str,
        context_type: &str,
        apn: &str,
        protocol: &str,
        active: bool,
    ) -> ApnContext {
        ApnContext {
            path: path.to_string(),
            name: path.rsplit('/').next().unwrap_or("bearer").to_string(),
            active,
            apn: apn.to_string(),
            protocol: protocol.to_string(),
            username: String::new(),
            password: String::new(),
            auth_method: "chap".to_string(),
            context_type: context_type.to_string(),
        }
    }

    #[test]
    fn dedups_stale_data_bearers_keeps_attach() {
        let contexts = vec![
            apn_ctx("/mm/Bearer/0", APN_CONTEXT_TYPE_ATTACH, "MORE", "ip", true),
            apn_ctx("/mm/Bearer/1", "internet", "more", "dual", false),
            apn_ctx("/mm/Bearer/2", "internet", "more", "dual", false),
        ];

        let deduped = dedup_apn_contexts(contexts);

        assert_eq!(deduped.len(), 2);
        assert_eq!(deduped[0].context_type, APN_CONTEXT_TYPE_ATTACH);
        // 两个重复的数据承载仅保留最新的
        assert_eq!(deduped[1].path, "/mm/Bearer/2");
    }

    #[test]
    fn dedups_data_bearers_prefers_connected() {
        let contexts = vec![
            apn_ctx("/mm/Bearer/1", "internet", "more", "dual", true),
            apn_ctx("/mm/Bearer/2", "internet", "more", "dual", false),
        ];

        let deduped = dedup_apn_contexts(contexts);

        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].path, "/mm/Bearer/1");
        assert!(deduped[0].active);
    }

    #[test]
    fn keeps_distinct_apn_contexts() {
        let contexts = vec![
            apn_ctx("/mm/Bearer/1", "internet", "more", "dual", false),
            apn_ctx("/mm/Bearer/2", "internet", "ims", "dual", false),
        ];

        assert_eq!(dedup_apn_contexts(contexts).len(), 2);
    }
}

fn parse_mmcli_colon_value(line: &str) -> Option<(String, String)> {
    let (_, right) = line.split_once('|')?;
    let (key, value) = right.trim().split_once(':')?;
    Some((key.trim().to_lowercase(), value.trim().to_string()))
}

fn parse_mmcli_location_output(output: &str) -> HashMap<String, String> {
    let mut values = HashMap::new();
    for line in output.lines() {
        if let Some((key, value)) = parse_mmcli_colon_value(line) {
            values.insert(key, value);
        }
    }
    values
}

fn parse_mmcli_signal_output(output: &str) -> (String, HashMap<String, String>) {
    let mut section = String::new();
    let mut values = HashMap::new();

    for line in output.lines() {
        if let Some((left, right)) = line.split_once('|') {
            let left = left.trim();
            if !left.is_empty()
                && left
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == ' ')
            {
                let upper = left.to_uppercase();
                if upper != "SIGNAL" && !upper.contains("REFRESH") {
                    section = left.to_lowercase();
                }
            }
            if let Some((key, value)) = right.trim().split_once(':') {
                values.insert(key.trim().to_lowercase(), value.trim().to_string());
            }
        }
    }

    (section, values)
}

fn mmcli_signal_section_to_tech(section: &str) -> &'static str {
    let s = section.to_lowercase();
    if s.contains("nr") || s.contains("5g") {
        "nr"
    } else if s.contains("lte") || s.contains("4g") {
        "lte"
    } else if s.contains("umts") || s.contains("wcdma") || s.contains("3g") {
        "umts"
    } else {
        "gsm"
    }
}

async fn read_mmcli_location_output() -> Result<String, String> {
    run_recovery_command("mmcli", &["-m", "any", "--location-get"]).await
}

async fn read_mmcli_signal_output() -> Result<String, String> {
    run_recovery_command("mmcli", &["-m", "any", "--signal-get"]).await
}

pub async fn start_cell_monitoring() -> Result<(), String> {
    run_recovery_command("mmcli", &["-m", "any", "--location-enable-3gpp"]).await?;
    run_recovery_command("mmcli", &["-m", "any", "--signal-setup=5"]).await?;
    Ok(())
}

pub async fn stop_cell_monitoring() -> Result<(), String> {
    run_recovery_command("mmcli", &["-m", "any", "--signal-setup=0"]).await?;
    run_recovery_command("mmcli", &["-m", "any", "--location-disable-3gpp"]).await?;
    Ok(())
}

async fn get_cells_data_mmcli_fallback(
    conn: &Connection,
    modem_path: &str,
) -> zbus::Result<CellsResponse> {
    let location_output = read_mmcli_location_output()
        .await
        .map_err(zbus::fdo::Error::Failed)?;
    let signal_output = read_mmcli_signal_output()
        .await
        .map_err(zbus::fdo::Error::Failed)?;

    let location = parse_mmcli_location_output(&location_output);
    let (signal_section, signal) = parse_mmcli_signal_output(&signal_output);
    let tech = mmcli_signal_section_to_tech(&signal_section).to_string();
    let current_bands =
        extract_u32_array(&get_property(conn, modem_path, MM_MODEM, "CurrentBands").await?);

    let tac_text = location
        .get("tracking area code")
        .or_else(|| location.get("location area code"))
        .cloned()
        .unwrap_or_default();
    let cid_text = location.get("cell id").cloned().unwrap_or_default();
    let tac = parse_hex_u32(&tac_text);
    let cell_id = parse_hex_u32(&cid_text);

    if tac == 0 && cell_id == 0 {
        return Ok(CellsResponse::default());
    }

    let serving = CellInfo {
        is_serving: true,
        tech: tech.clone(),
        cell_id,
        band: single_current_band_label(&current_bands, &tech).unwrap_or_default(),
        arfcn: String::new(),
        pci: String::new(),
        rsrp: signal
            .get("rsrp")
            .map(|v| signal_metric_to_centi_db(v))
            .unwrap_or_default(),
        rsrq: signal
            .get("rsrq")
            .map(|v| signal_metric_to_centi_db(v))
            .unwrap_or_default(),
        sinr: signal
            .get("s/n")
            .or_else(|| signal.get("snr"))
            .map(|v| signal_metric_to_centi_db(v))
            .unwrap_or_default(),
        earfcn: String::new(),
        nrarfcn: String::new(),
        cell_type: tech.to_uppercase(),
        ssb_rsrp: String::new(),
        ssb_rsrq: String::new(),
        ssb_sinr: String::new(),
    };

    Ok(CellsResponse {
        serving_cell: ServingCell { tech, cell_id, tac },
        cells: vec![serving],
    })
}

pub async fn get_cells_data(conn: &Connection) -> zbus::Result<CellsResponse> {
    let modem_path = find_modem_path(conn).await?;
    if let Ok(cells) = get_cells_data_qmicli(conn, &modem_path).await {
        if !cells.cells.is_empty() {
            return Ok(cells);
        }
    }

    let proxy = Proxy::new(conn, MM_SERVICE, modem_path.as_str(), MM_MODEM).await?;
    let cells: Vec<HashMap<String, OwnedValue>> = match proxy.call("GetCellInfo", &()).await {
        Ok(v) => v,
        Err(e) if is_get_cellinfo_unsupported(&e) => {
            return get_cells_data_mmcli_fallback(conn, &modem_path).await;
        }
        Err(e) => return Err(e),
    };
    let current_bands =
        extract_u32_array(&get_property(conn, &modem_path, MM_MODEM, "CurrentBands").await?);

    let mut serving_cell = ServingCell::default();
    let mut parsed_cells = Vec::with_capacity(cells.len());

    for cell in cells {
        let tech = detect_cell_tech(&cell).to_string();
        let is_serving = cell.get("serving").map(extract_bool).unwrap_or(false);
        let cell_id_hex = ["ci", "cell-local-id", "cell-local-id-lte"]
            .iter()
            .find_map(|k| cell.get(*k))
            .map(extract_string)
            .unwrap_or_default();
        let tac_hex = ["tac", "tracking-area-code", "lac", "location-area-code"]
            .iter()
            .find_map(|k| cell.get(*k))
            .map(extract_string)
            .unwrap_or_default();

        if is_serving {
            serving_cell = ServingCell {
                tech: tech.clone(),
                cell_id: parse_hex_u32(&cell_id_hex.trim_start_matches("0x")),
                tac: parse_hex_u32(&tac_hex.trim_start_matches("0x")),
            };
        }

        let (arfcn, earfcn, nrarfcn) =
            if let Some(n) = first_u32_string(&cell, &["nrarfcn", "nr-arfcn", "frequency-nr-dl"]) {
                let nr = n.clone();
                (n.clone(), String::new(), nr)
            } else if let Some(e) =
                first_u32_string(&cell, &["earfcn", "lte-arfcn", "dl-earfcn", "dl_earfcn"])
            {
                let ear = e.clone();
                (ear, e.clone(), String::new())
            } else if let Some(value) = cell.get("arfcn").or_else(|| cell.get("u-arfcn")) {
                let a = extract_u32(value).to_string();
                (a.clone(), a, String::new())
            } else if let Some(value) = cell
                .get("frequency-fdd-dl")
                .or_else(|| cell.get("frequency-tdd"))
            {
                let a = extract_u32(value).to_string();
                (a.clone(), a, String::new())
            } else {
                (String::new(), String::new(), String::new())
            };

        let pci = cell_pci_string(&cell);

        let cell_type = if tech == "nr" {
            "NR".to_string()
        } else if tech == "lte" {
            "LTE".to_string()
        } else {
            tech.to_uppercase()
        };

        let cell_id_u = parse_hex_u32(&cell_id_hex.trim_start_matches("0x"));
        parsed_cells.push(CellInfo {
            is_serving,
            band: single_current_band_label(&current_bands, &tech).unwrap_or_default(),
            tech: tech.clone(),
            cell_id: cell_id_u,
            arfcn: arfcn.clone(),
            pci,
            rsrp: parse_cell_metric(
                cell.get("rsrp")
                    .or_else(|| cell.get("lte-rsrp"))
                    .or_else(|| cell.get("meas-rsrp")),
            ),
            rsrq: parse_cell_metric(cell.get("rsrq").or_else(|| cell.get("meas-rsrq"))),
            sinr: parse_cell_metric(cell.get("sinr").or_else(|| cell.get("meas-sinr"))),
            earfcn,
            nrarfcn,
            cell_type,
            ssb_rsrp: parse_cell_metric(
                cell.get("ssb-rsrp")
                    .or_else(|| cell.get("ss-rsrp"))
                    .or_else(|| cell.get("nr-ss-rsrp")),
            ),
            ssb_rsrq: parse_cell_metric(
                cell.get("ssb-rsrq")
                    .or_else(|| cell.get("ss-rsrq"))
                    .or_else(|| cell.get("nr-ss-rsrq")),
            ),
            ssb_sinr: parse_cell_metric(cell.get("ssb-sinr").or_else(|| cell.get("ss-sinr"))),
        });
    }

    Ok(CellsResponse {
        serving_cell,
        cells: parsed_cells,
    })
}

pub async fn get_radio_mode(conn: &Connection) -> zbus::Result<RadioModeResponse> {
    let modem_path = find_modem_path(conn).await?;
    let current_modes = get_property(conn, &modem_path, MM_MODEM, "CurrentModes").await?;
    let supported_modes = get_property(conn, &modem_path, MM_MODEM, "SupportedModes").await?;
    let (allowed, preferred) =
        <(u32, u32)>::try_from(current_modes).unwrap_or((MM_MODE_NONE, MM_MODE_NONE));
    let supported = extract_mode_pairs(&supported_modes);
    let technology_preference = mm_access_tech_to_string(
        get_all_properties(conn, &modem_path, MM_MODEM)
            .await?
            .get("AccessTechnologies")
            .map(extract_u32)
            .unwrap_or(0),
    );

    Ok(RadioModeResponse {
        mode: normalize_mode(allowed, preferred),
        technology_preference,
        supported_modes: supported_mode_labels(&supported),
    })
}

pub async fn set_radio_mode(conn: &Connection, mode: RadioMode) -> zbus::Result<()> {
    with_serial(async {
        let modem_path = find_modem_path(conn).await?;
        let supported_modes = get_property(conn, &modem_path, MM_MODEM, "SupportedModes").await?;
        let supported = extract_mode_pairs(&supported_modes);
        let selected = choose_mode_pair(&mode, &supported).ok_or_else(|| {
            let label = match mode {
                RadioMode::Auto => "自动模式",
                RadioMode::LteOnly => "LTE 单模",
                RadioMode::NrOnly => "NR 5G 单模",
            };
            zbus::fdo::Error::Failed(format!("当前模组不支持切换到{label}"))
        })?;

        let proxy = Proxy::new(conn, MM_SERVICE, modem_path.as_str(), MM_MODEM).await?;
        proxy
            .call::<_, _, ()>("SetCurrentModes", &(selected,))
            .await?;
        Ok(())
    })
    .await
}

fn lte_physical_is_tdd(p: u32) -> bool {
    (33..=53).contains(&p) || (54..=66).contains(&p)
}

fn nr_physical_is_tdd(n: u32) -> bool {
    matches!(
        n,
        38 | 39 | 40 | 41 | 77 | 78 | 79 | 96 | 97 | 98 | 102 | 104 | 105
    )
}

fn split_mm_band_ids_to_physical(
    current_bands: &[u32],
) -> (Vec<u32>, Vec<u32>, Vec<u32>, Vec<u32>) {
    let mut lte_fdd = Vec::new();
    let mut lte_tdd = Vec::new();
    let mut nr_fdd = Vec::new();
    let mut nr_tdd = Vec::new();
    for &id in current_bands {
        if (31..=115).contains(&id) {
            let p = id - 30;
            if lte_physical_is_tdd(p) {
                lte_tdd.push(p);
            } else {
                lte_fdd.push(p);
            }
        } else if (301..=561).contains(&id) {
            let n = id - 300;
            if nr_physical_is_tdd(n) {
                nr_tdd.push(n);
            } else {
                nr_fdd.push(n);
            }
        }
    }
    for v in [&mut lte_fdd, &mut lte_tdd, &mut nr_fdd, &mut nr_tdd] {
        v.sort_unstable();
        v.dedup();
    }
    (lte_fdd, lte_tdd, nr_fdd, nr_tdd)
}

fn push_supported_band(
    out: &mut Vec<u32>,
    unsupported: &mut Vec<String>,
    supported: &[u32],
    id: u32,
    label: String,
) {
    if supported.contains(&id) {
        out.push(id);
    } else {
        unsupported.push(label);
    }
}

fn accumulate_mm_ids_from_physical_bands(
    req: &BandLockRequest,
    supported: &[u32],
) -> Result<Vec<u32>, Vec<String>> {
    let mut out: Vec<u32> = Vec::new();
    let mut unsupported: Vec<String> = Vec::new();
    for &p in &req.lte_fdd_bands {
        let id = 30 + p;
        push_supported_band(
            &mut out,
            &mut unsupported,
            supported,
            id,
            format!("LTE B{p}"),
        );
    }
    for &p in &req.lte_tdd_bands {
        let id = 30 + p;
        push_supported_band(
            &mut out,
            &mut unsupported,
            supported,
            id,
            format!("LTE B{p}"),
        );
    }
    for &n in &req.nr_fdd_bands {
        let id = 300 + n;
        push_supported_band(
            &mut out,
            &mut unsupported,
            supported,
            id,
            format!("NR n{n}"),
        );
    }
    for &n in &req.nr_tdd_bands {
        let id = 300 + n;
        push_supported_band(
            &mut out,
            &mut unsupported,
            supported,
            id,
            format!("NR n{n}"),
        );
    }
    if !unsupported.is_empty() {
        unsupported.sort();
        unsupported.dedup();
        return Err(unsupported);
    }
    out.sort_unstable();
    out.dedup();
    Ok(out)
}

pub async fn get_band_lock_status(conn: &Connection) -> zbus::Result<BandLockStatus> {
    let modem_path = find_modem_path(conn).await?;
    let supported_bands =
        extract_u32_array(&get_property(conn, &modem_path, MM_MODEM, "SupportedBands").await?);
    let current_bands =
        extract_u32_array(&get_property(conn, &modem_path, MM_MODEM, "CurrentBands").await?);
    let mut supported_sorted = supported_bands.clone();
    let mut current_sorted = current_bands.clone();
    supported_sorted.sort_unstable();
    current_sorted.sort_unstable();
    let locked = !supported_sorted.is_empty() && current_sorted != supported_sorted;
    let (lte_fdd_bands, lte_tdd_bands, nr_fdd_bands, nr_tdd_bands) =
        split_mm_band_ids_to_physical(&current_bands);
    let (
        supported_lte_fdd_bands,
        supported_lte_tdd_bands,
        supported_nr_fdd_bands,
        supported_nr_tdd_bands,
    ) = split_mm_band_ids_to_physical(&supported_bands);
    Ok(BandLockStatus {
        locked,
        supported_lte_fdd_bands,
        supported_lte_tdd_bands,
        supported_nr_fdd_bands,
        supported_nr_tdd_bands,
        lte_fdd_bands,
        lte_tdd_bands,
        nr_fdd_bands,
        nr_tdd_bands,
    })
}

pub async fn set_band_lock(conn: &Connection, req: &BandLockRequest) -> zbus::Result<()> {
    with_serial(async {
        let modem_path = find_modem_path(conn).await?;
        let supported_bands =
            extract_u32_array(&get_property(conn, &modem_path, MM_MODEM, "SupportedBands").await?);
        let all_empty = req.lte_fdd_bands.is_empty()
            && req.lte_tdd_bands.is_empty()
            && req.nr_fdd_bands.is_empty()
            && req.nr_tdd_bands.is_empty();
        let next_bands = if all_empty {
            supported_bands.clone()
        } else {
            accumulate_mm_ids_from_physical_bands(req, &supported_bands).map_err(|unsupported| {
                zbus::fdo::Error::Failed(format!(
                    "Unsupported band selection: {}",
                    unsupported.join(", ")
                ))
            })?
        };
        if !all_empty && next_bands.is_empty() {
            return Err(
                zbus::fdo::Error::Failed("所选频段与 modem 支持的 MM 频段无交集".into()).into(),
            );
        }
        let proxy = Proxy::new(conn, MM_SERVICE, modem_path.as_str(), MM_MODEM).await?;
        proxy
            .call::<_, _, ()>("SetCurrentBands", &(next_bands,))
            .await?;
        Ok(())
    })
    .await
}

async fn known_bearer_paths(conn: &Connection, modem_path: &str) -> zbus::Result<Vec<String>> {
    let mut paths =
        extract_object_path_array(&get_property(conn, modem_path, MM_MODEM, "Bearers").await?);

    if let Ok(value) = get_property(conn, modem_path, MM_MODEM, "InitialBearer").await {
        let initial_bearer = extract_string(&value);
        if !initial_bearer.is_empty() && initial_bearer != "/" {
            paths.push(initial_bearer);
        }
    }

    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn simple_connect_settings_from_bearer_props(props: &InterfaceProperties) -> SimpleConnectSettings {
    let settings = extract_bearer_settings(props);
    let ip_type = props
        .get("IpType")
        .or_else(|| settings.get("ip-type"))
        .map(extract_u32)
        .filter(|v| *v > 0);
    let allowed_auth = props
        .get("AllowedAuth")
        .or_else(|| settings.get("allowed-auth"))
        .map(extract_u32)
        .filter(|v| *v > 0);

    SimpleConnectSettings {
        apn: property_string(props, "Apn").or_else(|| property_string(&settings, "apn")),
        user: property_string(props, "User").or_else(|| property_string(&settings, "user")),
        password: property_string(props, "Password")
            .or_else(|| property_string(&settings, "password")),
        ip_type,
        allowed_auth,
        source: Some("bearer"),
    }
}

async fn read_bearer_simple_connect_settings(
    conn: &Connection,
    modem_path: &str,
) -> SimpleConnectSettings {
    let mut selected = SimpleConnectSettings::default();
    let bearer_paths = match known_bearer_paths(conn, modem_path).await {
        Ok(paths) => paths,
        Err(err) => {
            warn!(error = %err, "Failed to read ModemManager bearer paths for APN lookup");
            return selected;
        }
    };

    for path in bearer_paths {
        match get_all_properties(conn, &path, MM_BEARER).await {
            Ok(props) => {
                let settings = simple_connect_settings_from_bearer_props(&props);
                if settings.has_apn() {
                    return settings;
                }
                selected.fill_missing_from(settings);
            }
            Err(err) => {
                warn!(path = %path, error = %err, "Failed to read ModemManager bearer properties for APN lookup");
            }
        }
    }

    selected
}

fn default_apn_for_operator_code(operator_code: &str) -> Option<&'static str> {
    let digits: String = operator_code
        .chars()
        .filter(|ch| ch.is_ascii_digit())
        .collect();
    let normalized = if digits.starts_with("460") && digits.len() >= 5 {
        &digits[..5]
    } else {
        digits.as_str()
    };

    match normalized {
        "46000" | "46002" | "46004" | "46007" | "46008" => Some("cmnet"),
        "46001" | "46006" | "46009" => Some("3gnet"),
        "46003" | "46005" | "46011" => Some("ctnet"),
        "46015" => Some("cbnet"),
        _ => None,
    }
}

async fn modem_operator_code(conn: &Connection, modem_path: &str) -> String {
    let gpp_props = get_all_properties(conn, modem_path, MM_MODEM_3GPP)
        .await
        .unwrap_or_default();
    let operator_code = gpp_props
        .get("OperatorCode")
        .map(extract_string)
        .unwrap_or_default();
    if !operator_code.trim().is_empty() {
        return operator_code;
    }

    let Ok(sim_path) = get_sim_path(conn, modem_path).await else {
        return String::new();
    };
    if sim_path.is_empty() || sim_path == "/" {
        return String::new();
    }
    let sim_props = get_all_properties(conn, &sim_path, MM_SIM)
        .await
        .unwrap_or_default();
    sim_props
        .get("OperatorIdentifier")
        .map(extract_string)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            let imsi = sim_props
                .get("Imsi")
                .map(extract_string)
                .unwrap_or_default();
            operator_code_from_imsi(&imsi)
        })
}

async fn inferred_default_simple_connect_settings(
    conn: &Connection,
    modem_path: &str,
) -> SimpleConnectSettings {
    let operator_code = modem_operator_code(conn, modem_path).await;
    let apn = default_apn_for_operator_code(&operator_code).map(str::to_string);
    SimpleConnectSettings {
        apn,
        source: Some("operator-default"),
        ..SimpleConnectSettings::default()
    }
}

async fn resolve_simple_connect_settings(
    conn: &Connection,
    modem_path: &str,
    configured_apn: Option<&ApnConfig>,
) -> SimpleConnectSettings {
    let mut settings = configured_apn
        .map(apn_config_to_simple_connect_settings)
        .unwrap_or_default();
    if settings.has_apn() {
        return settings;
    }

    settings.fill_missing_from(read_bearer_simple_connect_settings(conn, modem_path).await);
    if settings.has_apn() {
        return settings;
    }

    settings.fill_missing_from(inferred_default_simple_connect_settings(conn, modem_path).await);
    settings
}

#[allow(dead_code)]
fn insert_simple_connect_settings<'a>(
    props: &mut HashMap<String, Value<'a>>,
    settings: &'a SimpleConnectSettings,
) {
    let Some(apn) = settings.apn.as_deref().filter(|apn| !apn.trim().is_empty()) else {
        return;
    };
    props.insert("apn".to_string(), Value::new(apn.trim()));
    if let Some(user) = settings
        .user
        .as_deref()
        .filter(|user| !user.trim().is_empty())
    {
        props.insert("user".to_string(), Value::new(user.trim()));
    }
    if let Some(password) = settings
        .password
        .as_deref()
        .filter(|password| !password.trim().is_empty())
    {
        props.insert("password".to_string(), Value::new(password));
    }
    if let Some(ip_type) = settings.ip_type {
        props.insert("ip-type".to_string(), Value::new(ip_type));
    }
    if let Some(allowed_auth) = settings.allowed_auth {
        props.insert("allowed-auth".to_string(), Value::new(allowed_auth));
    }
}

pub async fn set_data_connection_with_apn(
    conn: &Connection,
    active: bool,
    allow_roaming: bool,
    configured_apn: Option<&ApnConfig>,
) -> zbus::Result<()> {
    set_data_connection_inner(conn, active, allow_roaming, configured_apn).await
}

async fn set_data_connection_inner(
    conn: &Connection,
    active: bool,
    allow_roaming: bool,
    configured_apn: Option<&ApnConfig>,
) -> zbus::Result<()> {
    with_serial(async {
        let profile = find_nm_modem_connection()
            .await
            .map_err(|err| zbus::fdo::Error::Failed(format!(
                "找不到 NM modem 连接 profile: {err}"
            )))?;

        if active {
            // 检查 modem 状态，避免重复连接
            if let Ok(modem_path) = find_modem_path(conn).await {
                let state = modem_state(conn, &modem_path).await.unwrap_or(0);
                if state >= MM_MODEM_STATE_CONNECTED {
                    info!(
                        state = mm_state_to_string(state),
                        "Data connection already active, skipping duplicate connect"
                    );
                    return Ok(());
                }
                if data_connection_transition_in_progress(state) {
                    info!(
                        state = mm_state_to_string(state),
                        "Data connection transition in progress, skipping duplicate connect"
                    );
                    return Ok(());
                }
            }

            // 解析 APN 设置
            let connect_settings = if let Ok(modem_path) = find_modem_path(conn).await {
                resolve_simple_connect_settings(conn, &modem_path, configured_apn).await
            } else {
                configured_apn
                    .map(apn_config_to_simple_connect_settings)
                    .unwrap_or_default()
            };

            // 更新 NM profile 的 APN/漫游设置
            if let Err(err) = nm_update_connection(&profile, &connect_settings, allow_roaming).await {
                warn!(error = %err, "Failed to update NM connection settings, proceeding with existing");
            }

            // 通过 NM 激活连接（NM 自动处理接口 UP、IP 配置、DNS、路由）
            nm_activate_connection(&profile).await.map_err(|err| {
                zbus::fdo::Error::Failed(format!("NM 连接激活失败: {err}"))
            })?;

            info!(
                allow_roaming,
                profile = %profile,
                apn = connect_settings.apn.as_deref().unwrap_or(""),
                apn_source = connect_settings.source.unwrap_or("none"),
                "Data connection activated via NetworkManager"
            );
        } else {
            // 通过 NM 停用连接
            if let Err(err) = nm_deactivate_connection(&profile).await {
                // 如果已经断开，忽略错误
                if !get_data_connection_status(conn).await.unwrap_or(false) {
                    warn!(error = %err, "NM deactivation returned error but data is already disconnected");
                } else {
                    return Err(zbus::fdo::Error::Failed(format!(
                        "NM 连接停用失败: {err}"
                    ))
                    .into());
                }
            }
            info!("Data connection disconnected via NetworkManager");
        }

        Ok(())
    })
    .await
}

pub async fn get_data_connection_status(conn: &Connection) -> zbus::Result<bool> {
    let modem_path = find_modem_path(conn).await?;
    let modem_props = get_all_properties(conn, &modem_path, MM_MODEM).await?;
    Ok(modem_props.get("State").map(extract_i32).unwrap_or(0) >= MM_MODEM_STATE_CONNECTED)
}

#[allow(dead_code)]
async fn disconnect_known_bearers(conn: &Connection, modem_path: &str) {
    let mut paths = match get_property(conn, modem_path, MM_MODEM, "Bearers").await {
        Ok(value) => extract_object_path_array(&value),
        Err(err) => {
            warn!(error = %err, "Failed to read ModemManager bearers after data disconnect");
            Vec::new()
        }
    };

    if let Ok(value) = get_property(conn, modem_path, MM_MODEM, "InitialBearer").await {
        let initial_bearer = extract_string(&value);
        if !initial_bearer.is_empty() && initial_bearer != "/" {
            paths.push(initial_bearer);
        }
    }

    paths.sort();
    paths.dedup();

    for path in paths {
        match Proxy::new(conn, MM_SERVICE, path.as_str(), MM_BEARER).await {
            Ok(proxy) => {
                if let Err(err) = proxy.call::<_, _, ()>("Disconnect", &()).await {
                    warn!(path = %path, error = %err, "Failed to disconnect ModemManager bearer");
                }
            }
            Err(err) => {
                warn!(path = %path, error = %err, "Failed to create ModemManager bearer proxy")
            }
        }
    }
}

/// 当前是否处于漫游注册态（与「是否允许漫游」无关，后者来自本地配置）。
pub async fn get_is_roaming_mm(conn: &Connection) -> zbus::Result<bool> {
    let modem_path = find_modem_path(conn).await?;
    let gpp_props = get_all_properties(conn, &modem_path, MM_MODEM_3GPP).await?;
    let reg_state = gpp_props
        .get("RegistrationState")
        .map(extract_u32)
        .unwrap_or(0);
    Ok(matches!(reg_state, 5 | 7 | 10))
}

/// 持久化「允许漫游」并若数据已连接则重连以使 Simple.Connect 的 allow-roaming 生效。
pub async fn apply_roaming_policy(
    conn: &Connection,
    config: &ConfigManager,
    allowed: bool,
) -> zbus::Result<()> {
    config
        .set_roaming_allowed(allowed)
        .map_err(|e| zbus::fdo::Error::Failed(e))?;
    if get_data_connection_status(conn).await.unwrap_or(false) {
        let apn_config = config.get_apn_config();
        set_data_connection_with_apn(conn, false, allowed, Some(&apn_config)).await?;
        set_data_connection_with_apn(conn, true, allowed, Some(&apn_config)).await?;
    }
    Ok(())
}

fn is_invalid_transition_error(err: &zbus::Error) -> bool {
    let msg = format!("{err}");
    msg.contains("Invalid transition")
        || msg.contains("org.freedesktop.ModemManager1.Error.Core.Retry")
}

fn is_qmi_network_selection_internal_error(text: &str) -> bool {
    text.contains("Couldn't set network selection preference")
        || text.contains("org.freedesktop.libqmi.Error.Protocol.Internal")
        || text.contains("QMI protocol error (3): 'Internal'")
}

async fn modem_state(conn: &Connection, modem_path: &str) -> zbus::Result<i32> {
    get_property(conn, modem_path, MM_MODEM, "State")
        .await
        .map(|value| extract_i32(&value))
}

async fn modem_registration_state(conn: &Connection, modem_path: &str) -> zbus::Result<u32> {
    get_property(conn, modem_path, MM_MODEM_3GPP, "RegistrationState")
        .await
        .map(|value| extract_u32(&value))
}

fn modem_state_is_transient(state: i32) -> bool {
    matches!(state, 0 | 1 | 4 | 5 | 9 | 10)
}

fn data_connection_transition_in_progress(state: i32) -> bool {
    matches!(
        state,
        MM_MODEM_STATE_DISCONNECTING | MM_MODEM_STATE_CONNECTING
    )
}

async fn wait_for_modem_state<F>(
    conn: &Connection,
    modem_path: &str,
    timeout: Duration,
    mut ready: F,
) -> Result<i32, String>
where
    F: FnMut(i32) -> bool,
{
    let deadline = Instant::now() + timeout;
    let mut last_state = 0;
    loop {
        if let Ok(state) = modem_state(conn, modem_path).await {
            last_state = state;
            if ready(state) {
                return Ok(state);
            }
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "等待 Modem 状态超时，最后状态：{}",
                mm_state_to_string(last_state)
            ));
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn wait_for_radio_search(
    conn: &Connection,
    modem_path: &str,
    timeout: Duration,
) -> Result<i32, String> {
    wait_for_modem_state(conn, modem_path, timeout, |state| state >= 7).await
}

fn registration_is_ready(registration: u32, modem_state: i32) -> bool {
    modem_state >= 8 || matches!(registration, 1 | 5 | 6 | 7 | 9 | 10)
}

async fn registration_snapshot(
    conn: &Connection,
    modem_path: &str,
) -> Result<(bool, String), String> {
    let modem_state = modem_state(conn, modem_path)
        .await
        .map_err(|err| err.to_string())?;
    let gpp_props = get_all_properties(conn, modem_path, MM_MODEM_3GPP)
        .await
        .map_err(|err| err.to_string())?;
    let operator_code = gpp_props
        .get("OperatorCode")
        .map(extract_string)
        .unwrap_or_default();
    let operator_raw = gpp_props
        .get("OperatorName")
        .map(extract_string)
        .unwrap_or_default();
    let registration = gpp_props
        .get("RegistrationState")
        .map(extract_u32)
        .unwrap_or(0);
    let (mcc, mnc) = split_operator_code(&operator_code);
    let operator = localize_operator_display(&mcc, &mnc, &operator_raw);
    let operator = if operator.is_empty() {
        "unknown".to_string()
    } else {
        operator
    };
    let operator_code = if operator_code.is_empty() {
        "N/A".to_string()
    } else {
        operator_code
    };
    let summary = format!(
        "{} / {} / {}",
        operator,
        operator_code,
        mm_registration_to_string(registration)
    );
    Ok((registration_is_ready(registration, modem_state), summary))
}

async fn wait_for_registered_network(
    conn: &Connection,
    modem_path: &str,
    steps: &mut Vec<BasebandRestartStep>,
    timeout: Duration,
) -> Result<String, String> {
    let deadline = Instant::now() + timeout;
    let mut last_record_at = Instant::now() - Duration::from_secs(10);

    loop {
        let current_summary = match registration_snapshot(conn, modem_path).await {
            Ok((ready, summary)) => {
                set_baseband_restart_registration(Some(summary.clone()));
                if ready {
                    record_baseband_step(steps, "当前注册状态", "ok", Some(summary.clone()));
                    return Ok(summary);
                }
                if last_record_at.elapsed() >= Duration::from_secs(5) {
                    record_baseband_step(steps, "当前注册状态", "running", Some(summary.clone()));
                    last_record_at = Instant::now();
                }
                summary
            }
            Err(err) => {
                let summary = format!("读取注册状态失败：{err}");
                if last_record_at.elapsed() >= Duration::from_secs(5) {
                    record_baseband_step(steps, "当前注册状态", "running", Some(summary.clone()));
                    last_record_at = Instant::now();
                }
                summary
            }
        };

        if Instant::now() >= deadline {
            let message = format!("等待网络注册超时，最后状态：{}", current_summary);
            record_baseband_step(steps, "当前注册状态", "error", Some(message.clone()));
            return Err(message);
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn simple_connect_for_baseband_restart(
    conn: &Connection,
    modem_path: &str,
    allow_roaming: bool,
    configured_apn: Option<&ApnConfig>,
) -> Result<String, String> {
    let profile = find_nm_modem_connection().await?;
    let connect_settings = resolve_simple_connect_settings(conn, modem_path, configured_apn).await;
    if let Err(err) = nm_update_connection(&profile, &connect_settings, allow_roaming).await {
        warn!(error = %err, "Failed to update NM connection for baseband restart");
    }
    nm_activate_connection(&profile).await?;
    Ok(format!("NM connection {profile} activated"))
}

async fn run_baseband_simple_connect_step(
    conn: &Connection,
    modem_path: &str,
    steps: &mut Vec<BasebandRestartStep>,
    allow_roaming: bool,
    configured_apn: Option<&ApnConfig>,
) {
    if get_data_connection_status(conn).await.unwrap_or(false) {
        record_baseband_step(
            steps,
            "触发自动驻网/拨号",
            "ok",
            Some("数据连接已处于 connected".to_string()),
        );
        return;
    }

    record_baseband_step(
        steps,
        "触发自动驻网/拨号",
        "running",
        Some("ModemManager Simple.Connect".to_string()),
    );
    match simple_connect_for_baseband_restart(conn, modem_path, allow_roaming, configured_apn).await
    {
        Ok(path) => record_baseband_step(steps, "触发自动驻网/拨号", "ok", Some(path)),
        Err(err) => record_baseband_step(
            steps,
            "触发自动驻网/拨号",
            "warning",
            Some(format!("Simple.Connect 返回错误，继续等待驻网：{err}")),
        ),
    }
}

async fn set_modem_enabled(
    conn: &Connection,
    modem_path: &str,
    enabled: bool,
) -> Result<i32, String> {
    let desired_ready = |state: i32| {
        if enabled {
            state >= 6
        } else {
            state == 3
        }
    };

    for attempt in 0..5 {
        let state = modem_state(conn, modem_path)
            .await
            .map_err(|err| err.to_string())?;
        if desired_ready(state) {
            return Ok(state);
        }
        if modem_state_is_transient(state) {
            tokio::time::sleep(Duration::from_secs(2)).await;
            continue;
        }

        let proxy = Proxy::new(conn, MM_SERVICE, modem_path, MM_MODEM)
            .await
            .map_err(|err| err.to_string())?;
        match proxy.call::<_, _, ()>("Enable", &(enabled,)).await {
            Ok(()) => {
                return wait_for_modem_state(
                    conn,
                    modem_path,
                    Duration::from_secs(45),
                    desired_ready,
                )
                .await;
            }
            Err(err) if is_invalid_transition_error(&err) && attempt < 4 => {
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            Err(err) => return Err(err.to_string()),
        }
    }

    wait_for_modem_state(conn, modem_path, Duration::from_secs(15), desired_ready).await
}

async fn recover_after_registration_failure(
    conn: &Connection,
    modem_path: &str,
    original_error: String,
) -> Result<(), String> {
    set_modem_enabled(conn, modem_path, false)
        .await
        .map_err(|err| format!("{original_error}；注册失败后关闭射频失败：{err}"))?;
    tokio::time::sleep(Duration::from_secs(3)).await;
    set_modem_enabled(conn, modem_path, true)
        .await
        .map_err(|err| format!("{original_error}；注册失败后重新启用射频失败：{err}"))?;

    match wait_for_radio_search(conn, modem_path, Duration::from_secs(60)).await {
        Ok(_) => Ok(()),
        Err(err) => Err(format!(
            "{original_error}；已尝试重置射频但仍无法搜网：{err}；请稍后重试或手动断电重启"
        )),
    }
}

pub async fn set_airplane_mode(conn: &Connection, enabled: bool) -> Result<(), String> {
    with_serial(async {
        let modem_path = find_modem_path(conn).await.map_err(|err| err.to_string())?;
        set_modem_enabled(conn, &modem_path, !enabled).await?;
        Ok(())
    })
    .await
}

pub async fn get_airplane_mode(conn: &Connection) -> zbus::Result<AirplaneModeResponse> {
    let modem_path = find_modem_path(conn).await?;
    let modem_props = get_all_properties(conn, &modem_path, MM_MODEM).await?;
    let state = modem_props.get("State").map(extract_i32).unwrap_or(0);
    let powered = state >= 3;
    let online = state >= 6;

    Ok(AirplaneModeResponse {
        enabled: matches!(state, 3 | 4),
        powered,
        online,
    })
}

pub async fn get_signal_strength(conn: &Connection) -> zbus::Result<SignalStrengthResponse> {
    let modem_path = find_modem_path(conn).await?;
    let modem_props = get_all_properties(conn, &modem_path, MM_MODEM).await?;
    let strength = modem_props
        .get("SignalQuality")
        .and_then(|value| {
            <(u32, bool)>::try_from(value.clone())
                .ok()
                .map(|(q, _)| q as i32)
        })
        .unwrap_or(0);

    Ok(SignalStrengthResponse { strength })
}

fn metric_str_to_dbm(s: &str) -> i32 {
    s.parse::<f64>()
        .ok()
        .map(|v| (v / 100.0).round() as i32)
        .unwrap_or(0)
}

fn preferred_cell_rsrp_raw(c: &CellInfo) -> &str {
    if !c.rsrp.is_empty() {
        return &c.rsrp;
    }
    if !c.ssb_rsrp.is_empty() {
        return &c.ssb_rsrp;
    }
    ""
}

fn cell_to_location_row(c: &CellInfo, mcc: &str, mnc: &str, tac: u32) -> CellLocationInfo {
    let rsrq = c.rsrq.parse::<f64>().ok().map(|v| v / 100.0);
    let sinr = c.sinr.parse::<f64>().ok().map(|v| v / 100.0);
    let arfcn_u = c
        .arfcn
        .parse()
        .ok()
        .or_else(|| c.earfcn.parse().ok())
        .or_else(|| c.nrarfcn.parse().ok());
    let pci_u = c.pci.parse().ok();
    CellLocationInfo {
        mcc: mcc.to_string(),
        mnc: mnc.to_string(),
        lac: tac,
        cid: c.cell_id,
        signal_strength: metric_str_to_dbm(preferred_cell_rsrp_raw(c)),
        radio_type: c.tech.to_uppercase(),
        arfcn: arfcn_u,
        pci: pci_u,
        rsrq,
        sinr,
    }
}

fn split_operator_code(code: &str) -> (String, String) {
    if code.len() >= 6 {
        (code[..3].to_string(), code[3..6].to_string())
    } else if code.len() >= 5 {
        (code[..3].to_string(), code[3..].to_string())
    } else {
        (String::new(), String::new())
    }
}

pub async fn get_cell_location(conn: &Connection) -> zbus::Result<CellLocationResponse> {
    let net = get_network_info_data(conn).await?;
    let cells = get_cells_data(conn).await?;
    let mcc = net.mcc.clone().unwrap_or_default();
    let mnc = net.mnc.clone().unwrap_or_default();
    let tac_serving = cells.serving_cell.tac;

    let mut neighbor_cells: Vec<CellLocationInfo> = Vec::new();
    let mut cell_info: Option<CellLocationInfo> = None;

    for c in &cells.cells {
        let entry = cell_to_location_row(c, &mcc, &mnc, tac_serving);
        if c.is_serving {
            cell_info = Some(entry);
        } else {
            neighbor_cells.push(entry);
        }
    }

    // 部分固件不标记 serving，回退：把 RSRP/SSB-RSRP 最强的小区当作服务小区展示
    if cell_info.is_none() && !cells.cells.is_empty() {
        let best_idx = cells
            .cells
            .iter()
            .enumerate()
            .max_by_key(|(_, c)| metric_str_to_dbm(preferred_cell_rsrp_raw(c)))
            .map(|(i, _)| i)
            .unwrap_or(0);
        cell_info = Some(cell_to_location_row(
            &cells.cells[best_idx],
            &mcc,
            &mnc,
            tac_serving,
        ));
        neighbor_cells = cells
            .cells
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != best_idx)
            .map(|(_, c)| cell_to_location_row(c, &mcc, &mnc, tac_serving))
            .collect();
    }

    let available = cell_info.is_some() || !neighbor_cells.is_empty();
    let cells_flat = if available {
        let mut v = Vec::new();
        if let Some(ref sc) = cell_info {
            v.push(sc.clone());
        }
        v.extend(neighbor_cells.clone());
        Some(v)
    } else {
        None
    };

    Ok(CellLocationResponse {
        available,
        cell_info,
        neighbor_cells,
        cells: cells_flat,
    })
}

fn parse_available_networks_value(value: &OwnedValue) -> Vec<OperatorInfo> {
    let Ok(rows) = Vec::<HashMap<String, OwnedValue>>::try_from(value.clone()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (idx, row) in rows.into_iter().enumerate() {
        let op_id = row
            .get("operator-id")
            .map(extract_string)
            .unwrap_or_default();
        let (mcc, mnc) = split_operator_code(&op_id);
        let name = row
            .get("operator-long")
            .or_else(|| row.get("operator-name"))
            .map(extract_string)
            .unwrap_or_else(|| op_id.clone());
        let name = localize_operator_display(&mcc, &mnc, &name);
        let status = row
            .get("status")
            .map(extract_string)
            .unwrap_or_else(|| "available".into());
        let tech = row
            .get("access-technology")
            .map(extract_u32)
            .map(|v| mm_access_tech_to_string(v).to_uppercase())
            .unwrap_or_else(|| "LTE".to_string());
        out.push(OperatorInfo {
            path: format!("/scan/{idx}"),
            name,
            status,
            mcc,
            mnc,
            technologies: vec![tech],
        });
    }
    out
}

pub async fn get_operators_list(conn: &Connection) -> zbus::Result<OperatorListResponse> {
    let modem_path = find_modem_path(conn).await?;
    let gpp = get_all_properties(conn, &modem_path, MM_MODEM_3GPP).await?;
    let modem_props = get_all_properties(conn, &modem_path, MM_MODEM).await?;
    let op_name = gpp
        .get("OperatorName")
        .map(extract_string)
        .unwrap_or_default();
    let oc = gpp
        .get("OperatorCode")
        .map(extract_string)
        .unwrap_or_default();
    let (mcc, mnc) = split_operator_code(&oc);
    let access = modem_props
        .get("AccessTechnologies")
        .map(extract_u32)
        .map(mm_access_tech_to_string)
        .unwrap_or_else(|| "lte".to_string())
        .to_uppercase();
    let current = OperatorInfo {
        path: format!("{modem_path}/current"),
        name: localize_operator_display(&mcc, &mnc, &op_name),
        status: "current".into(),
        mcc,
        mnc,
        technologies: vec![access],
    };
    let mut operators = vec![current];
    if let Ok(v) = get_property(conn, &modem_path, MM_MODEM_3GPP, "AvailableNetworks").await {
        let mut scanned = parse_available_networks_value(&v);
        if !scanned.is_empty() {
            scanned.retain(|o| o.status != "current");
            operators.extend(scanned);
        }
    }
    Ok(OperatorListResponse { operators })
}

pub async fn scan_operators(conn: &Connection) -> zbus::Result<OperatorListResponse> {
    let modem_path = find_modem_path(conn).await?;
    let proxy = Proxy::new(conn, MM_SERVICE, modem_path.as_str(), MM_MODEM_3GPP).await?;
    match tokio::time::timeout(
        Duration::from_secs(OPERATOR_SCAN_REQUEST_TIMEOUT_SECS),
        proxy.call::<_, _, ()>("Scan", &()),
    )
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(err)) => warn!(error = %err, "Operator scan request failed"),
        Err(_) => warn!(
            timeout_secs = OPERATOR_SCAN_REQUEST_TIMEOUT_SECS,
            "Operator scan request timed out"
        ),
    }

    let polls = OPERATOR_SCAN_CACHE_POLL_SECS / 2;
    for _ in 0..polls {
        tokio::time::sleep(Duration::from_secs(2)).await;
        if let Ok(v) = get_property(conn, &modem_path, MM_MODEM_3GPP, "AvailableNetworks").await {
            let parsed = parse_available_networks_value(&v);
            if !parsed.is_empty() {
                let mut base = get_operators_list(conn).await?.operators;
                for o in parsed {
                    let key = format!("{}-{}", o.mcc, o.mnc);
                    if !base.iter().any(|b| format!("{}-{}", b.mcc, b.mnc) == key) {
                        base.push(o);
                    }
                }
                return Ok(OperatorListResponse { operators: base });
            }
        }
    }
    get_operators_list(conn).await
}

async fn register_operator_on_modem(
    conn: &Connection,
    modem_path: &str,
    mccmnc: &str,
) -> Result<(), String> {
    let proxy = Proxy::new(conn, MM_SERVICE, modem_path, MM_MODEM_3GPP)
        .await
        .map_err(|err| err.to_string())?;
    let network_id = mccmnc.to_string();
    let args = (network_id,);
    match tokio::time::timeout(
        Duration::from_secs(NETWORK_REGISTER_TIMEOUT_SECS),
        proxy.call::<_, _, ()>("Register", &args),
    )
    .await
    {
        Ok(Ok(())) => Ok(()),
        Ok(Err(err)) => Err(err.to_string()),
        Err(_) => Err(format!(
            "Network registration request timed out after {NETWORK_REGISTER_TIMEOUT_SECS}s"
        )),
    }
}

pub async fn register_operator_manual(conn: &Connection, mccmnc: &str) -> Result<(), String> {
    with_serial(async {
        let modem_path = find_modem_path(conn).await.map_err(|err| err.to_string())?;
        match register_operator_on_modem(conn, &modem_path, mccmnc).await {
            Ok(()) => Ok(()),
            Err(err) => {
                if is_qmi_network_selection_internal_error(&err) {
                    recover_after_registration_failure(conn, &modem_path, err).await
                } else {
                    Err(err)
                }
            }
        }
    })
    .await
}

pub async fn register_operator_auto(conn: &Connection) -> Result<(), String> {
    with_serial(async {
        let modem_path = find_modem_path(conn).await.map_err(|err| err.to_string())?;
        match register_operator_on_modem(conn, &modem_path, "").await {
            Ok(()) => Ok(()),
            Err(err) => {
                if is_qmi_network_selection_internal_error(&err) {
                    recover_after_registration_failure(conn, &modem_path, err).await
                } else {
                    Err(err)
                }
            }
        }
    })
    .await
}

const MM_BEARER: &str = "org.freedesktop.ModemManager1.Bearer";
const MM_BEARER_ALLOWED_AUTH_NONE: u32 = 1 << 0;
const MM_BEARER_ALLOWED_AUTH_PAP: u32 = 1 << 1;
const MM_BEARER_ALLOWED_AUTH_CHAP: u32 = 1 << 2;
/// MMBearerType: LTE 初始附着承载（default-attach）
const MM_BEARER_TYPE_DEFAULT_ATTACH: u32 = 2;
const APN_CONTEXT_TYPE_ATTACH: &str = "attach";

fn bearer_ip_type_to_protocol(v: u32) -> &'static str {
    match v {
        1 => "ip",
        2 => "ipv6",
        _ => "dual",
    }
}

fn apn_auth_method_to_mm_allowed_auth(method: &str) -> Option<u32> {
    match method.to_ascii_lowercase().as_str() {
        "none" => Some(MM_BEARER_ALLOWED_AUTH_NONE),
        "pap" => Some(MM_BEARER_ALLOWED_AUTH_PAP),
        "chap" => Some(MM_BEARER_ALLOWED_AUTH_CHAP),
        _ => None,
    }
}

fn mm_allowed_auth_to_apn_auth_method(mask: u32) -> &'static str {
    if mask == MM_BEARER_ALLOWED_AUTH_NONE {
        "none"
    } else if mask & MM_BEARER_ALLOWED_AUTH_CHAP != 0 {
        "chap"
    } else if mask & MM_BEARER_ALLOWED_AUTH_PAP != 0 {
        "pap"
    } else {
        "chap"
    }
}

fn extract_bearer_settings(props: &InterfaceProperties) -> InterfaceProperties {
    props
        .get("Properties")
        .and_then(|value| InterfaceProperties::try_from(value.clone()).ok())
        .unwrap_or_default()
}

pub async fn list_apn_contexts(
    conn: &Connection,
    configured_apn: Option<&ApnConfig>,
) -> zbus::Result<ApnListResponse> {
    let modem_path = find_modem_path(conn).await?;
    let bearer_paths = known_bearer_paths(conn, &modem_path).await?;
    let mut contexts = Vec::new();
    for path in bearer_paths {
        let props = get_all_properties(conn, &path, MM_BEARER).await?;
        let settings = extract_bearer_settings(&props);
        let apn = props
            .get("Apn")
            .or_else(|| settings.get("apn"))
            .map(extract_string)
            .unwrap_or_default();
        let user = props
            .get("User")
            .or_else(|| settings.get("user"))
            .map(extract_string)
            .unwrap_or_default();
        let password = props
            .get("Password")
            .or_else(|| settings.get("password"))
            .map(extract_string)
            .unwrap_or_default();
        let ip_type = props
            .get("IpType")
            .or_else(|| settings.get("ip-type"))
            .map(extract_u32)
            .unwrap_or(0);
        let auth_method = props
            .get("AllowedAuth")
            .or_else(|| settings.get("allowed-auth"))
            .map(extract_u32)
            .map(mm_allowed_auth_to_apn_auth_method)
            .unwrap_or("chap");
        let connected = props.get("Connected").map(extract_bool).unwrap_or(false);
        let bearer_type = props.get("Type").map(extract_u32).unwrap_or(0);
        let is_attach = bearer_type == MM_BEARER_TYPE_DEFAULT_ATTACH;
        let name = if is_attach {
            "attach".to_string()
        } else {
            path.rsplit('/').next().unwrap_or("bearer").to_string()
        };
        contexts.push(ApnContext {
            path: path.clone(),
            name,
            active: connected,
            apn,
            protocol: bearer_ip_type_to_protocol(ip_type).to_string(),
            username: user,
            password,
            auth_method: auth_method.into(),
            context_type: if is_attach {
                APN_CONTEXT_TYPE_ATTACH.into()
            } else {
                "internet".into()
            },
        });
    }
    if let Some(config) = configured_apn.filter(|config| !config.apn.trim().is_empty()) {
        for ctx in &mut contexts {
            if ctx.apn.trim().is_empty() {
                ctx.apn = config.apn.trim().to_string();
                ctx.protocol = config.protocol.trim().to_string();
                ctx.username = config.username.trim().to_string();
                ctx.password = config.password.clone();
                ctx.auth_method = config.auth_method.trim().to_string();
            }
        }
    }
    let mut contexts = dedup_apn_contexts(contexts);
    if contexts.is_empty() {
        let mut fallback = configured_apn
            .map(apn_config_to_simple_connect_settings)
            .unwrap_or_default();
        if !fallback.has_apn() {
            fallback.fill_missing_from(
                inferred_default_simple_connect_settings(conn, &modem_path).await,
            );
        }
        let configured_auth = configured_apn
            .map(|config| config.auth_method.trim())
            .filter(|auth| !auth.is_empty())
            .unwrap_or("chap");
        contexts.push(ApnContext {
            path: format!("{modem_path}/bearer/default"),
            name: "default".into(),
            active: false,
            apn: fallback.apn.unwrap_or_default(),
            protocol: fallback
                .ip_type
                .map(bearer_ip_type_to_protocol)
                .unwrap_or("dual")
                .to_string(),
            username: fallback.user.unwrap_or_default(),
            password: fallback.password.unwrap_or_default(),
            auth_method: fallback
                .allowed_auth
                .map(mm_allowed_auth_to_apn_auth_method)
                .unwrap_or(configured_auth)
                .to_string(),
            context_type: "internet".into(),
        });
    }
    Ok(ApnListResponse { contexts })
}

/// 去重 APN 上下文：每次拨号会在 ModemManager 中新建 bearer，旧的断开 bearer
/// 不会被回收，导致同一 APN/协议出现多个重复条目。同键仅保留一个：
/// 优先已连接者，否则保留最新出现的。附着承载（attach）单独展示，不参与去重。
fn dedup_apn_contexts(contexts: Vec<ApnContext>) -> Vec<ApnContext> {
    let mut result: Vec<ApnContext> = Vec::new();
    for ctx in contexts {
        if ctx.context_type == APN_CONTEXT_TYPE_ATTACH {
            result.push(ctx);
            continue;
        }
        let key = (ctx.apn.trim().to_ascii_lowercase(), ctx.protocol.clone());
        if let Some(existing) = result.iter_mut().find(|c| {
            c.context_type != APN_CONTEXT_TYPE_ATTACH
                && (c.apn.trim().to_ascii_lowercase(), c.protocol.clone()) == key
        }) {
            if !existing.active {
                *existing = ctx;
            }
        } else {
            result.push(ctx);
        }
    }
    result
}

fn extract_object_path_array(value: &OwnedValue) -> Vec<String> {
    if let Ok(paths) = Vec::<OwnedObjectPath>::try_from(value.clone()) {
        return paths.into_iter().map(|p| p.to_string()).collect();
    }
    if let Ok(paths) = Vec::<zbus::zvariant::ObjectPath<'_>>::try_from(value.clone()) {
        return paths.into_iter().map(|p| p.to_string()).collect();
    }
    Vec::new()
}

fn extract_u64(value: &OwnedValue) -> u64 {
    if let Ok(number) = u64::try_from(value.clone()) {
        return number;
    }
    if let Ok(number) = u32::try_from(value.clone()) {
        return number.into();
    }
    if let Ok(number) = i64::try_from(value.clone()) {
        return number.max(0) as u64;
    }
    0
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct BearerTrafficStats {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_packets: u64,
    pub tx_packets: u64,
}

fn bearer_paths_from_props(conn_value: Option<OwnedValue>) -> Vec<String> {
    conn_value
        .as_ref()
        .map(extract_object_path_array)
        .unwrap_or_default()
}

async fn bearer_paths_for_modem(conn: &Connection, modem_path: &str) -> Vec<String> {
    let mut bearers = bearer_paths_from_props(
        get_property(conn, modem_path, MM_MODEM, "Bearers")
            .await
            .ok(),
    );

    if let Ok(val) = get_property(conn, modem_path, MM_MODEM, "InitialBearer").await {
        let initial = extract_string(&val);
        if !initial.is_empty() && initial != "/" {
            bearers.push(initial);
        }
    }

    bearers.sort();
    bearers.dedup();
    bearers
}

fn bearer_interface_names(props: &InterfaceProperties) -> Vec<String> {
    let mut names = Vec::new();
    for key in ["Interface", "IpInterface"] {
        if let Some(name) = props
            .get(key)
            .map(extract_string)
            .and_then(non_empty_string)
        {
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names
}

fn extract_bearer_stats(props: &InterfaceProperties) -> Option<BearerTrafficStats> {
    let stats_val = props.get("Stats")?;
    let stats_map = InterfaceProperties::try_from(stats_val.clone()).ok()?;
    let rx_bytes = stats_map.get("rx-bytes").map(extract_u64).unwrap_or(0).max(
        stats_map
            .get("total-rx-bytes")
            .map(extract_u64)
            .unwrap_or(0),
    );
    let tx_bytes = stats_map.get("tx-bytes").map(extract_u64).unwrap_or(0).max(
        stats_map
            .get("total-tx-bytes")
            .map(extract_u64)
            .unwrap_or(0),
    );
    Some(BearerTrafficStats {
        rx_bytes,
        tx_bytes,
        ..Default::default()
    })
}

fn merge_stats(
    primary: Option<BearerTrafficStats>,
    fallback: Option<BearerTrafficStats>,
) -> Option<BearerTrafficStats> {
    match (primary, fallback) {
        (Some(a), Some(b)) => Some(BearerTrafficStats {
            rx_bytes: a.rx_bytes.max(b.rx_bytes),
            tx_bytes: a.tx_bytes.max(b.tx_bytes),
            rx_packets: a.rx_packets.max(b.rx_packets),
            tx_packets: a.tx_packets.max(b.tx_packets),
        }),
        (Some(stats), None) | (None, Some(stats)) => Some(stats),
        (None, None) => None,
    }
}

fn merge_interface_stats(
    stats_by_interface: &mut HashMap<String, BearerTrafficStats>,
    interface_name: &str,
    stats: BearerTrafficStats,
) {
    stats_by_interface
        .entry(interface_name.to_string())
        .and_modify(|current| {
            current.rx_bytes = current.rx_bytes.max(stats.rx_bytes);
            current.tx_bytes = current.tx_bytes.max(stats.tx_bytes);
            current.rx_packets = current.rx_packets.max(stats.rx_packets);
            current.tx_packets = current.tx_packets.max(stats.tx_packets);
        })
        .or_insert(stats);
}

fn parse_qmicli_packet_statistics(output: &str) -> Option<BearerTrafficStats> {
    fn parse_line(output: &str, label: &str) -> Option<u64> {
        output.lines().find_map(|line| {
            let line = line.trim();
            let value = line.strip_prefix(label)?.split(':').nth(1)?.trim();
            value.parse::<u64>().ok()
        })
    }

    let stats = BearerTrafficStats {
        rx_bytes: parse_line(output, "RX bytes OK").unwrap_or(0),
        tx_bytes: parse_line(output, "TX bytes OK").unwrap_or(0),
        rx_packets: parse_line(output, "RX packets OK").unwrap_or(0),
        tx_packets: parse_line(output, "TX packets OK").unwrap_or(0),
    };

    (stats != BearerTrafficStats::default()).then_some(stats)
}

async fn qmi_packet_stats_for_modem(
    conn: &Connection,
    modem_path: &str,
) -> Option<BearerTrafficStats> {
    let device = qmi_control_device(conn, modem_path).await?;
    let args = vec![
        "-p".to_string(),
        "-d".to_string(),
        device,
        "--wds-get-packet-statistics".to_string(),
    ];
    let output = with_serial(async { run_modem_helper_command("qmicli", args).await })
        .await
        .ok()?;
    parse_qmicli_packet_statistics(&output)
}

/// 遍历所有 modem 及其 bearers，按 ModemManager 声明的实际数据网口汇总流量 Stats。
pub async fn get_bearer_stats_by_interface(
    conn: &Connection,
) -> zbus::Result<HashMap<String, BearerTrafficStats>> {
    let mut stats_by_interface = HashMap::new();
    let modem_paths = match list_modem_paths(conn).await {
        Ok(paths) => paths,
        Err(_) => return Ok(stats_by_interface),
    };

    for modem_path in modem_paths {
        let bearers = bearer_paths_for_modem(conn, &modem_path).await;
        let mut qmi_stats: Option<Option<BearerTrafficStats>> = None;

        for bearer_path in bearers {
            let bearer_props = match get_all_properties(conn, &bearer_path, MM_BEARER).await {
                Ok(props) => props,
                Err(_) => continue,
            };

            let interface_names = bearer_interface_names(&bearer_props);
            if interface_names.is_empty() {
                continue;
            }

            if qmi_stats.is_none() {
                qmi_stats = Some(qmi_packet_stats_for_modem(conn, &modem_path).await);
            }

            if let Some(stats) =
                merge_stats(extract_bearer_stats(&bearer_props), qmi_stats.flatten())
            {
                for interface_name in interface_names {
                    merge_interface_stats(&mut stats_by_interface, &interface_name, stats);
                }
            }
        }
    }

    Ok(stats_by_interface)
}

/// 遍历所有 modem 及其 bearers，查找与指定 interface 匹配的 bearer 并获取其流量 Stats
pub async fn get_bearer_stats_for_interface(
    conn: &Connection,
    interface_name: &str,
) -> zbus::Result<Option<BearerTrafficStats>> {
    let modem_paths = match list_modem_paths(conn).await {
        Ok(paths) => paths,
        Err(_) => return Ok(None),
    };

    for modem_path in modem_paths {
        for bearer_path in bearer_paths_for_modem(conn, &modem_path).await {
            let bearer_props = match get_all_properties(conn, &bearer_path, MM_BEARER).await {
                Ok(props) => props,
                Err(_) => continue,
            };

            if bearer_interface_names(&bearer_props)
                .iter()
                .any(|name| name == interface_name)
            {
                return Ok(merge_stats(
                    extract_bearer_stats(&bearer_props),
                    qmi_packet_stats_for_modem(conn, &modem_path).await,
                ));
            }
        }
    }

    Ok(None)
}

pub async fn set_apn_on_bearer(conn: &Connection, req: &SetApnRequest) -> zbus::Result<()> {
    with_serial(async {
        let props_proxy =
            Proxy::new(conn, MM_SERVICE, req.context_path.as_str(), DBUS_PROPERTIES).await?;
        if let Some(ref auth_method) = req.auth_method {
            apn_auth_method_to_mm_allowed_auth(auth_method).ok_or_else(|| {
                zbus::fdo::Error::InvalidArgs(format!("Unsupported APN auth method: {auth_method}"))
            })?;
        }
        if let Some(ref apn) = req.apn {
            props_proxy
                .call::<_, _, ()>(
                    "Set",
                    &(MM_BEARER, "Apn", zbus::zvariant::Value::new(apn.as_str())),
                )
                .await?;
        }
        if let Some(ref user) = req.username {
            props_proxy
                .call::<_, _, ()>(
                    "Set",
                    &(MM_BEARER, "User", zbus::zvariant::Value::new(user.as_str())),
                )
                .await?;
        }
        if let Some(ref pass) = req.password {
            props_proxy
                .call::<_, _, ()>(
                    "Set",
                    &(
                        MM_BEARER,
                        "Password",
                        zbus::zvariant::Value::new(pass.as_str()),
                    ),
                )
                .await?;
        }
        if let Some(ref proto) = req.protocol {
            let ip_type = match proto.as_str() {
                "ip" => 1u32,
                "ipv6" => 2u32,
                _ => 3u32,
            };
            props_proxy
                .call::<_, _, ()>(
                    "Set",
                    &(MM_BEARER, "IpType", zbus::zvariant::Value::new(ip_type)),
                )
                .await?;
        }
        Ok(())
    })
    .await
}

fn mm_call_state_to_string(state: i32) -> &'static str {
    match state {
        1 => "dialing",
        2 => "alerting",
        3 => "incoming",
        4 => "active",
        5 => "held",
        6 => "waiting",
        7 => "terminated",
        _ => "unknown",
    }
}

fn mm_call_direction_to_string(direction: i32) -> &'static str {
    match direction {
        1 => "incoming",
        2 => "outgoing",
        _ => "unknown",
    }
}

async fn get_call_info(conn: &Connection, path: &str) -> zbus::Result<CallInfo> {
    let props = get_all_properties(conn, path, MM_CALL).await?;
    let state = props.get("State").map(extract_i32).unwrap_or(0);
    let direction = props.get("Direction").map(extract_i32).unwrap_or(0);
    let phone_number = props.get("Number").map(extract_string).unwrap_or_default();

    Ok(CallInfo {
        path: path.to_string(),
        phone_number,
        state: mm_call_state_to_string(state).to_string(),
        direction: mm_call_direction_to_string(direction).to_string(),
        start_time: None,
    })
}

pub async fn list_current_calls(conn: &Connection) -> zbus::Result<CallListResponse> {
    let modem_path = find_modem_path(conn).await?;
    let mut calls = Vec::new();
    if let Ok(voice_proxy) = Proxy::new(conn, MM_SERVICE, modem_path.as_str(), MM_VOICE).await {
        if let Ok(call_paths) = voice_proxy
            .call::<_, _, Vec<OwnedObjectPath>>("ListCalls", &())
            .await
        {
            for path in call_paths {
                if let Ok(call) = get_call_info(conn, path.as_str()).await {
                    if call.state == "terminated" {
                        let _ = delete_call_object(conn, path.as_str()).await;
                    } else {
                        calls.push(call);
                    }
                }
            }
        }
    }
    if calls.is_empty() {
        if let Ok(at_calls) = list_at_calls(conn).await {
            calls = at_calls;
        }
    }
    Ok(CallListResponse { calls })
}

pub async fn make_call(conn: &Connection, phone_number: &str) -> zbus::Result<String> {
    with_serial(async {
        let modem_path = find_modem_path(conn).await?;
        wait_until_voice_ready(conn, &modem_path).await?;
        cleanup_finished_calls(conn, &modem_path).await?;

        if let Ok(existing) = list_current_calls(conn).await {
            for call in existing.calls {
                if matches!(
                    call.state.as_str(),
                    "dialing" | "alerting" | "active" | "held" | "incoming" | "waiting"
                ) {
                    if call.phone_number == phone_number && call.direction == "outgoing" {
                        return Ok(call.path);
                    }
                    return Err(zbus::fdo::Error::Failed(
                        "已有通话进行中，请先挂断当前通话".to_string(),
                    )
                    .into());
                }
            }
        }

        match create_and_start_at_call(conn, phone_number).await {
            Ok(path) => return Ok(path),
            Err(err) => {
                warn!(error = %err, "AT voice dial failed, falling back to ModemManager Voice")
            }
        }

        let mut last_error = None;
        for attempt in 0..2 {
            match create_and_start_mm_call(conn, &modem_path, phone_number).await {
                Ok(path) => return Ok(path),
                Err(err) if attempt == 0 && is_retryable_call_setup_error(&err) => {
                    last_error = Some(err);
                    cleanup_finished_calls(conn, &modem_path).await.ok();
                    tokio::time::sleep(Duration::from_millis(800)).await;
                    wait_until_voice_ready(conn, &modem_path).await.ok();
                }
                Err(err) => return Err(err),
            }
        }

        Err(last_error
            .unwrap_or_else(|| zbus::fdo::Error::Failed("拨号失败，请稍后重试".to_string()).into()))
    })
    .await
}

pub async fn hangup_call(conn: &Connection, call_path: &str) -> zbus::Result<()> {
    if is_at_call_path(call_path) {
        run_direct_at_command(conn, "ATH")
            .await
            .map_err(|err| zbus::fdo::Error::Failed(err))?;
        return Ok(());
    }
    terminate_call(conn, call_path).await
}

async fn call_path_arg<'a>(call_path: &'a str) -> zbus::Result<zbus::zvariant::ObjectPath<'a>> {
    zbus::zvariant::ObjectPath::try_from(call_path).map_err(|_| {
        zbus::fdo::Error::InvalidArgs(format!("Invalid call path: {call_path}")).into()
    })
}

async fn delete_call_object(conn: &Connection, call_path: &str) -> zbus::Result<()> {
    let modem_path = find_modem_path(conn).await?;
    let voice_proxy = Proxy::new(conn, MM_SERVICE, modem_path.as_str(), MM_VOICE).await?;
    let path = call_path_arg(call_path).await?;
    voice_proxy.call::<_, _, ()>("DeleteCall", &(path,)).await
}

async fn cleanup_finished_calls(conn: &Connection, modem_path: &str) -> zbus::Result<()> {
    let voice_proxy = Proxy::new(conn, MM_SERVICE, modem_path, MM_VOICE).await?;
    let call_paths: Vec<OwnedObjectPath> = voice_proxy.call("ListCalls", &()).await?;
    for path in call_paths {
        if matches!(
            get_call_info(conn, path.as_str()).await.ok().map(|call| call.state),
            Some(state) if state == "terminated" || state == "unknown"
        ) {
            delete_call_object(conn, path.as_str()).await.ok();
        }
    }
    Ok(())
}

async fn wait_until_voice_ready(conn: &Connection, modem_path: &str) -> zbus::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut last_modem_state = 0;
    let mut last_registration_state = 0;

    loop {
        last_modem_state = get_property(conn, modem_path, MM_MODEM, "State")
            .await
            .map(|value| extract_i32(&value))
            .unwrap_or(last_modem_state);
        last_registration_state = get_all_properties(conn, modem_path, MM_MODEM_3GPP)
            .await
            .ok()
            .and_then(|props| props.get("RegistrationState").map(extract_u32))
            .unwrap_or(last_registration_state);

        if last_modem_state >= 8 && last_registration_state != 8 {
            return Ok(());
        }
        if Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    if last_registration_state == 8 {
        return Err(zbus::fdo::Error::Failed(
            "当前网络仅允许紧急呼叫，请稍后重试或检查网络注册状态".to_string(),
        )
        .into());
    }

    Err(zbus::fdo::Error::Failed(format!(
        "当前模组未完成网络注册（状态：{}），请稍后重试",
        mm_state_to_string(last_modem_state)
    ))
    .into())
}

fn sanitize_voice_number(phone_number: &str) -> Result<String, String> {
    let number = phone_number.trim();
    if number.is_empty() {
        return Err("Phone number is required".to_string());
    }
    if !number
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '+' | '*' | '#'))
    {
        return Err("Phone number contains unsupported characters".to_string());
    }
    Ok(number.to_string())
}

async fn run_direct_at_command(conn: &Connection, command: &str) -> Result<String, String> {
    let device = at_command_device(conn).await?;
    let command = command.to_string();
    tokio::task::spawn_blocking(move || run_direct_at_command_blocking(&device, &command))
        .await
        .map_err(|err| format!("AT 命令任务失败：{err}"))?
}

/// 排空串口缓冲区后发送 AT 命令，使用较长超时。
/// 解决部分 modem 响应缓慢或串口残留前次响应数据的问题。
async fn run_direct_at_command_draining(
    conn: &Connection,
    command: &str,
) -> Result<String, String> {
    let device = at_command_device(conn).await?;
    let command = command.to_string();
    tokio::task::spawn_blocking(move || {
        run_direct_at_command_draining_blocking(&device, &command, SMSC_BACKGROUND_AT_TIMEOUT_SECS)
    })
    .await
    .map_err(|err| format!("AT 命令任务失败：{err}"))?
}

#[cfg(unix)]
fn run_direct_at_command_blocking(device: &str, command: &str) -> Result<String, String> {
    use std::io::{Read, Write};
    use std::os::fd::AsRawFd;

    let mut port = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(device)
        .map_err(|err| format!("打开 AT 端口 {device} 失败：{err}"))?;

    let fd = port.as_raw_fd();
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        if flags >= 0 {
            let _ = libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
        }
    }

    port.write_all(format!("{command}\r").as_bytes())
        .map_err(|err| format!("写入 AT 命令失败：{err}"))?;
    port.flush()
        .map_err(|err| format!("刷新 AT 端口失败：{err}"))?;

    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    let mut output = Vec::new();
    let mut buffer = [0u8; 512];
    while std::time::Instant::now() < deadline {
        match port.read(&mut buffer) {
            Ok(0) => std::thread::sleep(Duration::from_millis(80)),
            Ok(n) => {
                output.extend_from_slice(&buffer[..n]);
                let text = String::from_utf8_lossy(&output);
                if text.contains("\r\nOK\r\n")
                    || text.contains("\nOK\r")
                    || text.contains("ERROR")
                    || text.contains("NO CARRIER")
                {
                    break;
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(80));
            }
            Err(err) => return Err(format!("读取 AT 响应失败：{err}")),
        }
    }

    let text = String::from_utf8_lossy(&output).trim().to_string();
    if text.contains("ERROR") {
        Err(text)
    } else if text.is_empty() {
        Ok("ok".to_string())
    } else {
        Ok(text)
    }
}

#[cfg(not(unix))]
fn run_direct_at_command_blocking(_device: &str, _command: &str) -> Result<String, String> {
    Err("Direct AT port access is only supported on Linux devices".to_string())
}

#[cfg(unix)]
fn run_direct_at_command_draining_blocking(
    device: &str,
    command: &str,
    timeout_secs: u64,
) -> Result<String, String> {
    use std::io::{Read, Write};
    use std::os::fd::AsRawFd;

    let mut port = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(device)
        .map_err(|err| format!("打开 AT 端口 {device} 失败：{err}"))?;

    let fd = port.as_raw_fd();
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        if flags >= 0 {
            let _ = libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
        }
    }

    // 排空缓冲区中的残留数据
    let mut drain_buf = [0u8; 512];
    let drain_deadline = std::time::Instant::now() + Duration::from_millis(300);
    while std::time::Instant::now() < drain_deadline {
        match port.read(&mut drain_buf) {
            Ok(0) => break,
            Ok(_) => continue,
            Err(_) => break,
        }
    }

    port.write_all(format!("{command}\r").as_bytes())
        .map_err(|err| format!("写入 AT 命令失败：{err}"))?;
    port.flush()
        .map_err(|err| format!("刷新 AT 端口失败：{err}"))?;

    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
    let mut output = Vec::new();
    let mut buffer = [0u8; 512];
    while std::time::Instant::now() < deadline {
        match port.read(&mut buffer) {
            Ok(0) => std::thread::sleep(Duration::from_millis(80)),
            Ok(n) => {
                output.extend_from_slice(&buffer[..n]);
                let text = String::from_utf8_lossy(&output);
                if text.contains("\r\nOK\r\n")
                    || text.contains("\nOK\r")
                    || text.contains("ERROR")
                    || text.contains("NO CARRIER")
                {
                    break;
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(80));
            }
            Err(err) => return Err(format!("读取 AT 响应失败：{err}")),
        }
    }

    let text = String::from_utf8_lossy(&output).trim().to_string();
    if text.contains("ERROR") {
        Err(text)
    } else if text.is_empty() {
        Err("AT 命令无响应".to_string())
    } else {
        Ok(text)
    }
}

#[cfg(not(unix))]
fn run_direct_at_command_draining_blocking(
    _device: &str,
    _command: &str,
    _timeout_secs: u64,
) -> Result<String, String> {
    Err("Direct AT port access is only supported on Linux devices".to_string())
}

fn at_call_path(index: &str) -> String {
    format!("at://call/{index}")
}

fn is_at_call_path(path: &str) -> bool {
    path.starts_with("at://call/")
}

fn at_clcc_state_to_string(state: &str) -> &'static str {
    match state.trim() {
        "0" => "active",
        "1" => "held",
        "2" => "dialing",
        "3" => "alerting",
        "4" => "incoming",
        "5" => "waiting",
        _ => "unknown",
    }
}

fn parse_at_clcc_line(line: &str) -> Option<CallInfo> {
    let (_, data) = line.split_once("+CLCC:")?;
    let parts: Vec<String> = data
        .split(',')
        .map(|part| part.trim().trim_matches('\'').trim_matches('"').to_string())
        .collect();
    if parts.len() < 4 {
        return None;
    }
    if parts.get(3).map(|v| v.as_str()) != Some("0") {
        return None;
    }
    let direction = if parts.get(1).map(|v| v.as_str()) == Some("1") {
        "incoming"
    } else {
        "outgoing"
    };
    Some(CallInfo {
        path: at_call_path(&parts[0]),
        phone_number: parts.get(5).cloned().unwrap_or_default(),
        state: at_clcc_state_to_string(&parts[2]).to_string(),
        direction: direction.to_string(),
        start_time: None,
    })
}

async fn list_at_calls(conn: &Connection) -> Result<Vec<CallInfo>, String> {
    let output = run_direct_at_command(conn, "AT+CLCC").await?;
    Ok(output.lines().filter_map(parse_at_clcc_line).collect())
}

async fn create_and_start_at_call(conn: &Connection, phone_number: &str) -> Result<String, String> {
    let number = sanitize_voice_number(phone_number)?;
    run_direct_at_command(conn, &format!("ATD{};", number)).await?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if let Ok(calls) = list_at_calls(conn).await {
            if let Some(call) = calls.into_iter().find(|call| {
                call.direction == "outgoing"
                    && (call.phone_number.is_empty() || call.phone_number == number)
                    && matches!(
                        call.state.as_str(),
                        "dialing" | "alerting" | "active" | "held"
                    )
            }) {
                return Ok(call.path);
            }
        }
    }
    let ceer = run_direct_at_command(conn, "AT+CEER")
        .await
        .unwrap_or_else(|err| err);
    Err(format!("ATD 已发送，但未检测到语音通话状态；{ceer}"))
}

async fn create_and_start_mm_call(
    conn: &Connection,
    modem_path: &str,
    phone_number: &str,
) -> zbus::Result<String> {
    let voice_proxy = Proxy::new(conn, MM_SERVICE, modem_path, MM_VOICE).await?;
    let mut call_props: HashMap<String, Value<'_>> = HashMap::new();
    call_props.insert("number".to_string(), Value::new(phone_number));
    let call_path: OwnedObjectPath = voice_proxy.call("CreateCall", &(call_props,)).await?;
    let call_proxy = Proxy::new(conn, MM_SERVICE, &call_path, MM_CALL).await?;
    if let Err(err) = call_proxy.call::<_, _, ()>("Start", &()).await {
        delete_call_object(conn, call_path.as_str()).await.ok();
        return Err(err);
    }
    info!(path = %call_path, phone_number = %phone_number, "Voice call started");
    Ok(call_path.to_string())
}

fn is_incompatible_call_state_error(error: &zbus::Error) -> bool {
    let text = error.to_string();
    text.contains("IncompatibleState") || text.contains("QMI protocol error (90)")
}

fn is_emergency_only_error(error: &zbus::Error) -> bool {
    let text = error.to_string();
    text.contains("only emergency calls allowed")
}

fn is_retryable_call_setup_error(error: &zbus::Error) -> bool {
    is_incompatible_call_state_error(error) || is_emergency_only_error(error)
}

async fn terminate_call(conn: &Connection, call_path: &str) -> zbus::Result<()> {
    let call = get_call_info(conn, call_path).await.ok();
    if !matches!(call.as_ref().map(|c| c.state.as_str()), Some("terminated")) {
        let hangup_result = async {
            let call_proxy = Proxy::new(conn, MM_SERVICE, call_path, MM_CALL).await?;
            call_proxy.call::<_, _, ()>("Hangup", &()).await
        }
        .await;

        if let Err(err) = hangup_result {
            if !is_incompatible_call_state_error(&err) {
                return Err(err);
            }
        }
    }

    match delete_call_object(conn, call_path).await {
        Ok(()) => Ok(()),
        Err(err) if is_incompatible_call_state_error(&err) => Ok(()),
        Err(err) => Err(err),
    }
}

pub async fn hangup_all_calls(conn: &Connection) -> zbus::Result<()> {
    with_serial(async {
        if list_at_calls(conn)
            .await
            .map(|calls| !calls.is_empty())
            .unwrap_or(false)
        {
            run_direct_at_command(conn, "ATH")
                .await
                .map_err(|err| zbus::fdo::Error::Failed(err))?;
            return Ok(());
        }
        let modem_path = find_modem_path(conn).await?;
        let voice_proxy = Proxy::new(conn, MM_SERVICE, modem_path.as_str(), MM_VOICE).await?;
        let call_paths: Vec<OwnedObjectPath> = voice_proxy.call("ListCalls", &()).await?;
        for path in call_paths {
            terminate_call(conn, path.as_str()).await?;
        }
        Ok(())
    })
    .await
}

pub async fn answer_call(conn: &Connection, call_path: &str) -> zbus::Result<()> {
    with_serial(async {
        if is_at_call_path(call_path) {
            run_direct_at_command(conn, "ATA")
                .await
                .map_err(|err| zbus::fdo::Error::Failed(err))?;
            return Ok(());
        }
        let call_proxy = Proxy::new(conn, MM_SERVICE, call_path, MM_CALL).await?;
        call_proxy.call::<_, _, ()>("Accept", &()).await?;
        Ok(())
    })
    .await
}

pub async fn get_call_by_path(conn: &Connection, call_path: &str) -> zbus::Result<CallInfo> {
    if is_at_call_path(call_path) {
        if let Ok(calls) = list_at_calls(conn).await {
            if let Some(call) = calls.into_iter().find(|call| call.path == call_path) {
                return Ok(call);
            }
        }
    }
    get_call_info(conn, call_path).await
}

pub async fn get_call_settings(conn: &Connection) -> zbus::Result<CallSettingsResponse> {
    let modem_path = find_modem_path(conn).await?;
    let voice_proxy = Proxy::new(conn, MM_SERVICE, modem_path.as_str(), MM_VOICE).await?;
    let waiting = match voice_proxy
        .call::<_, _, bool>("CallWaitingQuery", &())
        .await
    {
        Ok(true) => "enabled",
        Ok(false) => "disabled",
        Err(_) => "unknown",
    };
    Ok(CallSettingsResponse {
        calling_line_presentation: "unknown".to_string(),
        calling_name_presentation: "unknown".to_string(),
        connected_line_presentation: "unknown".to_string(),
        connected_line_restriction: "unknown".to_string(),
        called_line_presentation: "unknown".to_string(),
        calling_line_restriction: "unknown".to_string(),
        hide_caller_id: "unknown".to_string(),
        voice_call_waiting: waiting.to_string(),
    })
}

pub async fn set_call_waiting(conn: &Connection, enabled: bool) -> zbus::Result<()> {
    with_serial(async {
        let modem_path = find_modem_path(conn).await?;
        let voice_proxy = Proxy::new(conn, MM_SERVICE, modem_path.as_str(), MM_VOICE).await?;
        voice_proxy
            .call::<_, _, ()>("CallWaitingSetup", &(enabled,))
            .await?;
        Ok(())
    })
    .await
}

fn schedule_sent_sms_delete(conn: &Connection, modem_path: &str, sms_path: OwnedObjectPath) {
    let conn_clone = conn.clone();
    let modem_path = modem_path.to_string();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let proxy = Proxy::new(&conn_clone, MM_SERVICE, modem_path.as_str(), MM_MESSAGING).await;
        match proxy {
            Ok(proxy) => {
                if let Err(e) = proxy.call::<_, _, ()>("Delete", &(sms_path.clone(),)).await {
                    warn!(error = %e, path = ?sms_path, "Failed to delete sent SMS from ModemManager");
                } else {
                    info!(path = ?sms_path, "Sent SMS deleted successfully from ModemManager");
                }
            }
            Err(e) => {
                warn!(error = %e, "Failed to create ModemManager messaging proxy for SMS deletion");
            }
        }
    });
}

pub async fn send_sms(
    conn: &Connection,
    phone_number: &str,
    content: &str,
) -> zbus::Result<String> {
    with_serial(async {
        let modem_path = find_modem_path(conn).await?;
        let proxy = Proxy::new(conn, MM_SERVICE, modem_path.as_str(), MM_MESSAGING).await?;

        let mut sms_props: HashMap<String, Value<'_>> = HashMap::new();
        sms_props.insert("number".to_string(), Value::new(phone_number));
        sms_props.insert("text".to_string(), Value::new(content));

        let sms_path: OwnedObjectPath = proxy.call("Create", &(sms_props,)).await?;
        let sms_proxy = Proxy::new(conn, MM_SERVICE, &sms_path, MM_SMS).await?;
        sms_proxy.call::<_, _, ()>("Send", &()).await?;

        info!(path = %sms_path, "SMS sent successfully");
        schedule_sent_sms_delete(conn, modem_path.as_str(), sms_path.clone());
        Ok(sms_path.to_string())
    })
    .await
}

pub async fn init_data_connection(
    conn: &Connection,
    user_disabled: &AtomicBool,
    allow_roaming: bool,
    configured_apn: Option<ApnConfig>,
) -> String {
    // 设置 NM autoconnect 状态
    if let Ok(profile) = find_nm_modem_connection().await {
        let auto = !user_disabled.load(Ordering::SeqCst);
        if let Err(err) = nm_set_autoconnect(&profile, auto).await {
            warn!(error = %err, auto, "Failed to set NM autoconnect during init");
        }
    }

    if user_disabled.load(Ordering::SeqCst) {
        return match set_data_connection_with_apn(
            conn,
            false,
            allow_roaming,
            configured_apn.as_ref(),
        )
        .await
        {
            Ok(_) => "Cellular data disabled by user, disconnected".to_string(),
            Err(err) => format!("Cellular data disabled by user; disconnect skipped: {err}"),
        };
    }

    let modem_path = match find_modem_path(conn).await {
        Ok(path) => path,
        Err(err) => return format!("Failed to discover modem path: {err}"),
    };

    let state = match get_property(conn, &modem_path, MM_MODEM, "State").await {
        Ok(value) => extract_i32(&value),
        Err(err) => return format!("Failed to get modem state: {err}"),
    };

    let state_text = mm_state_to_string(state);
    if state < MM_MODEM_STATE_REGISTERED {
        return format!("Modem not registered (state: {state_text}), skipping auto-connect");
    }
    if state >= MM_MODEM_STATE_CONNECTED {
        return format!("Data connection already active (state: {state_text})");
    }
    if data_connection_transition_in_progress(state) {
        return format!("Data connection transition in progress (state: {state_text}), waiting");
    }

    match set_data_connection_with_apn(conn, true, allow_roaming, configured_apn.as_ref()).await {
        Ok(_) => format!("Data connection activated (state was: {state_text})"),
        Err(err) => format!("Failed to activate data connection: {err}"),
    }
}

/// 启动时确保 NM 有可用的 modem 连接 profile；同时清理旧版本遗留的 unmanaged 配置
pub async fn ensure_nm_modem_profile() -> String {
    // 1. Remove old unmanaged config if it exists
    let legacy_config = "/etc/NetworkManager/conf.d/99-simadmin-unmanaged-modem.conf";
    if tokio::fs::metadata(legacy_config).await.is_ok() {
        if let Err(err) = tokio::fs::remove_file(legacy_config).await {
            warn!(path = legacy_config, error = %err, "Failed to remove legacy NM unmanaged config");
        } else {
            info!("Removed legacy NM unmanaged-modem config");
            // Restart NM to pick up the change
            if let Ok(status) = Command::new("systemctl")
                .args(["is-active", "--quiet", "NetworkManager.service"])
                .status()
                .await
            {
                if status.success() {
                    let _ =
                        run_recovery_command("systemctl", &["restart", "NetworkManager.service"])
                            .await;
                    // Give NM a moment to come back and discover the modem
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
            }
        }
    }

    // 2. Check if NM is running
    match Command::new("systemctl")
        .args(["is-active", "--quiet", "NetworkManager.service"])
        .status()
        .await
    {
        Ok(status) if status.success() => {}
        _ => return "NetworkManager is not active, NM modem profile setup skipped".to_string(),
    }

    // 3. Find or create modem profile
    match find_nm_modem_connection().await {
        Ok(name) => {
            info!(profile = %name, "Found existing NM modem connection profile");
            format!("NM modem profile ready: {name}")
        }
        Err(_) => match create_nm_modem_connection().await {
            Ok(name) => {
                info!(profile = %name, "Created NM modem connection profile");
                format!("NM modem profile created: {name}")
            }
            Err(err) => {
                warn!(error = %err, "Failed to create NM modem connection profile");
                format!("NM modem profile setup failed: {err}")
            }
        },
    }
}

/// Public wrapper for handlers to query the NM modem connection profile name
pub async fn find_nm_modem_connection_pub() -> Result<String, String> {
    find_nm_modem_connection().await
}

/// Public wrapper for handlers to set NM autoconnect
pub async fn nm_set_autoconnect_pub(profile: &str, enabled: bool) -> Result<(), String> {
    nm_set_autoconnect(profile, enabled).await
}

async fn find_nm_modem_connection() -> Result<String, String> {
    let output = Command::new("nmcli")
        .args(["-t", "-f", "NAME,TYPE", "connection", "show"])
        .output()
        .await
        .map_err(|err| format!("failed to run nmcli: {err}"))?;

    if !output.status.success() {
        return Err(format!(
            "nmcli failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        // nmcli -t output uses ':' as separator
        if let Some((name, type_field)) = line.rsplit_once(':') {
            if type_field.trim() == "gsm" {
                let name = name.trim();
                if !name.is_empty() {
                    return Ok(name.to_string());
                }
            }
        }
    }

    Err("no gsm connection profile found in NetworkManager".to_string())
}

async fn create_nm_modem_connection() -> Result<String, String> {
    run_recovery_command(
        "nmcli",
        &[
            "connection",
            "add",
            "type",
            "gsm",
            "con-name",
            NM_CREATED_PROFILE_NAME,
            "ifname",
            "*",
            "gsm.auto-config",
            "yes",
            "connection.autoconnect",
            "no",
        ],
    )
    .await?;
    Ok(NM_CREATED_PROFILE_NAME.to_string())
}

async fn nm_update_connection(
    profile: &str,
    settings: &SimpleConnectSettings,
    allow_roaming: bool,
) -> Result<(), String> {
    let mut args: Vec<String> = vec!["connection".into(), "modify".into(), profile.into()];

    if let Some(apn) = settings.apn.as_deref().filter(|a| !a.trim().is_empty()) {
        args.push("gsm.apn".into());
        args.push(apn.trim().into());
    }
    if let Some(user) = settings.user.as_deref().filter(|u| !u.trim().is_empty()) {
        args.push("gsm.username".into());
        args.push(user.trim().into());
    }
    if let Some(password) = settings
        .password
        .as_deref()
        .filter(|p| !p.trim().is_empty())
    {
        args.push("gsm.password".into());
        args.push(password.into());
    }
    args.push("gsm.home-only".into());
    args.push(if allow_roaming { "no" } else { "yes" }.into());

    run_recovery_command_owned("nmcli", &args, Duration::from_secs(10)).await?;
    Ok(())
}

async fn nm_activate_connection(profile: &str) -> Result<(), String> {
    run_recovery_command_owned(
        "nmcli",
        &[
            "--wait".into(),
            "30".into(),
            "connection".into(),
            "up".into(),
            profile.into(),
        ],
        Duration::from_secs(45),
    )
    .await?;
    Ok(())
}

async fn nm_deactivate_connection(profile: &str) -> Result<(), String> {
    run_recovery_command_owned(
        "nmcli",
        &["connection".into(), "down".into(), profile.into()],
        Duration::from_secs(15),
    )
    .await?;
    Ok(())
}

async fn nm_set_autoconnect(profile: &str, enabled: bool) -> Result<(), String> {
    run_recovery_command_owned(
        "nmcli",
        &[
            "connection".into(),
            "modify".into(),
            profile.into(),
            "connection.autoconnect".into(),
            if enabled { "yes" } else { "no" }.into(),
        ],
        Duration::from_secs(10),
    )
    .await?;
    Ok(())
}

async fn run_recovery_command(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .await
        .map_err(|err| format!("failed to spawn {program}: {err}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if output.status.success() {
        if stdout.is_empty() {
            Ok("ok".to_string())
        } else {
            Ok(stdout)
        }
    } else if stderr.is_empty() {
        Err(format!("{program} exited with status {}", output.status))
    } else {
        Err(stderr)
    }
}

async fn run_recovery_command_owned(
    program: &str,
    args: &[String],
    timeout: Duration,
) -> Result<String, String> {
    let output = tokio::time::timeout(timeout, Command::new(program).args(args).output())
        .await
        .map_err(|_| format!("{program} timed out after {}s", timeout.as_secs()))?
        .map_err(|err| format!("failed to spawn {program}: {err}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if output.status.success() {
        if stdout.is_empty() {
            Ok("ok".to_string())
        } else {
            Ok(stdout)
        }
    } else if stderr.is_empty() {
        Err(format!("{program} exited with status {}", output.status))
    } else {
        Err(stderr)
    }
}

fn record_baseband_step(
    steps: &mut Vec<BasebandRestartStep>,
    step: impl Into<String>,
    status: impl Into<String>,
    detail: Option<String>,
) {
    let item = BasebandRestartStep {
        step: step.into(),
        status: status.into(),
        detail,
    };
    steps.push(item.clone());
    if let Ok(mut progress) = BASEBAND_RESTART_STEPS.lock() {
        progress.push(item);
    }
}

pub fn reset_baseband_restart_progress() {
    if let Ok(mut progress) = BASEBAND_RESTART_STEPS.lock() {
        progress.clear();
    }
    set_baseband_restart_registration(None);
    BASEBAND_RESTART_RUNNING.store(true, Ordering::SeqCst);
}

fn set_baseband_restart_registration(value: Option<String>) {
    if let Ok(mut registration) = BASEBAND_RESTART_REGISTRATION.lock() {
        *registration = value;
    }
}

pub fn record_restart_step(step: &str, status: &str, detail: Option<String>) {
    let item = BasebandRestartStep {
        step: step.to_string(),
        status: status.to_string(),
        detail,
    };
    if let Ok(mut progress) = BASEBAND_RESTART_STEPS.lock() {
        progress.push(item);
    }
}

pub struct BasebandRestartRunGuard;

impl Drop for BasebandRestartRunGuard {
    fn drop(&mut self) {
        BASEBAND_RESTART_RUNNING.store(false, Ordering::SeqCst);
    }
}

pub fn get_baseband_restart_progress() -> BasebandRestartResponse {
    let steps = BASEBAND_RESTART_STEPS
        .lock()
        .map(|progress| progress.clone())
        .unwrap_or_default();
    let current_registration = BASEBAND_RESTART_REGISTRATION
        .lock()
        .ok()
        .and_then(|registration| registration.clone());
    BasebandRestartResponse {
        steps,
        running: BASEBAND_RESTART_RUNNING.load(Ordering::SeqCst),
        current_registration,
    }
}

pub async fn restart_baseband(
    conn: &Connection,
    auto_connect_data: bool,
    allow_roaming: bool,
    configured_apn: Option<ApnConfig>,
) -> Result<BasebandRestartResponse, String> {
    reset_baseband_restart_progress();
    let _progress_guard = BasebandRestartRunGuard;
    with_serial(async move {
        restart_baseband_inner(
            conn,
            auto_connect_data,
            allow_roaming,
            configured_apn.as_ref(),
        )
        .await
    })
    .await
}

pub async fn power_cycle_sim_for_profile_switch(
    conn: &Connection,
    auto_connect_data: bool,
    allow_roaming: bool,
    configured_apn: Option<ApnConfig>,
) -> Result<BasebandRestartResponse, String> {
    with_serial(async move {
        power_cycle_sim_for_profile_switch_inner(
            conn,
            auto_connect_data,
            allow_roaming,
            configured_apn.as_ref(),
        )
        .await
    })
    .await
}

async fn power_cycle_sim_for_profile_switch_inner(
    conn: &Connection,
    auto_connect_data: bool,
    allow_roaming: bool,
    configured_apn: Option<&ApnConfig>,
) -> Result<BasebandRestartResponse, String> {
    let mut steps = Vec::new();
    record_baseband_step(
        &mut steps,
        "开始刷新 eSIM Profile（SIM 断电重枚举）",
        "running",
        None,
    );

    let initial_qmi_device = match find_modem_path(conn).await {
        Ok(modem_path) => {
            record_baseband_step(&mut steps, "定位当前基带", "ok", Some(modem_path.clone()));
            qmi_control_device(conn, &modem_path)
                .await
                .or_else(find_qmi_device_path)
        }
        Err(err) => {
            record_baseband_step(
                &mut steps,
                "定位当前基带",
                "warning",
                Some(format!("D-Bus 暂不可用，改用设备节点兜底：{err}")),
            );
            find_qmi_device_path()
        }
    };

    record_baseband_step(&mut steps, "停止 ModemManager", "running", None);
    match run_recovery_command("systemctl", &["stop", "ModemManager"]).await {
        Ok(output) => record_baseband_step(&mut steps, "停止 ModemManager", "ok", Some(output)),
        Err(err) => record_baseband_step(
            &mut steps,
            "停止 ModemManager",
            "warning",
            Some(format!("停止失败，继续尝试 SIM 断电：{err}")),
        ),
    }
    // Poll for MM to become inactive instead of a fixed 3s sleep
    for _ in 0..6 {
        match Command::new("systemctl")
            .args(["is-active", "--quiet", "ModemManager.service"])
            .status()
            .await
        {
            Ok(status) if !status.success() => break,
            _ => tokio::time::sleep(Duration::from_millis(500)).await,
        }
    }

    let power_result: Result<(), String> = async {
        let qmi_device =
            wait_for_qmi_device_path(initial_qmi_device.as_deref(), Duration::from_secs(12))
                .await
                .ok_or_else(|| "未找到 QMI 设备节点，无法执行 SIM 断电上电".to_string())?;
        record_baseband_step(&mut steps, "定位 QMI 设备", "ok", Some(qmi_device.clone()));

        record_baseband_step(&mut steps, "SIM 断电", "running", None);
        match qmicli_sim_power(&qmi_device, false).await {
            Ok(output) => record_baseband_step(&mut steps, "SIM 断电", "ok", Some(output)),
            Err(err) => {
                record_baseband_step(&mut steps, "SIM 断电", "error", Some(err.clone()));
                return Err(err);
            }
        }

        record_baseband_step(
            &mut steps,
            "等待 SIM 断电完成",
            "running",
            Some("等待 1 秒".to_string()),
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
        record_baseband_step(&mut steps, "等待 SIM 断电完成", "ok", None);

        let qmi_device = wait_for_qmi_device_path(Some(&qmi_device), Duration::from_secs(12))
            .await
            .ok_or_else(|| "SIM 断电后未重新找到 QMI 设备节点".to_string())?;
        record_baseband_step(&mut steps, "SIM 上电", "running", None);
        match qmicli_sim_power(&qmi_device, true).await {
            Ok(output) => record_baseband_step(&mut steps, "SIM 上电", "ok", Some(output)),
            Err(err) => {
                record_baseband_step(&mut steps, "SIM 上电", "error", Some(err.clone()));
                return Err(err);
            }
        }

        record_baseband_step(
            &mut steps,
            "等待 SIM 重新上电",
            "running",
            Some("等待 1 秒".to_string()),
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
        record_baseband_step(&mut steps, "等待 SIM 重新上电", "ok", None);
        Ok(())
    }
    .await;

    record_baseband_step(&mut steps, "启动 ModemManager", "running", None);
    let start_result = run_recovery_command("systemctl", &["start", "ModemManager"]).await;
    match &start_result {
        Ok(output) => {
            record_baseband_step(&mut steps, "启动 ModemManager", "ok", Some(output.clone()))
        }
        Err(err) => {
            record_baseband_step(&mut steps, "启动 ModemManager", "error", Some(err.clone()))
        }
    }

    if let Err(err) = power_result {
        return Err(err);
    }
    if let Err(err) = start_result {
        return Err(format!("SIM 已重新上电，但 ModemManager 启动失败：{err}"));
    }

    record_baseband_step(
        &mut steps,
        "等待基带重新枚举",
        "running",
        Some("轮询等待 Modem 出现（最长 15 秒）".to_string()),
    );
    // Poll for modem to reappear instead of a fixed 10s sleep
    let modem_path = {
        let enum_deadline = Instant::now() + Duration::from_secs(15);
        let mut found_path = None;
        loop {
            match find_modem_path(conn).await {
                Ok(path) => {
                    found_path = Some(path);
                    break;
                }
                Err(_) if Instant::now() < enum_deadline => {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                Err(_) => break,
            }
        }
        match found_path {
            Some(path) => {
                record_baseband_step(&mut steps, "等待基带重新枚举", "ok", Some(path.clone()));
                path
            }
            None => {
                let message =
                    "等待基带重新枚举超时：ModemManager 启动后 15 秒内未检测到 Modem".to_string();
                record_baseband_step(
                    &mut steps,
                    "等待基带重新枚举",
                    "error",
                    Some(message.clone()),
                );
                return Err(message);
            }
        }
    };

    record_baseband_step(&mut steps, "启用基带射频", "running", None);
    match wait_for_modem_state(conn, &modem_path, Duration::from_secs(10), |s| s >= 3).await {
        Ok(state) => record_baseband_step(
            &mut steps,
            "启用基带射频",
            "ok",
            Some(mm_state_to_string(state).to_string()),
        ),
        Err(err) => {
            record_baseband_step(&mut steps, "启用基带射频", "error", Some(err.clone()));
            return Err(err);
        }
    }

    if auto_connect_data {
        run_baseband_simple_connect_step(
            conn,
            &modem_path,
            &mut steps,
            allow_roaming,
            configured_apn,
        )
        .await;
    } else {
        record_baseband_step(
            &mut steps,
            "触发自动驻网/拨号",
            "skipped",
            Some("蜂窝数据已由用户关闭，仅等待 Modem 驻网".to_string()),
        );
    }

    if let Err(err) =
        wait_for_registered_network(conn, &modem_path, &mut steps, Duration::from_secs(30)).await
    {
        record_baseband_step(
            &mut steps,
            "等待网络注册",
            "warning",
            Some(format!("超时或注册异常：{err}")),
        );
    }

    record_baseband_step(&mut steps, "eSIM Profile 刷新完成", "ok", None);
    let current_registration = BASEBAND_RESTART_REGISTRATION
        .lock()
        .ok()
        .and_then(|registration| registration.clone());
    Ok(BasebandRestartResponse {
        steps,
        running: false,
        current_registration,
    })
}

async fn restart_baseband_inner(
    conn: &Connection,
    auto_connect_data: bool,
    allow_roaming: bool,
    configured_apn: Option<&ApnConfig>,
) -> Result<BasebandRestartResponse, String> {
    let mut steps = Vec::new();
    record_baseband_step(&mut steps, "开始重启基带（软重启）", "running", None);

    let modem_path = match find_modem_path(conn).await {
        Ok(path) => path,
        Err(err) => {
            let message = err.to_string();
            record_baseband_step(&mut steps, "定位当前基带", "error", Some(message.clone()));
            return Err(message);
        }
    };
    record_baseband_step(&mut steps, "定位当前基带", "ok", Some(modem_path.clone()));

    record_baseband_step(&mut steps, "关闭射频模块", "running", None);
    match set_modem_enabled(conn, &modem_path, false).await {
        Ok(state) => record_baseband_step(
            &mut steps,
            "关闭射频模块",
            "ok",
            Some(mm_state_to_string(state).to_string()),
        ),
        Err(err) => record_baseband_step(
            &mut steps,
            "关闭射频模块",
            "warning",
            Some(format!("关闭时报错，可能已处于关闭态：{err}")),
        ),
    }

    record_baseband_step(
        &mut steps,
        "清理基带状态",
        "running",
        Some("等待 5 秒，正在清理基带状态".to_string()),
    );
    tokio::time::sleep(Duration::from_secs(5)).await;
    record_baseband_step(&mut steps, "清理基带状态", "ok", None);

    record_baseband_step(&mut steps, "开启射频模块", "running", None);
    match set_modem_enabled(conn, &modem_path, true).await {
        Ok(state) => record_baseband_step(
            &mut steps,
            "开启射频模块",
            "ok",
            Some(mm_state_to_string(state).to_string()),
        ),
        Err(err) => {
            let message = format!("开启射频失败：{err}");
            record_baseband_step(&mut steps, "开启射频模块", "error", Some(message.clone()));
            return Err(message);
        }
    }

    match wait_for_radio_search(conn, &modem_path, Duration::from_secs(45)).await {
        Ok(state) => record_baseband_step(
            &mut steps,
            "等待射频搜索网络",
            "ok",
            Some(mm_state_to_string(state).to_string()),
        ),
        Err(err) => {
            record_baseband_step(&mut steps, "等待射频搜索网络", "error", Some(err.clone()));
            return Err(err);
        }
    }

    if auto_connect_data {
        run_baseband_simple_connect_step(
            conn,
            &modem_path,
            &mut steps,
            allow_roaming,
            configured_apn,
        )
        .await;
    } else {
        record_baseband_step(
            &mut steps,
            "触发自动驻网/拨号",
            "skipped",
            Some("蜂窝数据已由用户关闭，仅等待 Modem 驻网".to_string()),
        );
    }

    if let Err(err) =
        wait_for_registered_network(conn, &modem_path, &mut steps, Duration::from_secs(60)).await
    {
        record_baseband_step(
            &mut steps,
            "等待网络注册",
            "warning",
            Some(format!("超时或注册异常：{err}")),
        );
    }

    record_baseband_step(&mut steps, "重启基带完成", "ok", None);
    let current_registration = BASEBAND_RESTART_REGISTRATION
        .lock()
        .ok()
        .and_then(|registration| registration.clone());
    Ok(BasebandRestartResponse {
        steps,
        running: false,
        current_registration,
    })
}

pub async fn data_connection_watchdog(
    conn: std::sync::Arc<Connection>,
    interval_secs: u64,
    user_disabled: std::sync::Arc<AtomicBool>,
    airplane_requested: std::sync::Arc<AtomicBool>,
    config: std::sync::Arc<ConfigManager>,
    system_events: std::sync::Arc<SystemEventEmitter>,
) {
    use crate::iptables::get_iptables_rule_count;

    let mut last_log = String::new();
    let mut iptables_rules_logged = false;
    let mut missing_count = 0u32;
    let mut scan_requested_for_outage = false;
    let mut modem_outage_reported = false;
    let mut last_modem_restart_at: Option<Instant> = None;
    let mut searching_count = 0u32;
    let mut auto_register_requested_for_search = false;
    let mut last_searching_recovery_at: Option<Instant> = None;
    let mut last_data_activation_attempt_at: Option<Instant> = None;
    let mut transition_stuck_count = 0u32;
    let mut enabled_idle_count = 0u32;
    let mut cellular_problem_active = false;
    let mut data_activation_failure_active = false;
    const TRANSITION_STUCK_THRESHOLD: u32 = 6;
    const ENABLED_IDLE_RECOVERY_THRESHOLD: u32 = 8;

    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(interval_secs)).await;

        if BASEBAND_RESTART_RUNNING.load(Ordering::SeqCst) {
            let result = "Baseband restart in progress, watchdog paused".to_string();
            if result != last_log {
                info!(status = %result, "Watchdog: data connection");
                last_log = result;
            }
            continue;
        }

        match get_iptables_rule_count().await {
            Ok(count) => {
                if count.has_rules() {
                    if !iptables_rules_logged {
                        info!(
                            total = count.total(),
                            "Watchdog: iptables rules detected; automatic flush is disabled"
                        );
                        iptables_rules_logged = true;
                    }
                } else {
                    iptables_rules_logged = false;
                }
            }
            Err(err) => warn!(error = %err, "Watchdog: iptables check failed"),
        }

        let result = match find_modem_path(&conn).await {
            Ok(modem_path) => match get_property(&conn, &modem_path, MM_MODEM, "State").await {
                Ok(value) => {
                    if modem_outage_reported {
                        system_events
                            .emit_code(
                                system_event_codes::BASEBAND_MODEM_RECOVERED,
                                system_event_severity::INFO,
                                system_event_status::RECOVERED,
                                modem_path.to_string(),
                                "Modem 已重新出现在 ModemManager 中",
                            )
                            .await;
                        modem_outage_reported = false;
                    }
                    missing_count = 0;
                    scan_requested_for_outage = false;
                    let state = extract_i32(&value);
                    if state != 7 {
                        searching_count = 0;
                        auto_register_requested_for_search = false;
                    }
                    if state != 6 {
                        enabled_idle_count = 0;
                    }
                    if !data_connection_transition_in_progress(state) {
                        transition_stuck_count = 0;
                    }
                    if airplane_requested.load(Ordering::SeqCst) {
                        if state >= 6 {
                            match set_airplane_mode(&conn, true).await {
                                Ok(_) => "Airplane mode requested, modem disabled".to_string(),
                                Err(err) => {
                                    format!("Airplane mode requested, disable failed: {err}")
                                }
                            }
                        } else {
                            "Airplane mode requested, not reconnecting".to_string()
                        }
                    } else if user_disabled.load(Ordering::SeqCst) {
                        last_data_activation_attempt_at = None;
                        enabled_idle_count = 0;
                        searching_count = 0;
                        auto_register_requested_for_search = false;
                        transition_stuck_count = 0;
                        "User disabled cellular data, not reconnecting".to_string()
                    } else if state == 6 {
                        enabled_idle_count += 1;
                        if enabled_idle_count < ENABLED_IDLE_RECOVERY_THRESHOLD {
                            format!(
                                "Modem enabled but idle, waiting ({}/{})",
                                enabled_idle_count, ENABLED_IDLE_RECOVERY_THRESHOLD
                            )
                        } else {
                            enabled_idle_count = 0;
                            match set_modem_enabled(&conn, &modem_path, false).await {
                                Ok(_) => match set_modem_enabled(&conn, &modem_path, true).await {
                                    Ok(_) => {
                                        cellular_problem_active = true;
                                        system_events
                                            .emit_code(
                                                system_event_codes::CELLULAR_RADIO_CYCLE_TRIGGERED,
                                                system_event_severity::WARNING,
                                                system_event_status::TRIGGERED,
                                                modem_path.to_string(),
                                                "Modem enabled but idle, watchdog cycled radio state",
                                            )
                                            .await;
                                        "Modem enabled but idle, cycled radio state".to_string()
                                    }
                                    Err(err) => {
                                        format!("Modem enabled but idle, re-enable failed: {err}")
                                    }
                                },
                                Err(err) => {
                                    format!("Modem enabled but idle, disable failed: {err}")
                                }
                            }
                        }
                    } else if state == 7 {
                        searching_count += 1;
                        let registration = modem_registration_state(&conn, &modem_path)
                            .await
                            .unwrap_or(0);
                        let cooldown_active = last_searching_recovery_at
                            .map(|at| {
                                at.elapsed() < Duration::from_secs(MODEM_RECOVERY_COOLDOWN_SECS)
                            })
                            .unwrap_or(false);

                        if searching_count >= SEARCHING_RADIO_RESET_THRESHOLD && !cooldown_active {
                            last_searching_recovery_at = Some(Instant::now());
                            searching_count = 0;
                            auto_register_requested_for_search = false;
                            match set_modem_enabled(&conn, &modem_path, false).await {
                                Ok(_) => {
                                    tokio::time::sleep(Duration::from_secs(3)).await;
                                    match set_modem_enabled(&conn, &modem_path, true).await {
                                        Ok(_) => {
                                            cellular_problem_active = true;
                                            system_events
                                                .emit_code(
                                                    system_event_codes::CELLULAR_RADIO_CYCLE_TRIGGERED,
                                                    system_event_severity::WARNING,
                                                    system_event_status::TRIGGERED,
                                                    modem_path.to_string(),
                                                    "长时间 searching，watchdog 已循环射频状态",
                                                )
                                                .await;
                                            "Searching for too long, cycled radio state".to_string()
                                        }
                                        Err(err) => {
                                            format!(
                                                "Searching for too long, re-enable failed: {err}"
                                            )
                                        }
                                    }
                                }
                                Err(err) => {
                                    format!("Searching for too long, disable failed: {err}")
                                }
                            }
                        } else if searching_count >= SEARCHING_REGISTER_THRESHOLD
                            && !auto_register_requested_for_search
                        {
                            auto_register_requested_for_search = true;
                            cellular_problem_active = true;
                            system_events
                                .emit_code(
                                    system_event_codes::CELLULAR_SEARCHING_THRESHOLD,
                                    system_event_severity::WARNING,
                                    system_event_status::TRIGGERED,
                                    modem_path.to_string(),
                                    format!(
                                        "蜂窝网络长时间 searching，registration={}",
                                        mm_registration_to_string(registration)
                                    ),
                                )
                                .await;
                            match register_operator_auto(&conn).await {
                                Ok(_) => {
                                    system_events
                                        .emit_code(
                                            system_event_codes::CELLULAR_AUTO_REGISTER_TRIGGERED,
                                            system_event_severity::WARNING,
                                            system_event_status::TRIGGERED,
                                            modem_path.to_string(),
                                            "长时间 searching，已触发自动驻网",
                                        )
                                        .await;
                                    "Searching for too long, requested automatic registration"
                                        .to_string()
                                }
                                Err(err) => format!(
                                    "Searching for too long, automatic registration failed: {err}"
                                ),
                            }
                        } else if cooldown_active
                            && searching_count >= SEARCHING_RADIO_RESET_THRESHOLD
                        {
                            format!(
                                "Waiting for registration (state: searching, registration: {}, recovery cooldown active)",
                                mm_registration_to_string(registration)
                            )
                        } else {
                            format!(
                                "Waiting for registration (state: searching, registration: {}, attempts: {searching_count})",
                                mm_registration_to_string(registration)
                            )
                        }
                    } else if state < MM_MODEM_STATE_REGISTERED {
                        format!(
                            "Waiting for registration (state: {})",
                            mm_state_to_string(state)
                        )
                    } else if state >= MM_MODEM_STATE_CONNECTED {
                        last_data_activation_attempt_at = None;
                        data_activation_failure_active = false;
                        if cellular_problem_active {
                            system_events
                                .emit_code(
                                    system_event_codes::CELLULAR_CONNECTION_RECOVERED,
                                    system_event_severity::INFO,
                                    system_event_status::RECOVERED,
                                    modem_path.to_string(),
                                    "蜂窝数据连接已恢复",
                                )
                                .await;
                            cellular_problem_active = false;
                        }
                        "Connected".to_string()
                    } else if data_connection_transition_in_progress(state) {
                        transition_stuck_count += 1;
                        if transition_stuck_count >= TRANSITION_STUCK_THRESHOLD {
                            transition_stuck_count = 0;
                            warn!(
                                state = mm_state_to_string(state),
                                "Modem stuck in transition state, cycling radio"
                            );
                            match set_modem_enabled(&conn, &modem_path, false).await {
                                Ok(_) => {
                                    tokio::time::sleep(Duration::from_secs(3)).await;
                                    match set_modem_enabled(&conn, &modem_path, true).await {
                                        Ok(_) => {
                                            cellular_problem_active = true;
                                            system_events
                                                .emit_code(
                                                    system_event_codes::CELLULAR_RADIO_CYCLE_TRIGGERED,
                                                    system_event_severity::WARNING,
                                                    system_event_status::TRIGGERED,
                                                    modem_path.to_string(),
                                                    format!(
                                                        "数据连接状态卡住，已循环射频状态: {}",
                                                        mm_state_to_string(state)
                                                    ),
                                                )
                                                .await;
                                            "Transition stuck, cycled radio state".to_string()
                                        }
                                        Err(err) => {
                                            format!("Transition stuck, re-enable failed: {err}")
                                        }
                                    }
                                }
                                Err(err) => format!("Transition stuck, disable failed: {err}"),
                            }
                        } else {
                            format!(
                                "Data connection transition in progress (state: {}), waiting ({}/{})",
                                mm_state_to_string(state),
                                transition_stuck_count,
                                TRANSITION_STUCK_THRESHOLD
                            )
                        }
                    } else if user_disabled.load(Ordering::SeqCst) {
                        last_data_activation_attempt_at = None;
                        "User disabled cellular data, not reconnecting".to_string()
                    } else {
                        let cooldown_active = last_data_activation_attempt_at
                            .map(|at| {
                                at.elapsed() < Duration::from_secs(DATA_CONNECT_RETRY_COOLDOWN_SECS)
                            })
                            .unwrap_or(false);
                        if cooldown_active {
                            format!(
                                "Data connection inactive (state: {}), activation retry cooldown active",
                                mm_state_to_string(state)
                            )
                        } else {
                            last_data_activation_attempt_at = Some(Instant::now());
                            let allow_roaming = config.get_roaming_allowed();
                            let apn_config = config.get_apn_config();
                            match set_data_connection_with_apn(
                                &conn,
                                true,
                                allow_roaming,
                                Some(&apn_config),
                            )
                            .await
                            {
                                Ok(_) => "Connection activation requested".to_string(),
                                Err(err) => {
                                    cellular_problem_active = true;
                                    if !data_activation_failure_active {
                                        system_events
                                            .emit_code(
                                                system_event_codes::CELLULAR_ACTIVATION_FAILED,
                                                system_event_severity::WARNING,
                                                system_event_status::FAILED,
                                                modem_path.to_string(),
                                                format!("蜂窝数据连接激活失败: {err}"),
                                            )
                                            .await;
                                        data_activation_failure_active = true;
                                    }
                                    format!("Activation failed: {err}")
                                }
                            }
                        }
                    }
                }
                Err(err) => format!("Modem unavailable: {err}"),
            },
            Err(err) => {
                missing_count += 1;
                let cooldown_active = last_modem_restart_at
                    .map(|at| at.elapsed() < Duration::from_secs(MODEM_RECOVERY_COOLDOWN_SECS))
                    .unwrap_or(false);

                if missing_count >= MODEM_SCAN_THRESHOLD && !scan_requested_for_outage {
                    match run_recovery_command("mmcli", &["--scan-modems"]).await {
                        Ok(output) => {
                            scan_requested_for_outage = true;
                            system_events
                                .emit_code(
                                    system_event_codes::BASEBAND_SCAN_MODEMS_TRIGGERED,
                                    system_event_severity::INFO,
                                    system_event_status::TRIGGERED,
                                    "ModemManager",
                                    format!(
                                        "连续 {} 次未找到 Modem，已触发 mmcli --scan-modems",
                                        missing_count
                                    ),
                                )
                                .await;
                            info!(
                                failures = missing_count,
                                output = %output,
                                "Watchdog: requested modem rescan"
                            );
                        }
                        Err(scan_err) => {
                            warn!(
                                failures = missing_count,
                                error = %scan_err,
                                "Watchdog: modem rescan failed"
                            );
                        }
                    }
                }

                if missing_count >= MODEM_RESTART_THRESHOLD && !cooldown_active {
                    if !modem_outage_reported {
                        system_events
                            .emit_code(
                                system_event_codes::BASEBAND_MODEM_MISSING_THRESHOLD,
                                system_event_severity::CRITICAL,
                                system_event_status::TRIGGERED,
                                "ModemManager",
                                format!("连续 {} 次未找到 Modem", missing_count),
                            )
                            .await;
                        modem_outage_reported = true;
                    }
                    match run_recovery_command("systemctl", &["restart", "ModemManager"]).await {
                        Ok(_) => {
                            last_modem_restart_at = Some(Instant::now());
                            missing_count = 0;
                            scan_requested_for_outage = false;
                            system_events
                                .emit_code(
                                    system_event_codes::BASEBAND_MODEMMANAGER_RESTARTED,
                                    system_event_severity::WARNING,
                                    system_event_status::TRIGGERED,
                                    "ModemManager",
                                    "watchdog 已重启 ModemManager",
                                )
                                .await;
                            info!("Watchdog: restarted ModemManager after repeated modem loss");
                            "Modem unavailable, restarting ModemManager".to_string()
                        }
                        Err(restart_err) => {
                            system_events
                                .emit_code(
                                    system_event_codes::BASEBAND_MODEMMANAGER_RESTART_FAILED,
                                    system_event_severity::CRITICAL,
                                    system_event_status::FAILED,
                                    "ModemManager",
                                    format!("watchdog 重启 ModemManager 失败: {restart_err}"),
                                )
                                .await;
                            warn!(
                                failures = missing_count,
                                error = %restart_err,
                                "Watchdog: failed to restart ModemManager"
                            );
                            format!("Modem unavailable: {err}; restart failed: {restart_err}")
                        }
                    }
                } else if missing_count >= MODEM_RESTART_THRESHOLD && cooldown_active {
                    format!("Modem unavailable: {err}; recovery cooldown active")
                } else if scan_requested_for_outage {
                    format!("Modem unavailable: {err}; rescan requested")
                } else {
                    format!("Modem unavailable: {err}")
                }
            }
        };

        if result != last_log {
            info!(status = %result, "Watchdog: data connection");
            last_log = result;
        }
    }
}
