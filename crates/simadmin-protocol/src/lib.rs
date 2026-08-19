use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub const PROTOCOL_VERSION: u16 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentRegistrationRequest {
    pub installation_id: String,
    pub agent_type: AgentType,
    pub connection_scope: ConnectionScope,
    pub hostname: String,
    pub version: String,
    #[serde(default)]
    pub suggested_device_name: Option<String>,
    #[serde(default)]
    pub bootstrap_token: Option<String>,
    #[serde(default)]
    pub enrollment_token: Option<String>,
    #[serde(default)]
    pub hardware_fingerprint: Option<HardwareFingerprintPayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentRegistrationResponse {
    pub agent_id: String,
    pub pairing_code: String,
    pub pairing_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentClaimResponse {
    pub approved: bool,
    pub agent_id: String,
    pub device_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct HardwareFingerprintPayload {
    #[serde(default)]
    pub imei: Option<String>,
    #[serde(default)]
    pub usb_serial: Option<String>,
    #[serde(default)]
    pub vendor_id: Option<String>,
    #[serde(default)]
    pub product_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostDiscoveryBatchPayload {
    pub items: Vec<HostDiscoveryItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscoverySyncPayload {
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostDiscoveryItem {
    pub item_id: String,
    pub discovery_id: String,
    pub fingerprint: HardwareFingerprintPayload,
    pub usb_path: String,
    #[serde(default)]
    pub control_paths: Vec<String>,
    #[serde(default)]
    pub network_interfaces: Vec<String>,
    pub backend: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BindingPolicyPayload {
    #[default]
    HardwareBound,
    SlotBound,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BindingSyncPayload {
    pub version: i64,
    pub bindings: Vec<HostBindingPayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostBindingPayload {
    pub device_id: String,
    pub fingerprint: HardwareFingerprintPayload,
    pub binding_policy: BindingPolicyPayload,
    #[serde(default)]
    pub slot_id: Option<String>,
    pub usb_path: String,
    #[serde(default)]
    pub control_paths: Vec<String>,
    pub backend: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub binding_version: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BindingApplyResultPayload {
    pub version: i64,
    pub status: ConfigApplyStatus,
    #[serde(default)]
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentType {
    Simadmin,
    Host,
}

impl AgentType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Simadmin => "simadmin",
            Self::Host => "host",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionScope {
    Local,
    Remote,
}

impl ConnectionScope {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Remote => "remote",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Envelope {
    pub protocol_version: u16,
    pub message_id: String,
    #[serde(rename = "type")]
    pub message_type: String,
    pub sent_at: DateTime<Utc>,
    pub agent_id: String,
    pub device_id: Option<String>,
    pub correlation_id: Option<String>,
    #[serde(default)]
    pub payload: Value,
}

impl Envelope {
    pub fn new(
        message_type: impl Into<String>,
        agent_id: impl Into<String>,
        device_id: Option<String>,
        correlation_id: Option<String>,
        payload: impl Serialize,
    ) -> Result<Self, serde_json::Error> {
        Ok(Self {
            protocol_version: PROTOCOL_VERSION,
            message_id: format!("msg-{}", Uuid::new_v4()),
            message_type: message_type.into(),
            sent_at: Utc::now(),
            agent_id: agent_id.into(),
            device_id,
            correlation_id,
            payload: serde_json::to_value(payload)?,
        })
    }

    pub fn decode_payload<T: for<'de> Deserialize<'de>>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_value(self.payload.clone())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HeartbeatPayload {
    pub agent_type: AgentType,
    pub agent_version: String,
    pub session_generation: i64,
    pub managed_device_count: u32,
    pub local_queue_size: u64,
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub host_summary: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionReadyPayload {
    pub session_generation: i64,
    pub server_time: DateTime<Utc>,
    pub heartbeat_interval_seconds: u32,
    #[serde(default)]
    pub hub_instance_id: Option<String>,
    #[serde(default)]
    pub hub_version: Option<String>,
    #[serde(default)]
    pub canonical_public_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceProvisionRequest {
    pub hub_url: String,
    pub hub_instance_id: String,
    pub hub_version: String,
    pub enrollment_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HeartbeatAckPayload {
    pub server_time: DateTime<Utc>,
    pub heartbeat_interval_seconds: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeviceStatusBatchPayload {
    pub items: Vec<DeviceStatusItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SmsBatchPayload {
    pub items: Vec<SmsItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventBatchPayload {
    pub items: Vec<EventItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiagnosticBatchPayload {
    pub items: Vec<DiagnosticItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiagnosticItem {
    pub item_id: String,
    pub device_id: String,
    pub occurred_at: DateTime<Utc>,
    pub level: DiagnosticLevel,
    pub category: String,
    pub message: String,
    #[serde(default)]
    pub details: Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticLevel {
    Debug,
    Info,
    Warning,
    Error,
}

impl DiagnosticLevel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventItem {
    pub item_id: String,
    pub device_id: String,
    pub event_type: EventType,
    pub event_code: String,
    pub occurred_at: DateTime<Utc>,
    pub summary: String,
    #[serde(default)]
    pub details: Value,
    #[serde(default)]
    pub local_fallback_applied: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    Sms,
    Ddns,
    VersionUpdate,
    SystemEvent,
    DeviceStatus,
    Automation,
}

impl EventType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sms => "sms",
            Self::Ddns => "ddns",
            Self::VersionUpdate => "version_update",
            Self::SystemEvent => "system_event",
            Self::DeviceStatus => "device_status",
            Self::Automation => "automation",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SmsItem {
    pub item_id: String,
    pub device_id: String,
    pub direction: SmsDirection,
    pub phone_number: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub status: SmsStatus,
    #[serde(default)]
    pub pdu: Option<String>,
    #[serde(default = "default_sms_transport")]
    pub transport: String,
    #[serde(default)]
    pub iccid: Option<String>,
    #[serde(default)]
    pub imsi: Option<String>,
    #[serde(default)]
    pub operator_name: Option<String>,
    #[serde(default)]
    pub hub_command_id: Option<String>,
}

fn default_sms_transport() -> String {
    "modem".into()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SmsDirection {
    Incoming,
    Outgoing,
}

impl SmsDirection {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Incoming => "incoming",
            Self::Outgoing => "outgoing",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SmsStatus {
    Pending,
    Sent,
    Failed,
    Received,
    Unknown,
}

impl SmsStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Sent => "sent",
            Self::Failed => "failed",
            Self::Received => "received",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeviceStatusItem {
    pub item_id: String,
    pub device_id: String,
    pub observed_at: DateTime<Utc>,
    pub status: DeviceStatus,
    pub hardware_present: bool,
    pub control_channel_status: LayerStatus,
    pub sim_status: LayerStatus,
    pub cellular_registration_status: LayerStatus,
    pub data_connection_status: LayerStatus,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub simadmin_version: Option<String>,
    #[serde(default)]
    pub architecture: Option<String>,
    #[serde(default)]
    pub phone_number: Option<String>,
    #[serde(default)]
    pub carrier: Option<String>,
    #[serde(default)]
    pub network_type: Option<String>,
    #[serde(default)]
    pub signal_percent: Option<u8>,
    #[serde(default)]
    pub signal_dbm: Option<i16>,
    #[serde(default)]
    pub min_temperature_c: Option<f32>,
    #[serde(default)]
    pub max_temperature_c: Option<f32>,
    #[serde(default)]
    pub uptime_seconds: Option<u64>,
    #[serde(default)]
    pub imei: Option<String>,
    #[serde(default)]
    pub iccid: Option<String>,
    #[serde(default)]
    pub imsi: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub cpu_percent: Option<f32>,
    #[serde(default)]
    pub memory_percent: Option<f32>,
    #[serde(default)]
    pub feature_snapshot: Option<DeviceFeatureSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DeviceFeatureSnapshot {
    #[serde(default)]
    pub sim: Value,
    #[serde(default)]
    pub esim: Value,
    #[serde(default)]
    pub cellular: Value,
    #[serde(default)]
    pub device_network: Value,
    #[serde(default)]
    pub vowifi: Value,
    #[serde(default)]
    pub volte: Value,
    #[serde(default)]
    pub phone: Value,
    #[serde(default)]
    pub system: Value,
    #[serde(default)]
    pub local_notifications: Value,
    #[serde(default)]
    pub local_automation: Value,
    #[serde(default)]
    pub policy_status: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeviceActionCommandPayload {
    pub action: DeviceAction,
    #[serde(default)]
    pub parameters: Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeviceAction {
    RestartBaseband,
    RebootDevice,
    SetDataEnabled,
    SetRoamingEnabled,
    SetAirplaneMode,
    SetRadioMode,
    SetApn,
    EsimEnableProfile,
    EsimDisableProfile,
    EsimDeleteProfile,
    CallDial,
    CallHangupAll,
}

impl DeviceAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RestartBaseband => "restart_baseband",
            Self::RebootDevice => "reboot_device",
            Self::SetDataEnabled => "set_data_enabled",
            Self::SetRoamingEnabled => "set_roaming_enabled",
            Self::SetAirplaneMode => "set_airplane_mode",
            Self::SetRadioMode => "set_radio_mode",
            Self::SetApn => "set_apn",
            Self::EsimEnableProfile => "esim_enable_profile",
            Self::EsimDisableProfile => "esim_disable_profile",
            Self::EsimDeleteProfile => "esim_delete_profile",
            Self::CallDial => "call_dial",
            Self::CallHangupAll => "call_hangup_all",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeviceStatus {
    Online,
    Offline,
    /// Kept for compatibility with older Agents; Hub projects it as online and uses layer status for health.
    Degraded,
    IdentityConflict,
    Disabled,
}

impl DeviceStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Online => "online",
            Self::Offline => "offline",
            Self::Degraded => "degraded",
            Self::IdentityConflict => "identity_conflict",
            Self::Disabled => "disabled",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LayerStatus {
    Ok,
    Warning,
    Error,
    Unknown,
    Unavailable,
}

impl LayerStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Unknown => "unknown",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageAckPayload {
    pub source_message_id: String,
    pub items: Vec<ItemAck>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ItemAck {
    pub item_id: String,
    pub accepted: bool,
    #[serde(default)]
    pub error_code: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hub_handled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommandPayload {
    pub command_id: String,
    pub trace_id: String,
    pub command_type: String,
    pub expires_at: DateTime<Utc>,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SendSmsCommandPayload {
    pub phone_number: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigSyncPayload {
    pub desired_version: i64,
    pub desired_hash: String,
    pub schema_version: u16,
    pub managed_policy: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigApplyResultPayload {
    pub desired_version: i64,
    pub desired_hash: String,
    pub status: ConfigApplyStatus,
    pub applied_at: DateTime<Utc>,
    #[serde(default)]
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OtaUpdateCommandPayload {
    pub package_id: String,
    pub source: String,
    pub target_version: String,
    pub architecture: String,
    pub sha256: Option<String>,
    pub download_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeviceApiRequestPayload {
    pub method: String,
    pub path: String,
    #[serde(default)]
    pub body: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeviceApiResponsePayload {
    pub status: u16,
    #[serde(default)]
    pub body: Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConfigApplyStatus {
    Applied,
    Failed,
}

impl ConfigApplyStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommandAckPayload {
    pub command_id: String,
    pub accepted: bool,
    pub acknowledged_at: DateTime<Utc>,
    #[serde(default)]
    pub error_code: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommandResultPayload {
    pub command_id: String,
    pub status: CommandResultStatus,
    pub finished_at: DateTime<Utc>,
    #[serde(default)]
    pub result: Value,
    #[serde(default)]
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommandResultStatus {
    Succeeded,
    Failed,
    Unknown,
}

impl CommandResultStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Unknown => "unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_uses_type_field_and_round_trips() {
        let envelope = Envelope::new(
            "heartbeat",
            "agent-1",
            None,
            None,
            HeartbeatPayload {
                agent_type: AgentType::Simadmin,
                agent_version: "1.1.9".into(),
                session_generation: 4,
                managed_device_count: 1,
                local_queue_size: 0,
                timestamp: Utc::now(),
                host_summary: None,
            },
        )
        .expect("serialize payload");

        let json = serde_json::to_value(&envelope).expect("serialize envelope");
        assert_eq!(json["type"], "heartbeat");
        assert_eq!(json["protocol_version"], PROTOCOL_VERSION);

        let decoded: Envelope = serde_json::from_value(json).expect("decode envelope");
        let payload: HeartbeatPayload = decoded.decode_payload().expect("decode payload");
        assert_eq!(payload.session_generation, 4);
    }
}
