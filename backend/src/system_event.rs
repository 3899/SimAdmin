use crate::notification::NotificationSender;
use chrono::Utc;
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::warn;

const LOGIN_FAILURE_WINDOW: Duration = Duration::from_secs(5 * 60);
const LOGIN_FAILURE_THRESHOLD: usize = 5;

pub mod category {
    pub const BASEBAND: &str = "baseband";
    pub const CELLULAR: &str = "cellular";
    pub const DEVICE_NETWORK: &str = "device_network";
    pub const SYSTEM_SERVICE: &str = "system_service";
    pub const SECURITY: &str = "security";
    pub const ESIM: &str = "esim";
    pub const RESOURCE: &str = "resource";
}

pub mod severity {
    pub const INFO: &str = "info";
    pub const WARNING: &str = "warning";
    pub const CRITICAL: &str = "critical";
}

pub mod status {
    pub const TRIGGERED: &str = "triggered";
    pub const RECOVERED: &str = "recovered";
    pub const SUCCEEDED: &str = "succeeded";
    pub const FAILED: &str = "failed";
    pub const CHANGED: &str = "changed";
}

pub mod codes {
    pub const BASEBAND_MODEM_MISSING_THRESHOLD: &str = "baseband.modem_missing_threshold";
    pub const BASEBAND_SCAN_MODEMS_TRIGGERED: &str = "baseband.scan_modems_triggered";
    pub const BASEBAND_MODEMMANAGER_RESTARTED: &str = "baseband.modemmanager_restarted";
    pub const BASEBAND_MODEMMANAGER_RESTART_FAILED: &str = "baseband.modemmanager_restart_failed";
    pub const BASEBAND_MODEM_RECOVERED: &str = "baseband.modem_recovered";

    pub const CELLULAR_SEARCHING_THRESHOLD: &str = "cellular.searching_threshold";
    pub const CELLULAR_AUTO_REGISTER_TRIGGERED: &str = "cellular.auto_register_triggered";
    pub const CELLULAR_RADIO_CYCLE_TRIGGERED: &str = "cellular.radio_cycle_triggered";
    pub const CELLULAR_ACTIVATION_FAILED: &str = "cellular.activation_failed";
    pub const CELLULAR_CONNECTION_RECOVERED: &str = "cellular.connection_recovered";
    pub const CELLULAR_ROAMING_ALLOWED_CHANGED: &str = "cellular.roaming_allowed_changed";
    pub const CELLULAR_AIRPLANE_MODE_CHANGED: &str = "cellular.airplane_mode_changed";
    pub const CELLULAR_DATA_ENABLED_CHANGED: &str = "cellular.data_enabled_changed";

    pub const DEVICE_NETWORK_WLAN_CONNECTED: &str = "device_network.wlan_connected";
    pub const DEVICE_NETWORK_WLAN_DISCONNECTED: &str = "device_network.wlan_disconnected";
    pub const DEVICE_NETWORK_WLAN_SSID_CHANGED: &str = "device_network.wlan_ssid_changed";
    pub const DEVICE_NETWORK_WLAN_CONNECT_FAILED: &str = "device_network.wlan_connect_failed";

    pub const SYSTEM_SERVICE_REBOOT_REQUESTED: &str = "system_service.system_reboot_requested";
    pub const SYSTEM_SERVICE_SIMADMIN_RESTART_REQUESTED: &str =
        "system_service.simadmin_restart_requested";
    pub const SYSTEM_SERVICE_STARTED: &str = "system_service.service_started";
    pub const SYSTEM_SERVICE_REBOOT_PREP_FAILED: &str = "system_service.reboot_prep_failed";

    pub const SECURITY_PASSWORD_CHANGED: &str = "security.password_changed";
    pub const SECURITY_PASSWORD_PROTECTION_DISABLED: &str = "security.password_protection_disabled";
    pub const SECURITY_POLICY_CHANGED: &str = "security.policy_changed";
    pub const SECURITY_LOGIN_FAILED_THRESHOLD: &str = "security.login_failed_threshold";

    pub const ESIM_WORK_MODE_CHANGED: &str = "esim.work_mode_changed";
    pub const ESIM_LPAC_REPAIR_SUCCEEDED: &str = "esim.lpac_repair_succeeded";
    pub const ESIM_LPAC_REPAIR_FAILED: &str = "esim.lpac_repair_failed";
    pub const ESIM_PROFILE_ENABLE_SUCCEEDED: &str = "esim.profile_enable_succeeded";
    pub const ESIM_PROFILE_ENABLE_FAILED: &str = "esim.profile_enable_failed";
    pub const ESIM_PROFILE_DELETED: &str = "esim.profile_deleted";
    pub const ESIM_PROFILE_SWITCH_BASEBAND_RECOVERY_FAILED: &str =
        "esim.profile_switch_baseband_recovery_failed";
    pub const ESIM_PROFILE_DOWNLOAD_SUCCEEDED: &str = "esim.profile_download_succeeded";
    pub const ESIM_PROFILE_DOWNLOAD_FAILED: &str = "esim.profile_download_failed";
    pub const ESIM_RSP_NOTIFICATION_DELIVERY_FAILED: &str = "esim.rsp_notification_delivery_failed";

    pub const RESOURCE_TEMPERATURE_HIGH: &str = "resource.temperature_high";
    pub const RESOURCE_TEMPERATURE_RECOVERED: &str = "resource.temperature_recovered";
    pub const RESOURCE_DISK_LOW: &str = "resource.disk_low";
    pub const RESOURCE_DISK_RECOVERED: &str = "resource.disk_recovered";
    pub const RESOURCE_MEMORY_HIGH: &str = "resource.memory_high";
    pub const RESOURCE_MEMORY_RECOVERED: &str = "resource.memory_recovered";
    pub const RESOURCE_CPU_HIGH: &str = "resource.cpu_high";
    pub const RESOURCE_CPU_RECOVERED: &str = "resource.cpu_recovered";
    pub const RESOURCE_INTERFACE_ERRORS_INCREASED: &str = "resource.interface_errors_increased";
    pub const RESOURCE_INTERFACE_ERRORS_RECOVERED: &str = "resource.interface_errors_recovered";
    pub const RESOURCE_IPV4_CONNECTIVITY_FAILED: &str = "resource.ipv4_connectivity_failed";
    pub const RESOURCE_IPV4_CONNECTIVITY_RECOVERED: &str = "resource.ipv4_connectivity_recovered";
    pub const RESOURCE_IPV6_CONNECTIVITY_FAILED: &str = "resource.ipv6_connectivity_failed";
    pub const RESOURCE_IPV6_CONNECTIVITY_RECOVERED: &str = "resource.ipv6_connectivity_recovered";
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub struct SystemEventDefinition {
    pub code: &'static str,
    pub category: &'static str,
    pub category_label: &'static str,
    pub label: &'static str,
    pub default_enabled: bool,
}

pub const SYSTEM_EVENT_DEFINITIONS: &[SystemEventDefinition] = &[
    def(
        codes::BASEBAND_MODEM_MISSING_THRESHOLD,
        category::BASEBAND,
        "基带",
        "Modem 丢失达到阈值（连续 5 次）",
        true,
    ),
    def(
        codes::BASEBAND_MODEMMANAGER_RESTARTED,
        category::BASEBAND,
        "基带",
        "watchdog 重启 ModemManager",
        true,
    ),
    def(
        codes::BASEBAND_MODEMMANAGER_RESTART_FAILED,
        category::BASEBAND,
        "基带",
        "ModemManager 重启失败",
        true,
    ),
    def(
        codes::BASEBAND_MODEM_RECOVERED,
        category::BASEBAND,
        "基带",
        "Modem 恢复成功",
        true,
    ),
    def(
        codes::BASEBAND_SCAN_MODEMS_TRIGGERED,
        category::BASEBAND,
        "基带",
        "触发 mmcli 扫描 Modem（连续 3 次未找到 Modem）",
        false,
    ),
    def(
        codes::CELLULAR_SEARCHING_THRESHOLD,
        category::CELLULAR,
        "蜂窝网络",
        "长时间 searching（连续 4 次）",
        true,
    ),
    def(
        codes::CELLULAR_AUTO_REGISTER_TRIGGERED,
        category::CELLULAR,
        "蜂窝网络",
        "自动驻网触发（searching 连续 4 次）",
        true,
    ),
    def(
        codes::CELLULAR_RADIO_CYCLE_TRIGGERED,
        category::CELLULAR,
        "蜂窝网络",
        "射频循环触发（searching 8 次/状态卡住 6 次）",
        true,
    ),
    def(
        codes::CELLULAR_ACTIVATION_FAILED,
        category::CELLULAR,
        "蜂窝网络",
        "拨号/连接激活失败",
        true,
    ),
    def(
        codes::CELLULAR_CONNECTION_RECOVERED,
        category::CELLULAR,
        "蜂窝网络",
        "蜂窝连接恢复",
        true,
    ),
    def(
        codes::CELLULAR_ROAMING_ALLOWED_CHANGED,
        category::CELLULAR,
        "蜂窝网络",
        "允许漫游开关变化",
        true,
    ),
    def(
        codes::CELLULAR_AIRPLANE_MODE_CHANGED,
        category::CELLULAR,
        "蜂窝网络",
        "飞行模式开关变化",
        true,
    ),
    def(
        codes::CELLULAR_DATA_ENABLED_CHANGED,
        category::CELLULAR,
        "蜂窝网络",
        "数据开关变化",
        true,
    ),
    def(
        codes::DEVICE_NETWORK_WLAN_CONNECTED,
        category::DEVICE_NETWORK,
        "设备网络",
        "WLAN 连接",
        false,
    ),
    def(
        codes::DEVICE_NETWORK_WLAN_DISCONNECTED,
        category::DEVICE_NETWORK,
        "设备网络",
        "WLAN 断开",
        false,
    ),
    def(
        codes::DEVICE_NETWORK_WLAN_SSID_CHANGED,
        category::DEVICE_NETWORK,
        "设备网络",
        "SSID 变化",
        false,
    ),
    def(
        codes::DEVICE_NETWORK_WLAN_CONNECT_FAILED,
        category::DEVICE_NETWORK,
        "设备网络",
        "WLAN 连接失败",
        true,
    ),
    def(
        codes::SYSTEM_SERVICE_REBOOT_REQUESTED,
        category::SYSTEM_SERVICE,
        "系统/服务",
        "用户触发系统重启",
        true,
    ),
    def(
        codes::SYSTEM_SERVICE_SIMADMIN_RESTART_REQUESTED,
        category::SYSTEM_SERVICE,
        "系统/服务",
        "SimAdmin 服务重启请求",
        true,
    ),
    def(
        codes::SYSTEM_SERVICE_STARTED,
        category::SYSTEM_SERVICE,
        "系统/服务",
        "服务启动完成",
        false,
    ),
    def(
        codes::SYSTEM_SERVICE_REBOOT_PREP_FAILED,
        category::SYSTEM_SERVICE,
        "系统/服务",
        "系统重启预处理失败",
        true,
    ),
    def(
        codes::SECURITY_PASSWORD_CHANGED,
        category::SECURITY,
        "安全审计",
        "修改密码",
        true,
    ),
    def(
        codes::SECURITY_PASSWORD_PROTECTION_DISABLED,
        category::SECURITY,
        "安全审计",
        "关闭密码保护",
        true,
    ),
    def(
        codes::SECURITY_POLICY_CHANGED,
        category::SECURITY,
        "安全审计",
        "安全策略变更",
        true,
    ),
    def(
        codes::SECURITY_LOGIN_FAILED_THRESHOLD,
        category::SECURITY,
        "安全审计",
        "连续登录失败达到阈值（5 分钟内 5 次）",
        true,
    ),
    def(
        codes::ESIM_WORK_MODE_CHANGED,
        category::ESIM,
        "SIM/eSIM",
        "工作模式切换",
        true,
    ),
    def(
        codes::ESIM_LPAC_REPAIR_SUCCEEDED,
        category::ESIM,
        "SIM/eSIM",
        "lpac 修复成功",
        false,
    ),
    def(
        codes::ESIM_LPAC_REPAIR_FAILED,
        category::ESIM,
        "SIM/eSIM",
        "lpac 修复失败",
        true,
    ),
    def(
        codes::ESIM_PROFILE_ENABLE_SUCCEEDED,
        category::ESIM,
        "SIM/eSIM",
        "Profile 启用成功",
        false,
    ),
    def(
        codes::ESIM_PROFILE_ENABLE_FAILED,
        category::ESIM,
        "SIM/eSIM",
        "Profile 启用失败",
        true,
    ),
    def(
        codes::ESIM_PROFILE_DELETED,
        category::ESIM,
        "SIM/eSIM",
        "Profile 删除",
        true,
    ),
    def(
        codes::ESIM_PROFILE_SWITCH_BASEBAND_RECOVERY_FAILED,
        category::ESIM,
        "SIM/eSIM",
        "Profile 切换后基带恢复失败",
        true,
    ),
    def(
        codes::ESIM_PROFILE_DOWNLOAD_SUCCEEDED,
        category::ESIM,
        "SIM/eSIM",
        "Profile 写入成功",
        true,
    ),
    def(
        codes::ESIM_PROFILE_DOWNLOAD_FAILED,
        category::ESIM,
        "SIM/eSIM",
        "Profile 写入失败",
        true,
    ),
    def(
        codes::ESIM_RSP_NOTIFICATION_DELIVERY_FAILED,
        category::ESIM,
        "SIM/eSIM",
        "运营商 Profile 通知提交失败",
        true,
    ),
    def(
        codes::RESOURCE_TEMPERATURE_HIGH,
        category::RESOURCE,
        "资源告警",
        "高温（≥75°C）",
        true,
    ),
    def(
        codes::RESOURCE_TEMPERATURE_RECOVERED,
        category::RESOURCE,
        "资源告警",
        "温度恢复（≤65°C 连续 2 次）",
        true,
    ),
    def(
        codes::RESOURCE_DISK_LOW,
        category::RESOURCE,
        "资源告警",
        "磁盘空间不足（≤10% 或 ≤500MB）",
        true,
    ),
    def(
        codes::RESOURCE_DISK_RECOVERED,
        category::RESOURCE,
        "资源告警",
        "磁盘空间恢复（≥15% 且 ≥2GB）",
        true,
    ),
    def(
        codes::RESOURCE_MEMORY_HIGH,
        category::RESOURCE,
        "资源告警",
        "内存持续高占用（≥90% 持续 5 分钟）",
        true,
    ),
    def(
        codes::RESOURCE_MEMORY_RECOVERED,
        category::RESOURCE,
        "资源告警",
        "内存恢复（≤80% 持续 2 分钟）",
        true,
    ),
    def(
        codes::RESOURCE_CPU_HIGH,
        category::RESOURCE,
        "资源告警",
        "CPU 持续高负载（≥90% 持续 5 分钟）",
        true,
    ),
    def(
        codes::RESOURCE_CPU_RECOVERED,
        category::RESOURCE,
        "资源告警",
        "CPU 负载恢复（≤75% 持续 2 分钟）",
        true,
    ),
    def(
        codes::RESOURCE_INTERFACE_ERRORS_INCREASED,
        category::RESOURCE,
        "资源告警",
        "网络接口错误包增长（连续 3 次）",
        false,
    ),
    def(
        codes::RESOURCE_INTERFACE_ERRORS_RECOVERED,
        category::RESOURCE,
        "资源告警",
        "网络接口错误包恢复（连续 2 次）",
        false,
    ),
    def(
        codes::RESOURCE_IPV4_CONNECTIVITY_FAILED,
        category::RESOURCE,
        "资源告警",
        "IPv4 连通性失败（连续 3 次）",
        true,
    ),
    def(
        codes::RESOURCE_IPV4_CONNECTIVITY_RECOVERED,
        category::RESOURCE,
        "资源告警",
        "IPv4 连通性恢复（连续 2 次）",
        true,
    ),
    def(
        codes::RESOURCE_IPV6_CONNECTIVITY_FAILED,
        category::RESOURCE,
        "资源告警",
        "IPv6 连通性失败（连续 3 次）",
        false,
    ),
    def(
        codes::RESOURCE_IPV6_CONNECTIVITY_RECOVERED,
        category::RESOURCE,
        "资源告警",
        "IPv6 连通性恢复（连续 2 次）",
        false,
    ),
];

const fn def(
    code: &'static str,
    category: &'static str,
    category_label: &'static str,
    label: &'static str,
    default_enabled: bool,
) -> SystemEventDefinition {
    SystemEventDefinition {
        code,
        category,
        category_label,
        label,
        default_enabled,
    }
}

#[derive(Debug, Clone, serde::Deserialize, Serialize)]
pub struct SystemEvent {
    pub category: String,
    pub category_label: String,
    pub event_code: String,
    pub event_label: String,
    pub severity: String,
    pub severity_label: String,
    pub status: String,
    pub status_label: String,
    pub entity: String,
    pub message: String,
    pub timestamp: String,
}

impl SystemEvent {
    pub fn new(
        event_code: impl AsRef<str>,
        severity: impl AsRef<str>,
        status: impl AsRef<str>,
        entity: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        let code = event_code.as_ref();
        let severity = severity.as_ref();
        let status = status.as_ref();
        let definition = system_event_definition(code);
        Self {
            category: definition
                .map(|item| item.category)
                .unwrap_or("system")
                .to_string(),
            category_label: definition
                .map(|item| item.category_label)
                .unwrap_or("系统事件")
                .to_string(),
            event_code: code.to_string(),
            event_label: definition
                .map(|item| item.label)
                .unwrap_or(code)
                .to_string(),
            severity: severity.to_string(),
            severity_label: severity_label(severity).to_string(),
            status: status.to_string(),
            status_label: status_label(status).to_string(),
            entity: entity.into(),
            message: message.into(),
            timestamp: Utc::now().to_rfc3339(),
        }
    }
}

#[derive(Default)]
struct LoginFailureState {
    timestamps: VecDeque<Instant>,
    last_notification_at: Option<Instant>,
}

pub struct SystemEventEmitter {
    notification_sender: Arc<NotificationSender>,
    login_failures: Mutex<LoginFailureState>,
}

impl SystemEventEmitter {
    pub fn new(notification_sender: Arc<NotificationSender>) -> Self {
        Self {
            notification_sender,
            login_failures: Mutex::new(LoginFailureState::default()),
        }
    }

    pub async fn emit_code(
        &self,
        event_code: impl AsRef<str>,
        severity: impl AsRef<str>,
        status: impl AsRef<str>,
        entity: impl Into<String>,
        message: impl Into<String>,
    ) {
        if !self.is_enabled(event_code.as_ref()) {
            return;
        }
        self.emit(SystemEvent::new(
            event_code.as_ref(),
            severity.as_ref(),
            status.as_ref(),
            entity,
            message,
        ))
        .await;
    }

    pub async fn emit(&self, event: SystemEvent) {
        if !self.is_enabled(&event.event_code) {
            return;
        }
        if let Err(err) = self.notification_sender.forward_system_event(&event).await {
            warn!(
                event_code = %event.event_code,
                error = %err,
                "System event notification failed"
            );
        }
    }

    pub fn is_enabled(&self, event_code: &str) -> bool {
        self.notification_sender.system_event_enabled(event_code)
    }

    pub async fn record_login_failure(&self) {
        if !self.is_enabled(codes::SECURITY_LOGIN_FAILED_THRESHOLD) {
            return;
        }
        let now = Instant::now();
        let should_emit = {
            let mut state = self.login_failures.lock().await;
            while state
                .timestamps
                .front()
                .map(|time| now.duration_since(*time) > LOGIN_FAILURE_WINDOW)
                .unwrap_or(false)
            {
                state.timestamps.pop_front();
            }
            state.timestamps.push_back(now);

            let threshold_reached = state.timestamps.len() >= LOGIN_FAILURE_THRESHOLD;
            let cooled_down = state
                .last_notification_at
                .map(|time| now.duration_since(time) >= LOGIN_FAILURE_WINDOW)
                .unwrap_or(true);
            if threshold_reached && cooled_down {
                state.last_notification_at = Some(now);
                true
            } else {
                false
            }
        };

        if should_emit {
            self.emit_code(
                codes::SECURITY_LOGIN_FAILED_THRESHOLD,
                severity::WARNING,
                status::TRIGGERED,
                "web",
                "5 分钟内连续登录失败达到 5 次",
            )
            .await;
        }
    }
}

pub fn system_event_definition(code: &str) -> Option<&'static SystemEventDefinition> {
    SYSTEM_EVENT_DEFINITIONS
        .iter()
        .find(|definition| definition.code == code)
}

#[allow(dead_code)]
pub fn default_enabled_event_codes() -> Vec<String> {
    SYSTEM_EVENT_DEFINITIONS
        .iter()
        .filter(|definition| definition.default_enabled)
        .map(|definition| definition.code.to_string())
        .collect()
}

pub fn severity_label(value: &str) -> &'static str {
    match value {
        severity::INFO => "信息",
        severity::WARNING => "警告",
        severity::CRITICAL => "严重",
        _ => "未知",
    }
}

pub fn status_label(value: &str) -> &'static str {
    match value {
        status::TRIGGERED => "触发",
        status::RECOVERED => "恢复",
        status::SUCCEEDED => "成功",
        status::FAILED => "失败",
        status::CHANGED => "变化",
        _ => "未知",
    }
}

pub fn mask_identifier(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= 8 {
        return trimmed.to_string();
    }
    let suffix = trimmed
        .chars()
        .rev()
        .take(6)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    format!("***{suffix}")
}
