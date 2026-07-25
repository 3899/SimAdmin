//! 配置管理模块
//!
//! 使用 JSON 文件存储用户配置，支持热更新

use serde::{de::Error as DeError, Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tracing::{info, warn};

use crate::models::WorkMode;

/// Webhook 配置
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebhookConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub url: String,
    #[serde(default = "default_true")]
    pub forward_sms: bool,
    #[serde(default = "default_true")]
    pub forward_calls: bool,
    #[serde(default = "default_true")]
    pub forward_ddns: bool,
    #[serde(default = "default_true")]
    pub forward_updates: bool,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub secret: String, // 可选的签名密钥
    #[serde(default = "default_sms_template")]
    pub sms_template: String, // 短信 payload 模板
    #[serde(default = "default_call_template")]
    pub call_template: String, // 通话 payload 模板
    #[serde(default = "default_ddns_template")]
    pub ddns_template: String, // DDNS payload 模板
    #[serde(default = "default_update_template")]
    pub update_template: String, // 版本更新 payload 模板
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageChannelConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub forward_sms: bool,
    #[serde(default = "default_true")]
    pub forward_calls: bool,
    #[serde(default = "default_true")]
    pub forward_ddns: bool,
    #[serde(default = "default_true")]
    pub forward_updates: bool,
    #[serde(default = "default_plain_sms_template")]
    pub sms_template: String,
    #[serde(default = "default_plain_call_template")]
    pub call_template: String,
    #[serde(default = "default_plain_ddns_template")]
    pub ddns_template: String,
    #[serde(default = "default_plain_update_template")]
    pub update_template: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BarkConfig {
    #[serde(flatten)]
    pub common: MessageChannelConfig,
    #[serde(default = "default_bark_server_url")]
    pub server_url: String,
    #[serde(default)]
    pub device_key: String,
    #[serde(default = "default_sms_title_template")]
    pub title_template: String,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub sound: String,
    #[serde(default)]
    pub level: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub click_url: String,
    #[serde(default)]
    pub copy: String,
    #[serde(default)]
    pub auto_copy: bool,
    #[serde(default = "default_true")]
    pub save_history: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushPlusConfig {
    #[serde(flatten)]
    pub common: MessageChannelConfig,
    #[serde(default)]
    pub token: String,
    #[serde(default = "default_sms_title_template")]
    pub title_template: String,
    #[serde(default)]
    pub topic: String,
    #[serde(default = "default_pushplus_template")]
    pub template: String,
    #[serde(default)]
    pub channel: String,
    #[serde(default)]
    pub option: String,
    #[serde(default)]
    pub callback_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WecomAppConfig {
    #[serde(flatten)]
    pub common: MessageChannelConfig,
    #[serde(default = "default_wecom_api_base_url")]
    pub api_base_url: String,
    #[serde(default)]
    pub corp_id: String,
    #[serde(default)]
    pub agent_id: String,
    #[serde(default)]
    pub secret: String,
    #[serde(default = "default_wecom_to_user")]
    pub to_user: String,
    #[serde(default)]
    pub to_party: String,
    #[serde(default)]
    pub to_tag: String,
    #[serde(default)]
    pub safe: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WecomRobotConfig {
    #[serde(flatten)]
    pub common: MessageChannelConfig,
    #[serde(default)]
    pub webhook_url: String,
    #[serde(default)]
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DingtalkRobotConfig {
    #[serde(flatten)]
    pub common: MessageChannelConfig,
    #[serde(default)]
    pub webhook_url: String,
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub secret: String,
    #[serde(default)]
    pub at_mobiles: String,
    #[serde(default)]
    pub at_all: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DingtalkAppConfig {
    #[serde(flatten)]
    pub common: MessageChannelConfig,
    #[serde(default)]
    pub app_key: String,
    #[serde(default)]
    pub app_secret: String,
    #[serde(default)]
    pub robot_code: String,
    #[serde(default)]
    pub open_conversation_id: String,
    #[serde(default = "default_dingtalk_msg_key")]
    pub msg_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeishuRobotConfig {
    #[serde(flatten)]
    pub common: MessageChannelConfig,
    #[serde(default)]
    pub webhook_url: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub secret: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    #[serde(flatten)]
    pub common: MessageChannelConfig,
    #[serde(default = "default_telegram_api_base_url")]
    pub api_base_url: String,
    #[serde(default)]
    pub bot_token: String,
    #[serde(default)]
    pub chat_id: String,
    #[serde(default)]
    pub parse_mode: String,
    #[serde(default)]
    pub disable_web_page_preview: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerChan3Config {
    #[serde(flatten)]
    pub common: MessageChannelConfig,
    #[serde(default)]
    pub send_key: String,
    #[serde(default)]
    pub uid: String,
    #[serde(default)]
    pub channel: String,
    #[serde(default)]
    pub openid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailConfig {
    #[serde(flatten)]
    pub common: MessageChannelConfig,
    #[serde(default)]
    pub smtp_host: String,
    #[serde(default = "default_smtp_port")]
    pub smtp_port: u16,
    #[serde(default = "default_smtp_security")]
    pub smtp_security: String,
    #[serde(default, alias = "allow_insecure_connections")]
    pub allow_insecure_tls: bool,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub sender_address: String,
    #[serde(default)]
    pub sender_name: String,
    #[serde(default, alias = "receiver_address")]
    pub receiver_addresses: String,
    #[serde(default = "default_email_message_format")]
    pub message_format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyNotificationConfig {
    #[serde(default)]
    pub webhook: WebhookConfig,
    #[serde(default)]
    pub bark: BarkConfig,
    #[serde(default)]
    pub pushplus: PushPlusConfig,
    #[serde(default)]
    pub wecom_app: WecomAppConfig,
    #[serde(default)]
    pub wecom_robot: WecomRobotConfig,
    #[serde(default)]
    pub dingtalk_robot: DingtalkRobotConfig,
    #[serde(default)]
    pub dingtalk_app: DingtalkAppConfig,
    #[serde(default)]
    pub feishu_robot: FeishuRobotConfig,
    #[serde(default)]
    pub telegram: TelegramConfig,
    #[serde(default)]
    pub serverchan3: ServerChan3Config,
    #[serde(default)]
    pub email: EmailConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationChannel {
    Webhook,
    Bark,
    #[serde(rename = "pushplus", alias = "push_plus")]
    PushPlus,
    WecomApp,
    WecomRobot,
    DingtalkRobot,
    DingtalkApp,
    FeishuRobot,
    Telegram,
    Email,
    #[serde(rename = "serverchan3", alias = "server_chan3")]
    ServerChan3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationEventType {
    Sms,
    Ddns,
    VersionUpdate,
    SystemEvent,
    DeviceStatus,
    Automation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatcherOperator {
    Always,
    Contains,
    NotContains,
    Equals,
    Regex,
}

fn default_matcher_operator() -> MatcherOperator {
    MatcherOperator::Always
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleMatcher {
    #[serde(default)]
    pub field: String,
    #[serde(default = "default_matcher_operator")]
    pub operator: MatcherOperator,
    #[serde(default)]
    pub value: String,
}

impl Default for RuleMatcher {
    fn default() -> Self {
        Self {
            field: "summary".to_string(),
            operator: MatcherOperator::Always,
            value: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuietHoursSchedule {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub weekdays: Vec<u8>,
    #[serde(default = "default_quiet_start")]
    pub start: String,
    #[serde(default = "default_quiet_end")]
    pub end: String,
}

fn default_quiet_start() -> String {
    "22:00".to_string()
}

fn default_quiet_end() -> String {
    "08:00".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceStatusSchedule {
    #[serde(default = "default_device_status_schedule_mode")]
    pub mode: String,
    #[serde(default = "default_device_status_interval_minutes")]
    pub interval_minutes: u32,
    #[serde(default = "default_device_status_weekdays")]
    pub weekdays: Vec<u8>,
    #[serde(default = "default_device_status_times")]
    pub times: Vec<String>,
}

impl Default for DeviceStatusSchedule {
    fn default() -> Self {
        Self {
            mode: default_device_status_schedule_mode(),
            interval_minutes: default_device_status_interval_minutes(),
            weekdays: default_device_status_weekdays(),
            times: default_device_status_times(),
        }
    }
}

fn default_device_status_schedule_mode() -> String {
    "fixed".to_string()
}

fn default_device_status_interval_minutes() -> u32 {
    24 * 60
}

fn default_device_status_weekdays() -> Vec<u8> {
    vec![1, 2, 3, 4, 5, 6, 7]
}

fn default_device_status_times() -> Vec<String> {
    vec!["09:00".to_string()]
}

fn default_device_status_sms_period() -> String {
    "last_24h".to_string()
}

pub fn default_device_status_items() -> Vec<String> {
    [
        "device_power",
        "device_model",
        "system_version",
        "uptime",
        "work_mode",
        "sim_present",
        "sim_operator",
        "cellular_registration",
        "cellular_operator",
        "cellular_technology",
        "signal_strength",
        "data_connection",
        "airplane_mode",
        "roaming",
        "ipv4_connectivity",
        "ipv6_connectivity",
        "default_route",
        "default_ip",
        "wlan_enabled",
        "wlan_connected",
        "wlan_ssid",
        "key_interfaces",
        "cellular_traffic",
        "cpu_usage",
        "memory_usage",
        "root_disk",
        "top_temperatures",
        "service_version",
        "ddns_status",
        "ota_status",
        "forwarding_channels",
        "forwarding_rules",
        "sms_forwarding_stats",
    ]
    .into_iter()
    .map(ToString::to_string)
    .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationRule {
    pub id: String,
    #[serde(rename = "type")]
    pub event_type: NotificationEventType,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub matcher: RuleMatcher,
    #[serde(default)]
    pub channel_ids: Vec<String>,
    #[serde(default)]
    pub event_codes: Vec<String>,
    #[serde(default)]
    pub title_template: String,
    #[serde(default)]
    pub template: String,
    #[serde(default)]
    pub quiet_hours: Vec<QuietHoursSchedule>,
    #[serde(default = "default_ddns_failure_threshold")]
    pub ddns_failure_threshold: u32,
    #[serde(default = "default_device_status_items")]
    pub device_status_items: Vec<String>,
    #[serde(default)]
    pub device_status_schedule: DeviceStatusSchedule,
    #[serde(default = "default_device_status_sms_period")]
    pub device_status_sms_period: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationChannelInstance {
    pub id: String,
    #[serde(rename = "type")]
    pub channel_type: NotificationChannel,
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub rate_limit: NotificationRateLimitConfig,
    #[serde(default)]
    pub config: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationRateLimitConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_notification_rate_limit_max_messages")]
    pub max_messages: u32,
    #[serde(default = "default_notification_rate_limit_window_seconds")]
    pub window_seconds: u32,
}

impl Default for NotificationRateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_messages: default_notification_rate_limit_max_messages(),
            window_seconds: default_notification_rate_limit_window_seconds(),
        }
    }
}

fn default_notification_rate_limit_max_messages() -> u32 {
    20
}

fn default_notification_rate_limit_window_seconds() -> u32 {
    60
}

fn default_ddns_failure_threshold() -> u32 {
    1
}

fn default_notification_log_retention_days() -> u32 {
    90
}

fn default_notification_log_max_entries() -> u32 {
    10_000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationLogCleanupConfig {
    #[serde(default = "default_true")]
    pub retention_days_enabled: bool,
    #[serde(default = "default_notification_log_retention_days")]
    pub retention_days: u32,
    #[serde(default = "default_true")]
    pub max_entries_enabled: bool,
    #[serde(default = "default_notification_log_max_entries")]
    pub max_entries: u32,
}

impl Default for NotificationLogCleanupConfig {
    fn default() -> Self {
        Self {
            retention_days_enabled: true,
            retention_days: default_notification_log_retention_days(),
            max_entries_enabled: true,
            max_entries: default_notification_log_max_entries(),
        }
    }
}

fn default_notification_version() -> u8 {
    2
}

#[derive(Debug, Clone, Serialize)]
pub struct NotificationConfig {
    #[serde(default = "default_notification_version")]
    pub version: u8,
    #[serde(default)]
    pub channels: Vec<NotificationChannelInstance>,
    #[serde(default)]
    pub rules: Vec<NotificationRule>,
    #[serde(default)]
    pub log_cleanup: NotificationLogCleanupConfig,
}

#[derive(Deserialize)]
struct NotificationConfigV2 {
    #[serde(default = "default_notification_version", rename = "version")]
    _version: u8,
    #[serde(default)]
    channels: Vec<NotificationChannelInstance>,
    #[serde(default)]
    rules: Vec<NotificationRule>,
    #[serde(default)]
    log_cleanup: NotificationLogCleanupConfig,
}

impl<'de> Deserialize<'de> for NotificationConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let is_v2 = value.get("channels").is_some() || value.get("rules").is_some();
        if is_v2 {
            let parsed: NotificationConfigV2 =
                serde_json::from_value(value).map_err(D::Error::custom)?;
            return Ok(Self {
                version: 2,
                channels: parsed.channels,
                rules: parsed.rules,
                log_cleanup: parsed.log_cleanup,
            });
        }

        let legacy: LegacyNotificationConfig =
            serde_json::from_value(value).map_err(D::Error::custom)?;
        Ok(Self::from_legacy(legacy))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceNetworkConfig {
    #[serde(default)]
    pub ddns: DdnsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct VersionUpdateNotificationConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub proxy_prefix: String,
    #[serde(default)]
    pub last_notified_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SecurityConfig {
    #[serde(default)]
    pub password_protection_enabled: bool,
    #[serde(default = "default_password_min_length")]
    pub password_min_length: u8,
    #[serde(default = "default_true")]
    pub password_require_letters: bool,
    #[serde(default = "default_true")]
    pub password_require_digits: bool,
    #[serde(default = "default_true")]
    pub password_require_symbols: bool,
    #[serde(default = "default_session_ttl_seconds")]
    pub session_ttl_seconds: i64,
    #[serde(default = "default_idle_timeout_seconds")]
    pub idle_timeout_seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DdnsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_ddns_provider")]
    pub provider: String,
    #[serde(default)]
    pub access_id: String,
    #[serde(default)]
    pub access_secret: String,
    #[serde(default = "default_ddns_interval_seconds")]
    pub interval_seconds: u64,
    #[serde(default = "default_ddns_ttl")]
    pub ttl: u32,
    #[serde(default)]
    pub ipv4: DdnsIpConfig,
    #[serde(default = "default_ddns_ipv6_config")]
    pub ipv6: DdnsIpConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DdnsIpConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_ddns_get_type")]
    pub get_type: String,
    #[serde(default)]
    pub interface_name: String,
    #[serde(default)]
    pub urls: Vec<String>,
    #[serde(default)]
    pub domains: Vec<String>,
}

fn default_true() -> bool {
    true
}

/// 默认短信模板
fn default_sms_template() -> String {
    r#"{
  "msg_type": "text",
  "content": {
    "text": "📱 短信通知\n号码: {{phone_number}}\n内容: {{content}}\n时间: {{timestamp}}\n来源: {{own_number}}"
  }
}"#
    .to_string()
}

/// 默认通话模板
fn default_call_template() -> String {
    r#"{
  "msg_type": "text",
  "content": {
    "text": "📞 来电通知\n号码: {{phone_number}}\n类型: {{direction}}\n时间: {{start_time}}\n时长: {{duration}}秒\n已接听: {{answered}}"
  }
}"#.to_string()
}

fn default_ddns_template() -> String {
    r#"{
  "msg_type": "text",
  "content": {
    "text": "SimAdmin DDNS 通知\n域名: {{domains}}\nIP类型: {{ip_type}}\n新IP: {{new_ip}}\n旧IP: {{old_ip}}\n服务商: {{provider}}\n记录类型: {{record_type}}\n状态: {{status}}\n消息: {{message}}\n更新时间: {{timestamp}}"
  }
}"#
    .to_string()
}

fn default_update_template() -> String {
    r#"{
  "msg_type": "text",
  "content": {
    "text": "🚀 SimAdmin 发现新版本\n固件包: {{asset_name}}\n版本号: {{version}}\n时间: {{time}}\n来源: {{own_number}}\n\n请前往 OTA 在线更新模块检测版本，一键下载并升级。"
  }
}"#
    .to_string()
}

fn default_plain_sms_template() -> String {
    "📱 短信通知\n号码: {{发送方号码}}\n内容: {{短信内容}}\n时间: {{时间}}\n来源: {{本机号码}}"
        .to_string()
}

fn default_plain_call_template() -> String {
    "📞 来电通知\n号码: {{phone_number}}\n类型: {{direction}}\n时间: {{start_time}}\n时长: {{duration}}秒\n已接听: {{answered}}".to_string()
}

fn default_plain_ddns_template() -> String {
    "SimAdmin DDNS 通知\n域名: {{域名}}\nIP类型: {{IP类型}}\n新IP: {{新IP}}\n旧IP: {{旧IP}}\n服务商: {{服务商}}\n记录类型: {{记录类型}}\n状态: {{状态}}\n消息: {{消息}}\n更新时间: {{更新时间}}".to_string()
}

fn default_plain_update_template() -> String {
    "🚀 SimAdmin 发现新版本\n固件包: {{固件包}}\n版本号: {{版本号}}\n时间: {{时间}}\n来源: {{本机号码}}\n\n请前往 OTA 在线更新模块检测版本，一键下载并升级。".to_string()
}

fn default_sms_title_template() -> String {
    "SimAdmin 短信通知".to_string()
}

fn default_bark_server_url() -> String {
    "https://api.day.app".to_string()
}

fn default_pushplus_template() -> String {
    "txt".to_string()
}

fn default_wecom_to_user() -> String {
    "@all".to_string()
}

fn default_wecom_api_base_url() -> String {
    "https://qyapi.weixin.qq.com".to_string()
}

fn default_telegram_api_base_url() -> String {
    String::new()
}

fn default_dingtalk_msg_key() -> String {
    "sampleText".to_string()
}

fn default_smtp_port() -> u16 {
    465
}

fn default_smtp_security() -> String {
    "implicit_tls".to_string()
}

fn default_email_message_format() -> String {
    "plain".to_string()
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            url: String::new(),
            forward_sms: true,
            forward_calls: true,
            forward_ddns: true,
            forward_updates: true,
            headers: HashMap::new(),
            secret: String::new(),
            sms_template: default_sms_template(),
            call_template: default_call_template(),
            ddns_template: default_ddns_template(),
            update_template: default_update_template(),
        }
    }
}

impl Default for MessageChannelConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            forward_sms: true,
            forward_calls: true,
            forward_ddns: true,
            forward_updates: true,
            sms_template: default_plain_sms_template(),
            call_template: default_plain_call_template(),
            ddns_template: default_plain_ddns_template(),
            update_template: default_plain_update_template(),
        }
    }
}

impl Default for BarkConfig {
    fn default() -> Self {
        Self {
            common: MessageChannelConfig::default(),
            server_url: default_bark_server_url(),
            device_key: String::new(),
            title_template: default_sms_title_template(),
            group: String::new(),
            sound: String::new(),
            level: String::new(),
            icon: String::new(),
            click_url: String::new(),
            copy: String::new(),
            auto_copy: false,
            save_history: true,
        }
    }
}

impl Default for PushPlusConfig {
    fn default() -> Self {
        Self {
            common: MessageChannelConfig::default(),
            token: String::new(),
            title_template: default_sms_title_template(),
            topic: String::new(),
            template: default_pushplus_template(),
            channel: String::new(),
            option: String::new(),
            callback_url: String::new(),
        }
    }
}

impl Default for WecomAppConfig {
    fn default() -> Self {
        Self {
            common: MessageChannelConfig::default(),
            api_base_url: default_wecom_api_base_url(),
            corp_id: String::new(),
            agent_id: String::new(),
            secret: String::new(),
            to_user: default_wecom_to_user(),
            to_party: String::new(),
            to_tag: String::new(),
            safe: false,
        }
    }
}

impl Default for WecomRobotConfig {
    fn default() -> Self {
        Self {
            common: MessageChannelConfig::default(),
            webhook_url: String::new(),
            key: String::new(),
        }
    }
}

impl Default for DingtalkRobotConfig {
    fn default() -> Self {
        Self {
            common: MessageChannelConfig::default(),
            webhook_url: String::new(),
            access_token: String::new(),
            secret: String::new(),
            at_mobiles: String::new(),
            at_all: false,
        }
    }
}

impl Default for DingtalkAppConfig {
    fn default() -> Self {
        Self {
            common: MessageChannelConfig::default(),
            app_key: String::new(),
            app_secret: String::new(),
            robot_code: String::new(),
            open_conversation_id: String::new(),
            msg_key: default_dingtalk_msg_key(),
        }
    }
}

impl Default for FeishuRobotConfig {
    fn default() -> Self {
        Self {
            common: MessageChannelConfig::default(),
            webhook_url: String::new(),
            token: String::new(),
            secret: String::new(),
        }
    }
}

impl Default for TelegramConfig {
    fn default() -> Self {
        Self {
            common: MessageChannelConfig::default(),
            api_base_url: default_telegram_api_base_url(),
            bot_token: String::new(),
            chat_id: String::new(),
            parse_mode: String::new(),
            disable_web_page_preview: true,
        }
    }
}

impl Default for ServerChan3Config {
    fn default() -> Self {
        Self {
            common: MessageChannelConfig::default(),
            send_key: String::new(),
            uid: String::new(),
            channel: String::new(),
            openid: String::new(),
        }
    }
}

impl Default for EmailConfig {
    fn default() -> Self {
        Self {
            common: MessageChannelConfig::default(),
            smtp_host: String::new(),
            smtp_port: default_smtp_port(),
            smtp_security: default_smtp_security(),
            allow_insecure_tls: false,
            username: String::new(),
            password: String::new(),
            sender_address: String::new(),
            sender_name: String::new(),
            receiver_addresses: String::new(),
            message_format: default_email_message_format(),
        }
    }
}

impl Default for LegacyNotificationConfig {
    fn default() -> Self {
        Self {
            webhook: WebhookConfig::default(),
            bark: BarkConfig::default(),
            pushplus: PushPlusConfig::default(),
            wecom_app: WecomAppConfig::default(),
            wecom_robot: WecomRobotConfig::default(),
            dingtalk_robot: DingtalkRobotConfig::default(),
            dingtalk_app: DingtalkAppConfig::default(),
            feishu_robot: FeishuRobotConfig::default(),
            telegram: TelegramConfig::default(),
            serverchan3: ServerChan3Config::default(),
            email: EmailConfig::default(),
        }
    }
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            version: 2,
            channels: Vec::new(),
            rules: Vec::new(),
            log_cleanup: NotificationLogCleanupConfig::default(),
        }
    }
}

struct LegacyChannelMigration {
    id: String,
    channel_type: NotificationChannel,
    name: String,
    enabled: bool,
    config: Value,
    forward_sms: bool,
    forward_ddns: bool,
    forward_updates: bool,
    sms_template: String,
    ddns_template: String,
    update_template: String,
}

impl NotificationConfig {
    pub fn from_legacy(legacy: LegacyNotificationConfig) -> Self {
        let migrations = legacy_channel_migrations(&legacy);
        let channels = migrations
            .iter()
            .map(|item| NotificationChannelInstance {
                id: item.id.clone(),
                channel_type: item.channel_type,
                name: item.name.clone(),
                enabled: item.enabled,
                rate_limit: NotificationRateLimitConfig::default(),
                config: item.config.clone(),
            })
            .collect::<Vec<_>>();

        let mut rules = Vec::new();
        push_legacy_rule(
            &mut rules,
            NotificationEventType::Sms,
            "默认短信转发",
            "legacy-sms",
            &migrations,
        );
        push_legacy_rule(
            &mut rules,
            NotificationEventType::Ddns,
            "默认 DDNS 转发",
            "legacy-ddns",
            &migrations,
        );
        push_legacy_rule(
            &mut rules,
            NotificationEventType::VersionUpdate,
            "默认版本更新转发",
            "legacy-version-update",
            &migrations,
        );

        Self {
            version: 2,
            channels,
            rules,
            log_cleanup: NotificationLogCleanupConfig::default(),
        }
    }

    pub fn first_webhook_config(&self) -> Option<WebhookConfig> {
        self.channels
            .iter()
            .find(|channel| channel.channel_type == NotificationChannel::Webhook)
            .and_then(|channel| serde_json::from_value(channel.config.clone()).ok())
    }
}

fn channel_label(channel: NotificationChannel) -> &'static str {
    match channel {
        NotificationChannel::Webhook => "Webhook",
        NotificationChannel::Bark => "Bark",
        NotificationChannel::PushPlus => "PushPlus",
        NotificationChannel::WecomApp => "企业微信应用消息",
        NotificationChannel::WecomRobot => "企业微信群机器人",
        NotificationChannel::DingtalkRobot => "钉钉群自定义机器人",
        NotificationChannel::DingtalkApp => "钉钉企业内机器人",
        NotificationChannel::FeishuRobot => "飞书机器人",
        NotificationChannel::Telegram => "Telegram 机器人",
        NotificationChannel::Email => "Email",
        NotificationChannel::ServerChan3 => "Server酱3",
    }
}

fn config_value<T: Serialize>(config: &T) -> Value {
    serde_json::to_value(config).unwrap_or(Value::Object(Default::default()))
}

fn legacy_channel_migrations(legacy: &LegacyNotificationConfig) -> Vec<LegacyChannelMigration> {
    let mut channels = Vec::new();

    if legacy.webhook.enabled || !legacy.webhook.url.trim().is_empty() {
        channels.push(LegacyChannelMigration {
            id: "webhook-1".to_string(),
            channel_type: NotificationChannel::Webhook,
            name: channel_label(NotificationChannel::Webhook).to_string(),
            enabled: legacy.webhook.enabled,
            config: config_value(&legacy.webhook),
            forward_sms: legacy.webhook.forward_sms,
            forward_ddns: legacy.webhook.forward_ddns,
            forward_updates: legacy.webhook.forward_updates,
            sms_template: webhook_text_template(
                &legacy.webhook.sms_template,
                &default_rule_template(NotificationEventType::Sms),
            ),
            ddns_template: webhook_text_template(
                &legacy.webhook.ddns_template,
                &default_rule_template(NotificationEventType::Ddns),
            ),
            update_template: webhook_text_template(
                &legacy.webhook.update_template,
                &default_rule_template(NotificationEventType::VersionUpdate),
            ),
        });
    }

    push_message_channel_migration(
        &mut channels,
        NotificationChannel::Bark,
        "bark-1",
        &legacy.bark.common,
        &legacy.bark,
        legacy.bark.common.enabled || !legacy.bark.device_key.trim().is_empty(),
    );
    push_message_channel_migration(
        &mut channels,
        NotificationChannel::PushPlus,
        "pushplus-1",
        &legacy.pushplus.common,
        &legacy.pushplus,
        legacy.pushplus.common.enabled || !legacy.pushplus.token.trim().is_empty(),
    );
    push_message_channel_migration(
        &mut channels,
        NotificationChannel::WecomApp,
        "wecom-app-1",
        &legacy.wecom_app.common,
        &legacy.wecom_app,
        legacy.wecom_app.common.enabled
            || !legacy.wecom_app.corp_id.trim().is_empty()
            || !legacy.wecom_app.agent_id.trim().is_empty()
            || !legacy.wecom_app.secret.trim().is_empty(),
    );
    push_message_channel_migration(
        &mut channels,
        NotificationChannel::WecomRobot,
        "wecom-robot-1",
        &legacy.wecom_robot.common,
        &legacy.wecom_robot,
        legacy.wecom_robot.common.enabled
            || !legacy.wecom_robot.webhook_url.trim().is_empty()
            || !legacy.wecom_robot.key.trim().is_empty(),
    );
    push_message_channel_migration(
        &mut channels,
        NotificationChannel::DingtalkRobot,
        "dingtalk-robot-1",
        &legacy.dingtalk_robot.common,
        &legacy.dingtalk_robot,
        legacy.dingtalk_robot.common.enabled
            || !legacy.dingtalk_robot.webhook_url.trim().is_empty()
            || !legacy.dingtalk_robot.access_token.trim().is_empty(),
    );
    push_message_channel_migration(
        &mut channels,
        NotificationChannel::DingtalkApp,
        "dingtalk-app-1",
        &legacy.dingtalk_app.common,
        &legacy.dingtalk_app,
        legacy.dingtalk_app.common.enabled
            || !legacy.dingtalk_app.app_key.trim().is_empty()
            || !legacy.dingtalk_app.app_secret.trim().is_empty()
            || !legacy.dingtalk_app.open_conversation_id.trim().is_empty(),
    );
    push_message_channel_migration(
        &mut channels,
        NotificationChannel::FeishuRobot,
        "feishu-robot-1",
        &legacy.feishu_robot.common,
        &legacy.feishu_robot,
        legacy.feishu_robot.common.enabled
            || !legacy.feishu_robot.webhook_url.trim().is_empty()
            || !legacy.feishu_robot.token.trim().is_empty(),
    );
    push_message_channel_migration(
        &mut channels,
        NotificationChannel::Telegram,
        "telegram-1",
        &legacy.telegram.common,
        &legacy.telegram,
        legacy.telegram.common.enabled
            || !legacy.telegram.bot_token.trim().is_empty()
            || !legacy.telegram.chat_id.trim().is_empty(),
    );
    push_message_channel_migration(
        &mut channels,
        NotificationChannel::ServerChan3,
        "serverchan3-1",
        &legacy.serverchan3.common,
        &legacy.serverchan3,
        legacy.serverchan3.common.enabled
            || !legacy.serverchan3.send_key.trim().is_empty()
            || !legacy.serverchan3.uid.trim().is_empty(),
    );
    push_message_channel_migration(
        &mut channels,
        NotificationChannel::Email,
        "email-1",
        &legacy.email.common,
        &legacy.email,
        legacy.email.common.enabled
            || !legacy.email.smtp_host.trim().is_empty()
            || !legacy.email.sender_address.trim().is_empty()
            || !legacy.email.receiver_addresses.trim().is_empty(),
    );

    channels
}

fn push_message_channel_migration<T: Serialize>(
    channels: &mut Vec<LegacyChannelMigration>,
    channel_type: NotificationChannel,
    id: &str,
    common: &MessageChannelConfig,
    config: &T,
    configured: bool,
) {
    if !configured {
        return;
    }
    channels.push(LegacyChannelMigration {
        id: id.to_string(),
        channel_type,
        name: channel_label(channel_type).to_string(),
        enabled: common.enabled,
        config: config_value(config),
        forward_sms: common.forward_sms,
        forward_ddns: common.forward_ddns,
        forward_updates: common.forward_updates,
        sms_template: non_empty_template(&common.sms_template, NotificationEventType::Sms),
        ddns_template: non_empty_template(&common.ddns_template, NotificationEventType::Ddns),
        update_template: non_empty_template(
            &common.update_template,
            NotificationEventType::VersionUpdate,
        ),
    });
}

fn push_legacy_rule(
    rules: &mut Vec<NotificationRule>,
    event_type: NotificationEventType,
    name: &str,
    id: &str,
    channels: &[LegacyChannelMigration],
) {
    let selected = channels
        .iter()
        .filter(|channel| match event_type {
            NotificationEventType::Sms => channel.forward_sms,
            NotificationEventType::Ddns => channel.forward_ddns,
            NotificationEventType::VersionUpdate => channel.forward_updates,
            NotificationEventType::SystemEvent => false,
            NotificationEventType::DeviceStatus => false,
            NotificationEventType::Automation => false,
        })
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return;
    }

    let template = selected
        .first()
        .map(|channel| match event_type {
            NotificationEventType::Sms => channel.sms_template.clone(),
            NotificationEventType::Ddns => channel.ddns_template.clone(),
            NotificationEventType::VersionUpdate => channel.update_template.clone(),
            NotificationEventType::SystemEvent => String::new(),
            NotificationEventType::DeviceStatus => String::new(),
            NotificationEventType::Automation => String::new(),
        })
        .unwrap_or_else(|| default_rule_template(event_type));

    rules.push(NotificationRule {
        id: id.to_string(),
        event_type,
        name: name.to_string(),
        enabled: true,
        matcher: RuleMatcher::default(),
        channel_ids: selected
            .into_iter()
            .map(|channel| channel.id.clone())
            .collect(),
        event_codes: Vec::new(),
        title_template: default_rule_title_template(event_type),
        template,
        quiet_hours: Vec::new(),
        ddns_failure_threshold: default_ddns_failure_threshold(),
        device_status_items: default_device_status_items(),
        device_status_schedule: DeviceStatusSchedule::default(),
        device_status_sms_period: default_device_status_sms_period(),
    });
}

fn non_empty_template(template: &str, event_type: NotificationEventType) -> String {
    if template.trim().is_empty() {
        default_rule_template(event_type)
    } else {
        template.to_string()
    }
}

fn webhook_text_template(template: &str, fallback: &str) -> String {
    if template.trim().is_empty() {
        return fallback.to_string();
    }
    if let Ok(value) = serde_json::from_str::<Value>(template) {
        if let Some(text) = value
            .get("content")
            .and_then(|content| content.get("text"))
            .and_then(Value::as_str)
        {
            return text.replace("\\n", "\n");
        }
        if let Some(text) = value.get("text").and_then(Value::as_str) {
            return text.replace("\\n", "\n");
        }
    }
    template.to_string()
}

pub fn default_rule_template(event_type: NotificationEventType) -> String {
    match event_type {
        NotificationEventType::Sms => {
            "📱 短信通知\n号码: {{发送方号码}}\n内容: {{短信内容}}\n时间: {{时间}}\n来源: {{本机号码}}".to_string()
        }
        NotificationEventType::Ddns => {
            "DDNS 通知\n域名: {{域名}}\nIP 类型: {{IP类型}}\n新 IP: {{新IP}}\n旧 IP: {{旧IP}}\n服务商: {{服务商}}\n记录类型: {{记录类型}}\n状态: {{状态}}\n消息: {{消息}}\n更新时间: {{更新时间}}".to_string()
        }
        NotificationEventType::VersionUpdate => {
            "🚀 SimAdmin 发现新版本\n固件包: {{固件包}}\n版本号: {{版本号}}\n时间: {{时间}}\n来源: {{本机号码}}".to_string()
        }
        NotificationEventType::SystemEvent => {
            "系统事件通知\n分类: {{分类}}\n事件: {{事件}}\n等级: {{等级}}\n状态: {{状态}}\n对象: {{对象}}\n消息: {{消息}}\n时间: {{时间}}".to_string()
        }
        NotificationEventType::DeviceStatus => {
            "设备状态报告\n【{{状态分类}}】\n{{状态内容}}\n\n时间: {{时间}}".to_string()
        }
        NotificationEventType::Automation => {
            "🤖 自动化事件通知\n任务名称: {{任务名称}}\n任务类型: {{任务类型}}\n执行状态: {{任务状态}}\n详情: {{任务详情}}\n时间: {{触发时间}}\n来源: {{本机号码}}".to_string()
        }
    }
}

pub fn default_rule_title_template(event_type: NotificationEventType) -> String {
    match event_type {
        NotificationEventType::Sms => "{{发送方号码}}：验证码{{验证码}}".to_string(),
        NotificationEventType::Ddns => "DDNS通知：{{本机号码}}".to_string(),
        NotificationEventType::VersionUpdate => "版本更新通知：{{本机号码}}".to_string(),
        NotificationEventType::SystemEvent => "{{分类}}事件：{{本机号码}}".to_string(),
        NotificationEventType::DeviceStatus => "设备状态：{{本机号码}}".to_string(),
        NotificationEventType::Automation => "{{任务类型}}：{{本机号码}}".to_string(),
    }
}

impl Default for DeviceNetworkConfig {
    fn default() -> Self {
        Self {
            ddns: DdnsConfig::default(),
        }
    }
}

impl Default for VersionUpdateNotificationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            proxy_prefix: String::new(),
            last_notified_version: None,
        }
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            password_protection_enabled: false,
            password_min_length: default_password_min_length(),
            password_require_letters: true,
            password_require_digits: true,
            password_require_symbols: true,
            session_ttl_seconds: default_session_ttl_seconds(),
            idle_timeout_seconds: default_idle_timeout_seconds(),
        }
    }
}

impl Default for DdnsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: default_ddns_provider(),
            access_id: String::new(),
            access_secret: String::new(),
            interval_seconds: default_ddns_interval_seconds(),
            ttl: default_ddns_ttl(),
            ipv4: DdnsIpConfig {
                enabled: true,
                get_type: default_ddns_get_type(),
                interface_name: String::new(),
                urls: default_ddns_ipv4_urls(),
                domains: Vec::new(),
            },
            ipv6: default_ddns_ipv6_config(),
        }
    }
}

impl Default for DdnsIpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            get_type: default_ddns_get_type(),
            interface_name: String::new(),
            urls: Vec::new(),
            domains: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub tasks: Vec<AutomationTask>,
}

impl Default for AutomationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            tasks: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationTask {
    pub id: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub trigger: AutomationTrigger,
    pub action: AutomationAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "config", rename_all = "snake_case")]
pub enum AutomationTrigger {
    Fixed {
        weekdays: Vec<u8>,
        times: Vec<String>,
    },
    Interval {
        interval_value: u64,
        interval_unit: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "config", rename_all = "snake_case")]
pub enum AutomationAction {
    RestartBaseband,
    RebootDevice {
        delay_seconds: u32,
    },
    BackupData {
        components: Vec<String>,
        storage: BackupStorageConfig,
    },
    SendSms {
        phone_number: String,
        content: String,
        random_delay_seconds: Option<u32>,
        retry_limit: Option<u32>,
    },
}

fn default_backup_components() -> Vec<String> {
    [
        "config",
        "sms",
        "notification_config",
        "automation_config",
        "sim_cache",
        "esim_cache",
    ]
    .into_iter()
    .map(ToString::to_string)
    .collect()
}

fn default_backup_local_dir() -> String {
    "/opt/simadmin/backups".to_string()
}

fn default_backup_retention_days() -> u32 {
    7
}

fn default_backup_max_files() -> u32 {
    10
}

fn default_backup_interval_value() -> u64 {
    7
}

fn default_backup_interval_unit() -> String {
    "days".to_string()
}

fn default_backup_times() -> Vec<String> {
    vec!["04:00".to_string()]
}

fn default_backup_weekdays() -> Vec<u8> {
    vec![1, 2, 3, 4, 5, 6, 7]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_backup_components")]
    pub components: Vec<String>,
    #[serde(default)]
    pub schedule: BackupScheduleConfig,
    #[serde(default)]
    pub cleanup: BackupCleanupConfig,
    #[serde(default)]
    pub storage: BackupStorageConfig,
    #[serde(default)]
    pub last_run_at: String,
    #[serde(default)]
    pub last_run_key: String,
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            components: default_backup_components(),
            schedule: BackupScheduleConfig::default(),
            cleanup: BackupCleanupConfig::default(),
            storage: BackupStorageConfig::default(),
            last_run_at: String::new(),
            last_run_key: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BackupScheduleConfig {
    #[serde(default = "default_backup_schedule_mode")]
    pub mode: String,
    #[serde(default = "default_backup_weekdays")]
    pub weekdays: Vec<u8>,
    #[serde(default = "default_backup_times")]
    pub times: Vec<String>,
    #[serde(default = "default_backup_interval_value")]
    pub interval_value: u64,
    #[serde(default = "default_backup_interval_unit")]
    pub interval_unit: String,
}

fn default_backup_schedule_mode() -> String {
    "manual".to_string()
}

impl Default for BackupScheduleConfig {
    fn default() -> Self {
        Self {
            mode: default_backup_schedule_mode(),
            weekdays: default_backup_weekdays(),
            times: default_backup_times(),
            interval_value: default_backup_interval_value(),
            interval_unit: default_backup_interval_unit(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BackupCleanupConfig {
    #[serde(default = "default_true")]
    pub retention_days_enabled: bool,
    #[serde(default = "default_backup_retention_days")]
    pub retention_days: u32,
    #[serde(default = "default_true")]
    pub max_files_enabled: bool,
    #[serde(default = "default_backup_max_files")]
    pub max_files: u32,
}

impl Default for BackupCleanupConfig {
    fn default() -> Self {
        Self {
            retention_days_enabled: true,
            retention_days: default_backup_retention_days(),
            max_files_enabled: true,
            max_files: default_backup_max_files(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BackupStorageConfig {
    #[serde(default = "default_backup_local_dir")]
    pub local_dir: String,
}

impl Default for BackupStorageConfig {
    fn default() -> Self {
        Self {
            local_dir: default_backup_local_dir(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_channel_accepts_frontend_pushplus_key() {
        assert!(matches!(
            serde_json::from_str::<NotificationChannel>(r#""pushplus""#).unwrap(),
            NotificationChannel::PushPlus
        ));
        assert!(matches!(
            serde_json::from_str::<NotificationChannel>(r#""push_plus""#).unwrap(),
            NotificationChannel::PushPlus
        ));
        assert_eq!(
            serde_json::to_string(&NotificationChannel::PushPlus).unwrap(),
            r#""pushplus""#
        );
    }

    #[test]
    fn notification_channel_accepts_email_and_serverchan3_keys() {
        assert!(matches!(
            serde_json::from_str::<NotificationChannel>(r#""email""#).unwrap(),
            NotificationChannel::Email
        ));
        assert!(matches!(
            serde_json::from_str::<NotificationChannel>(r#""serverchan3""#).unwrap(),
            NotificationChannel::ServerChan3
        ));
        assert_eq!(
            serde_json::to_string(&NotificationChannel::Email).unwrap(),
            r#""email""#
        );
        assert_eq!(
            serde_json::to_string(&NotificationChannel::ServerChan3).unwrap(),
            r#""serverchan3""#
        );
    }

    #[test]
    fn email_config_accepts_legacy_receiver_address() {
        let config: EmailConfig = serde_json::from_str(
            r#"{"smtp_host":"smtp.example.com","receiver_address":"admin@example.com"}"#,
        )
        .unwrap();

        assert_eq!(config.smtp_security, "implicit_tls");
        assert_eq!(config.receiver_addresses, "admin@example.com");
    }

    #[test]
    fn legacy_notification_config_migrates_channels_and_rules() {
        let mut legacy = LegacyNotificationConfig::default();
        legacy.webhook.enabled = true;
        legacy.webhook.url = "https://example.com/hook".to_string();
        legacy.webhook.forward_sms = true;
        legacy.webhook.forward_ddns = false;
        legacy.webhook.forward_updates = true;

        let migrated = NotificationConfig::from_legacy(legacy);

        assert_eq!(migrated.version, 2);
        assert_eq!(migrated.channels.len(), 1);
        assert_eq!(migrated.channels[0].id, "webhook-1");
        assert_eq!(
            migrated.channels[0].channel_type,
            NotificationChannel::Webhook
        );
        assert!(migrated.channels[0].enabled);
        assert!(migrated
            .rules
            .iter()
            .any(|rule| rule.event_type == NotificationEventType::Sms
                && rule.channel_ids == vec!["webhook-1".to_string()]));
        assert!(!migrated
            .rules
            .iter()
            .any(|rule| rule.event_type == NotificationEventType::Ddns));
        assert!(migrated
            .rules
            .iter()
            .any(|rule| rule.event_type == NotificationEventType::VersionUpdate));
    }

    #[test]
    fn update_template_migration_removes_obsolete_commit_and_md5_lines() {
        let mut config = AppConfig::default();
        config.notifications.rules.push(NotificationRule {
            id: "rule-update".to_string(),
            event_type: NotificationEventType::VersionUpdate,
            name: "版本更新".to_string(),
            enabled: true,
            matcher: RuleMatcher::default(),
            channel_ids: Vec::new(),
            event_codes: Vec::new(),
            title_template: String::new(),
            template: "版本号: {{版本号}}\nCommit: {{Commit}}\n构建时间: {{构建时间}}\nMD5: {{MD5}}\n来源: {{本机号码}}".to_string(),
            quiet_hours: Vec::new(),
            ddns_failure_threshold: 1,
            device_status_items: default_device_status_items(),
            device_status_schedule: DeviceStatusSchedule::default(),
            device_status_sms_period: "last_24h".to_string(),
        });

        assert!(migrate_update_templates(&mut config));

        let template = &config.notifications.rules[0].template;
        assert_eq!(
            template,
            "版本号: {{版本号}}\n时间: {{时间}}\n来源: {{本机号码}}"
        );
    }
}

fn default_ddns_provider() -> String {
    "tencentcloud".to_string()
}

fn default_ddns_interval_seconds() -> u64 {
    300
}

fn default_ddns_ttl() -> u32 {
    600
}

fn default_ddns_get_type() -> String {
    "interface".to_string()
}

fn default_ddns_ipv4_urls() -> Vec<String> {
    vec![
        "https://api.ipify.org".to_string(),
        "https://ip.3322.net".to_string(),
        "https://4.ident.me".to_string(),
        "https://ddns.oray.com/checkip".to_string(),
        "https://4.ipw.cn".to_string(),
    ]
}

fn default_ddns_ipv6_urls() -> Vec<String> {
    vec![
        "https://api6.ipify.org".to_string(),
        "https://speed.neu6.edu.cn/getIP.php".to_string(),
        "https://v6.ident.me".to_string(),
        "https://myip6.ipip.net".to_string(),
        "https://6.ipw.cn".to_string(),
    ]
}

fn default_ddns_ipv6_config() -> DdnsIpConfig {
    DdnsIpConfig {
        enabled: false,
        get_type: default_ddns_get_type(),
        interface_name: String::new(),
        urls: default_ddns_ipv6_urls(),
        domains: Vec::new(),
    }
}

fn default_roaming_allowed() -> bool {
    true
}

fn default_data_enabled() -> bool {
    false
}

fn default_password_min_length() -> u8 {
    8
}

fn default_session_ttl_seconds() -> i64 {
    7 * 24 * 60 * 60
}

fn default_idle_timeout_seconds() -> i64 {
    60 * 60
}

fn default_apn_protocol() -> String {
    "dual".to_string()
}

fn default_apn_auth_method() -> String {
    "chap".to_string()
}

fn default_lpac_path() -> String {
    "/opt/simadmin/lpac/lpac".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApnConfig {
    #[serde(default)]
    pub apn: String,
    #[serde(default = "default_apn_protocol")]
    pub protocol: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default = "default_apn_auth_method")]
    pub auth_method: String,
}

impl Default for ApnConfig {
    fn default() -> Self {
        Self {
            apn: String::new(),
            protocol: default_apn_protocol(),
            username: String::new(),
            password: String::new(),
            auth_method: default_apn_auth_method(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EsimConfig {
    #[serde(default = "default_lpac_path")]
    pub lpac_path: String,
    #[serde(default)]
    pub custom_memory_total_kb: Option<u32>,
}

impl Default for EsimConfig {
    fn default() -> Self {
        Self {
            lpac_path: default_lpac_path(),
            custom_memory_total_kb: None,
        }
    }
}

/// 应用配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub webhook: WebhookConfig,
    #[serde(default)]
    pub notifications: NotificationConfig,
    #[serde(default)]
    pub device_network: DeviceNetworkConfig,
    #[serde(default)]
    pub version_update_notifications: VersionUpdateNotificationConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    /// 是否允许蜂窝数据漫游（写入 ModemManager Simple.Connect 的 allow-roaming）
    #[serde(default = "default_roaming_allowed")]
    pub roaming_allowed: bool,
    #[serde(default = "default_data_enabled")]
    pub data_enabled: bool,
    #[serde(default)]
    pub apn: ApnConfig,
    #[serde(default)]
    pub work_mode: WorkMode,
    #[serde(default)]
    pub esim: EsimConfig,
    #[serde(default)]
    pub automation: AutomationConfig,
    #[serde(default)]
    pub backup: BackupConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            webhook: WebhookConfig::default(),
            notifications: NotificationConfig::default(),
            device_network: DeviceNetworkConfig::default(),
            version_update_notifications: VersionUpdateNotificationConfig::default(),
            security: SecurityConfig::default(),
            roaming_allowed: default_roaming_allowed(),
            data_enabled: default_data_enabled(),
            apn: ApnConfig::default(),
            work_mode: WorkMode::default(),
            esim: EsimConfig::default(),
            automation: AutomationConfig::default(),
            backup: BackupConfig::default(),
        }
    }
}

fn migrate_legacy_webhook_config(config: &mut AppConfig) {
    if config.notifications.channels.is_empty()
        && config.notifications.rules.is_empty()
        && config.webhook != WebhookConfig::default()
    {
        let mut legacy = LegacyNotificationConfig::default();
        legacy.webhook = config.webhook.clone();
        config.notifications = NotificationConfig::from_legacy(legacy);
    }
    config.webhook = config
        .notifications
        .first_webhook_config()
        .unwrap_or_else(WebhookConfig::default);
}

fn migrate_update_template_string(template: &mut String) -> bool {
    let mut changed = false;
    let md5_patterns = [
        "OTA包 MD5: {{md5}}",
        "OTA包 MD5: {{MD5}}",
        "OTA包MD5: {{md5}}",
        "OTA包MD5: {{MD5}}",
        "MD5: {{md5}}",
        "MD5: {{MD5}}",
        "校验值: {{md5}}",
        "校验值: {{MD5}}",
        "二进制MD5: {{binary_md5}}",
        "前端MD5: {{frontend_md5}}",
        "{{md5}}",
        "{{MD5}}",
        "{{binary_md5}}",
        "{{frontend_md5}}",
    ];
    let commit_patterns = [
        "Commit: {{commit}}",
        "Commit: {{Commit}}",
        "commit: {{commit}}",
        "commit: {{Commit}}",
        "提交: {{commit}}",
        "提交: {{Commit}}",
        "发布目标: {{target_commitish}}",
        "{{commit}}",
        "{{Commit}}",
        "{{target_commitish}}",
    ];

    for pattern in md5_patterns.into_iter().chain(commit_patterns) {
        // Try replacing with leading newline (escaped JSON or real)
        let with_escaped_newline = format!("\\n{}", pattern);
        if template.contains(&with_escaped_newline) {
            *template = template.replace(&with_escaped_newline, "");
            changed = true;
        }
        let with_newline = format!("\n{}", pattern);
        if template.contains(&with_newline) {
            *template = template.replace(&with_newline, "");
            changed = true;
        }

        // Try replacing with trailing newline (escaped JSON or real)
        let with_escaped_trailing = format!("{}\\n", pattern);
        if template.contains(&with_escaped_trailing) {
            *template = template.replace(&with_escaped_trailing, "");
            changed = true;
        }
        let with_trailing = format!("{}\n", pattern);
        if template.contains(&with_trailing) {
            *template = template.replace(&with_trailing, "");
            changed = true;
        }

        // Fallback: replace pattern directly
        if template.contains(pattern) {
            *template = template.replace(pattern, "");
            changed = true;
        }
    }

    let time_replacements = [
        ("构建时间: {{构建时间}}", "时间: {{时间}}"),
        ("构建时间: {{build_time}}", "时间: {{time}}"),
        ("{{build_time}}", "{{time}}"),
        ("{{构建时间}}", "{{时间}}"),
    ];
    for (old, new) in time_replacements {
        if template.contains(old) {
            *template = template.replace(old, new);
            changed = true;
        }
    }

    changed
}

fn migrate_update_templates(config: &mut AppConfig) -> bool {
    let mut changed = false;

    // 1. Webhook template
    if migrate_update_template_string(&mut config.webhook.update_template) {
        changed = true;
    }

    // 2. Notification rules templates
    for rule in &mut config.notifications.rules {
        if rule.event_type == NotificationEventType::VersionUpdate {
            if migrate_update_template_string(&mut rule.template) {
                changed = true;
            }
        }
    }

    // 3. Notification channels templates
    for channel in &mut config.notifications.channels {
        if let Some(obj) = channel.config.as_object_mut() {
            // E.g. BarkConfig, PushPlusConfig, WecomAppConfig etc have nested "common"
            if let Some(common) = obj.get_mut("common").and_then(|v| v.as_object_mut()) {
                if let Some(serde_json::Value::String(tpl)) = common.get_mut("update_template") {
                    if migrate_update_template_string(tpl) {
                        changed = true;
                    }
                }
            }
            if let Some(serde_json::Value::String(tpl)) = obj.get_mut("update_template") {
                if migrate_update_template_string(tpl) {
                    changed = true;
                }
            }
        }
    }

    changed
}

/// 配置管理器
pub struct ConfigManager {
    config: Arc<RwLock<AppConfig>>,
    config_path: PathBuf,
}

impl ConfigManager {
    /// 创建新的配置管理器
    pub fn new(config_path: PathBuf) -> Self {
        let mut config = if config_path.exists() {
            match fs::read_to_string(&config_path) {
                Ok(content) => match serde_json::from_str::<AppConfig>(&content) {
                    Ok(cfg) => cfg,
                    Err(e) => {
                        warn!(error = %e, "Failed to parse config file, using defaults");
                        AppConfig::default()
                    }
                },
                Err(e) => {
                    warn!(error = %e, "Failed to read config file, using defaults");
                    AppConfig::default()
                }
            }
        } else {
            info!("No config file found, using defaults");
            AppConfig::default()
        };

        migrate_legacy_webhook_config(&mut config);
        let changed = migrate_update_templates(&mut config);

        let manager = Self {
            config: Arc::new(RwLock::new(config)),
            config_path,
        };

        // 保存配置（如果文件不存在，或者配置模板发生了自动清理）
        if !manager.config_path.exists() || changed {
            let _ = manager.save();
        }

        manager
    }

    /// 获取通知配置
    pub fn get_notifications(&self) -> NotificationConfig {
        self.config.read().unwrap().notifications.clone()
    }

    pub fn get_config(&self) -> AppConfig {
        self.config.read().unwrap().clone()
    }

    pub fn replace_config(&self, config: AppConfig) -> Result<(), String> {
        {
            let mut current = self.config.write().unwrap();
            *current = config;
        }
        self.save()
    }

    /// 获取自动化配置
    pub fn get_automation_config(&self) -> AutomationConfig {
        self.config.read().unwrap().automation.clone()
    }

    /// 更新自动化配置
    pub fn set_automation_config(&self, automation: AutomationConfig) -> Result<(), String> {
        {
            let mut config = self.config.write().unwrap();
            config.automation = automation;
        }
        self.save()
    }

    pub fn get_backup_config(&self) -> BackupConfig {
        self.config.read().unwrap().backup.clone()
    }

    pub fn set_backup_config(&self, backup: BackupConfig) -> Result<(), String> {
        {
            let mut config = self.config.write().unwrap();
            config.backup = backup;
        }
        self.save()
    }

    pub fn get_roaming_allowed(&self) -> bool {
        self.config.read().unwrap().roaming_allowed
    }

    pub fn get_data_enabled(&self) -> bool {
        self.config.read().unwrap().data_enabled
    }

    pub fn get_apn_config(&self) -> ApnConfig {
        self.config.read().unwrap().apn.clone()
    }

    pub fn get_work_mode(&self) -> WorkMode {
        self.config.read().unwrap().work_mode
    }

    pub fn get_esim_config(&self) -> EsimConfig {
        self.config.read().unwrap().esim.clone()
    }

    pub fn set_esim_config(&self, esim: EsimConfig) -> Result<(), String> {
        {
            let mut c = self.config.write().unwrap();
            c.esim = esim;
        }
        self.save()
    }

    pub fn get_device_network(&self) -> DeviceNetworkConfig {
        self.config.read().unwrap().device_network.clone()
    }

    pub fn get_ddns_config(&self) -> DdnsConfig {
        self.config.read().unwrap().device_network.ddns.clone()
    }

    pub fn get_version_update_notifications(&self) -> VersionUpdateNotificationConfig {
        self.config
            .read()
            .unwrap()
            .version_update_notifications
            .clone()
    }

    pub fn get_security(&self) -> SecurityConfig {
        self.config.read().unwrap().security.clone()
    }

    pub fn set_security(&self, security: SecurityConfig) -> Result<(), String> {
        {
            let mut c = self.config.write().unwrap();
            c.security = security;
        }
        self.save()
    }

    pub fn set_data_enabled(&self, enabled: bool) -> Result<(), String> {
        {
            let mut c = self.config.write().unwrap();
            c.data_enabled = enabled;
        }
        self.save()
    }

    pub fn set_apn_config(&self, apn: ApnConfig) -> Result<(), String> {
        {
            let mut c = self.config.write().unwrap();
            c.apn = apn;
        }
        self.save()
    }

    pub fn set_work_mode(&self, mode: WorkMode) -> Result<(), String> {
        {
            let mut c = self.config.write().unwrap();
            c.work_mode = mode;
        }
        self.save()
    }

    pub fn set_roaming_allowed(&self, allowed: bool) -> Result<(), String> {
        {
            let mut c = self.config.write().unwrap();
            c.roaming_allowed = allowed;
        }
        self.save()
    }

    pub fn set_ddns_config(&self, ddns: DdnsConfig) -> Result<(), String> {
        {
            let mut c = self.config.write().unwrap();
            c.device_network.ddns = ddns;
        }
        self.save()
    }

    pub fn set_last_notified_update_version(&self, version: String) -> Result<(), String> {
        {
            let mut c = self.config.write().unwrap();
            c.version_update_notifications.last_notified_version = Some(version);
        }
        self.save()
    }

    /// 更新通知配置
    pub fn set_notifications(&self, notifications: NotificationConfig) -> Result<(), String> {
        {
            let mut config = self.config.write().unwrap();
            config.webhook = notifications
                .first_webhook_config()
                .unwrap_or_else(WebhookConfig::default);
            config.notifications = notifications;
        }
        self.save()
    }

    /// 保存配置到文件
    pub fn save(&self) -> Result<(), String> {
        let config = self.config.read().unwrap();
        let content = serde_json::to_string_pretty(&*config)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;

        // 确保目录存在
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config directory: {}", e))?;
        }

        fs::write(&self.config_path, content)
            .map_err(|e| format!("Failed to write config file: {}", e))?;

        Ok(())
    }
}

/// 获取默认配置文件路径
pub fn get_default_config_path() -> PathBuf {
    // 尝试 /data/config.json（设备上的持久化目录）
    let device_path = PathBuf::from("/data/config.json");
    if device_path.parent().map(|p| p.exists()).unwrap_or(false) {
        return device_path;
    }

    // 回退到当前目录
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("config.json")
}
