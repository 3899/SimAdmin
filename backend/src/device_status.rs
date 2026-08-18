use crate::config::{ConfigManager, NotificationRule};
use crate::db::{Database, PeriodSmsStats};
use crate::device_network::DdnsManager;
use crate::handlers::{async_ping_host, read_temperature_sensors};
use crate::models::{NetworkInterfaceInfo, OtaLatestReleaseResponse, ThermalZone};
use crate::modem_manager::{
    get_airplane_mode, get_cells_data, get_data_connection_status, get_device_info_data,
    get_is_roaming_mm, get_network_info_data, get_signal_strength, get_sim_info_data_with_cache,
};
use crate::notification::{quiet_hours_active, NotificationSender};
use crate::utils::{
    connection_addresses_from_interfaces, format_uptime, read_cpu_load_sync, read_disk_info,
    read_memory_info, read_network_interfaces, read_system_info, read_uptime, sample_cpu_usage,
};
use chrono::{
    Datelike, Duration as ChronoDuration, FixedOffset, NaiveTime, TimeZone, Timelike, Utc,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::MissedTickBehavior;
use tracing::warn;
use zbus::Connection;

const BEIJING_UTC_OFFSET_SECONDS: i32 = 8 * 60 * 60;
const DEVICE_STATUS_TICK: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
pub struct DeviceStatusReport {
    pub lines: Vec<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Default)]
pub struct DeviceStatusSection {
    pub category: String,
    pub lines: Vec<String>,
}

impl DeviceStatusReport {
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    pub fn sections(&self) -> Vec<DeviceStatusSection> {
        let mut sections: Vec<DeviceStatusSection> = Vec::new();
        for line in &self.lines {
            let category = status_line_category(line);
            if let Some(section) = sections
                .iter_mut()
                .find(|section| section.category == category)
            {
                section.lines.push(line.clone());
            } else {
                sections.push(DeviceStatusSection {
                    category: category.to_string(),
                    lines: vec![line.clone()],
                });
            }
        }
        sections
    }
}

fn status_line_category(line: &str) -> &'static str {
    if line.starts_with("设备：")
        || line.starts_with("型号：")
        || line.starts_with("系统：")
        || line.starts_with("运行时长：")
    {
        "设备概览"
    } else if line.starts_with("工作模式：")
        || line.starts_with("SIM：")
        || line.starts_with("MCC/MNC：")
        || line.starts_with("号码：")
        || line.starts_with("ICCID：")
    {
        "SIM/eSIM"
    } else if line.starts_with("注册状态：")
        || line.starts_with("运营商：")
        || line.starts_with("网络：")
        || line.starts_with("信号：")
        || line.starts_with("数据连接：")
        || line.starts_with("飞行模式：")
        || line.starts_with("漫游：")
        || line.starts_with("小区：")
    {
        "蜂窝网络"
    } else if line.starts_with("IPv4：")
        || line.starts_with("IPv6：")
        || line.starts_with("出口：")
        || line.starts_with("IP：")
    {
        "IP 与连通性"
    } else if line.starts_with("WLAN")
        || line.starts_with("SSID：")
        || line.starts_with("接口：")
        || line.starts_with("蜂窝流量：")
        || line.starts_with("Wi-Fi 流量：")
    {
        "WLAN/LAN"
    } else if line.starts_with("CPU：")
        || line.starts_with("内存：")
        || line.starts_with("磁盘")
        || line.starts_with("双高温度：")
    {
        "系统资源"
    } else if line.starts_with("SimAdmin：")
        || line.starts_with("DDNS：")
        || line.starts_with("OTA：")
    {
        "服务状态"
    } else if line.starts_with("通道：")
        || line.starts_with("规则：")
        || (line.starts_with("短信：") && !line.contains("总计"))
    {
        "转发状态"
    } else if line.starts_with("短信：") {
        "通讯统计"
    } else if line.starts_with("密码保护：") || line.starts_with("会话有效期：") {
        "安全摘要"
    } else {
        "其他"
    }
}

#[derive(Default)]
struct ScheduleState {
    last_interval_sent: HashMap<String, Instant>,
    fixed_sent_keys: HashSet<String>,
}

pub fn spawn_device_status_scheduler(
    config_manager: Arc<ConfigManager>,
    notification_sender: Arc<NotificationSender>,
    database: Arc<Database>,
    dbus_conn: Arc<Connection>,
    ddns_manager: Arc<DdnsManager>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut state = ScheduleState::default();
        let mut tick = tokio::time::interval(DEVICE_STATUS_TICK);
        tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
        tick.tick().await;
        loop {
            tick.tick().await;
            let config = config_manager.get_notifications();
            for rule in config.rules.iter().filter(|rule| {
                rule.enabled
                    && rule.event_type == crate::config::NotificationEventType::DeviceStatus
            }) {
                if !device_status_due(rule, &mut state) {
                    continue;
                }
                if quiet_hours_active(&rule.quiet_hours) {
                    continue;
                }
                let report = collect_device_status_report(
                    rule,
                    Arc::clone(&config_manager),
                    Arc::clone(&database),
                    Arc::clone(&dbus_conn),
                    Arc::clone(&ddns_manager),
                )
                .await;
                if let Err(err) = notification_sender
                    .forward_device_status_report(&rule.id, &report)
                    .await
                {
                    warn!(rule_id = %rule.id, error = %err, "Device status notification failed");
                }
            }
        }
    })
}

fn beijing_offset() -> FixedOffset {
    FixedOffset::east_opt(BEIJING_UTC_OFFSET_SECONDS).expect("valid Beijing UTC offset")
}

fn now_string() -> String {
    Utc::now()
        .with_timezone(&beijing_offset())
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

fn device_status_due(rule: &NotificationRule, state: &mut ScheduleState) -> bool {
    let schedule = &rule.device_status_schedule;
    if schedule.mode == "interval" {
        let interval = Duration::from_secs(u64::from(schedule.interval_minutes.max(30)) * 60);
        let now = Instant::now();
        match state.last_interval_sent.get(&rule.id) {
            Some(last) if now.duration_since(*last) < interval => false,
            _ => {
                state.last_interval_sent.insert(rule.id.clone(), now);
                true
            }
        }
    } else {
        let now = Utc::now().with_timezone(&beijing_offset());
        let weekday = now.weekday().number_from_monday() as u8;
        let weekdays = if schedule.weekdays.is_empty() {
            vec![1, 2, 3, 4, 5, 6, 7]
        } else {
            schedule.weekdays.clone()
        };
        if !weekdays.contains(&weekday) {
            return false;
        }
        let current_minutes = now.hour() * 60 + now.minute();
        schedule.times.iter().any(|time| {
            let Some(minutes) = parse_hhmm(time) else {
                return false;
            };
            if minutes != current_minutes {
                return false;
            }
            let key = format!("{}:{}", rule.id, now.format("%Y-%m-%d %H:%M"));
            state.fixed_sent_keys.insert(key)
        })
    }
}

fn parse_hhmm(value: &str) -> Option<u32> {
    let time = NaiveTime::parse_from_str(value.trim(), "%H:%M").ok()?;
    Some(time.hour() * 60 + time.minute())
}

pub async fn collect_device_status_report(
    rule: &NotificationRule,
    config_manager: Arc<ConfigManager>,
    database: Arc<Database>,
    dbus_conn: Arc<Connection>,
    ddns_manager: Arc<DdnsManager>,
) -> DeviceStatusReport {
    let items = rule
        .device_status_items
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let mut lines = Vec::new();

    if any(&items, &["device_power", "device_model"]) {
        if let Ok(device) = get_device_info_data(&dbus_conn).await {
            if items.contains("device_power") {
                lines.push(format!(
                    "设备：{}，{}",
                    if device.online { "在线" } else { "离线" },
                    if device.powered {
                        "上电"
                    } else {
                        "未上电"
                    }
                ));
            }
            if items.contains("device_model") {
                lines.push(format!(
                    "型号：{}，厂商：{}",
                    device.model, device.manufacturer
                ));
            }
        }
    }

    if items.contains("system_version") {
        if let Ok(info) = read_system_info() {
            lines.push(format!("系统：{}，架构：{}", info.release, info.machine));
        }
    }
    if items.contains("uptime") {
        if let Ok((uptime, _)) = read_uptime() {
            lines.push(format!("运行时长：{}", format_uptime(uptime)));
        }
    }
    if items.contains("work_mode") {
        lines.push(format!("工作模式：{}", config_manager.get_work_mode()));
    }

    if any(
        &items,
        &[
            "sim_present",
            "sim_operator",
            "phone_number",
            "sim_identifiers",
        ],
    ) {
        if let Ok(sim) = get_sim_info_data_with_cache(&dbus_conn, Some(&database)).await {
            if items.contains("sim_present") {
                lines.push(format!(
                    "SIM：{}",
                    if sim.present {
                        "已识别"
                    } else {
                        "未识别"
                    }
                ));
            }
            if items.contains("sim_operator") {
                lines.push(format!("MCC/MNC：{}/{}", dash(&sim.mcc), dash(&sim.mnc)));
            }
            if items.contains("phone_number") {
                let number = sim
                    .phone_numbers
                    .iter()
                    .find(|item| !item.trim().is_empty())
                    .map(|item| mask_suffix(item, 4))
                    .unwrap_or_else(|| "-".to_string());
                lines.push(format!("号码：{number}"));
            }
            if items.contains("sim_identifiers") {
                lines.push(format!(
                    "ICCID：{}，IMSI：{}",
                    mask_suffix(&sim.iccid, 6),
                    mask_suffix(&sim.imsi, 6)
                ));
            }
        }
    }

    if any(
        &items,
        &[
            "cellular_registration",
            "cellular_operator",
            "cellular_technology",
            "signal_strength",
        ],
    ) {
        if let Ok(network) = get_network_info_data(&dbus_conn).await {
            if items.contains("cellular_registration") {
                lines.push(format!(
                    "注册状态：{}",
                    registration_label(&network.registration_status)
                ));
            }
            if items.contains("cellular_operator") {
                lines.push(format!("运营商：{}", dash(&network.operator_name)));
            }
            if items.contains("cellular_technology") {
                lines.push(format!("网络：{}", dash(&network.technology_preference)));
            }
            if items.contains("signal_strength") {
                lines.push(format!("信号：{}%", network.signal_strength));
            }
        } else if items.contains("signal_strength") {
            if let Ok(signal) = get_signal_strength(&dbus_conn).await {
                lines.push(format!("信号：{}%", signal.strength));
            }
        }
    }
    if items.contains("data_connection") {
        if let Ok(active) = get_data_connection_status(&dbus_conn).await {
            lines.push(format!(
                "数据连接：{}",
                if active { "已连接" } else { "未连接" }
            ));
        }
    }
    if items.contains("airplane_mode") {
        if let Ok(status) = get_airplane_mode(&dbus_conn).await {
            lines.push(format!(
                "飞行模式：{}",
                if status.enabled { "开启" } else { "关闭" }
            ));
        }
    }
    if items.contains("roaming") {
        let allowed = config_manager.get_roaming_allowed();
        if let Ok(is_roaming) = get_is_roaming_mm(&dbus_conn).await {
            lines.push(format!(
                "漫游：{}，允许漫游：{}",
                yes_no(is_roaming),
                yes_no(allowed)
            ));
        }
    }
    if items.contains("cell_summary") {
        if let Ok(cells) = get_cells_data(&dbus_conn).await {
            let serving = cells.cells.iter().find(|cell| cell.is_serving);
            if let Some(cell) = serving {
                lines.push(format!(
                    "小区：{}，PCI {}，频点 {}",
                    dash(&cell.tech),
                    dash(&cell.pci),
                    dash(&cell.arfcn)
                ));
            }
        }
    }

    if items.contains("ipv4_connectivity") {
        let result = async_ping_host("223.5.5.5", false).await;
        lines.push(format_ping("IPv4", &result));
    }
    if items.contains("ipv6_connectivity") {
        let result = async_ping_host("2400:3200::1", true).await;
        lines.push(format_ping("IPv6", &result));
    }

    let need_interfaces = any(
        &items,
        &[
            "default_route",
            "default_ip",
            "key_interfaces",
            "cellular_traffic",
            "wifi_traffic",
        ],
    );
    let interfaces = if need_interfaces {
        read_network_interfaces(Some(&dbus_conn)).await.ok()
    } else {
        None
    };
    if let Some(interfaces) = interfaces.as_ref() {
        if items.contains("default_route") {
            lines.push(format_default_route(interfaces));
        }
        if items.contains("default_ip") {
            lines.push(format_default_ip(interfaces));
        }
        if items.contains("key_interfaces") {
            lines.push(format_key_interfaces(interfaces));
        }
        if items.contains("cellular_traffic") {
            lines.push(format_traffic(
                "蜂窝流量",
                interfaces.iter().filter(|iface| iface.is_cellular),
            ));
        }
        if items.contains("wifi_traffic") {
            lines.push(format_traffic(
                "Wi-Fi 流量",
                interfaces.iter().filter(|iface| iface.is_wireless),
            ));
        }
    }

    if any(
        &items,
        &["wlan_enabled", "wlan_connected", "wlan_ssid", "wlan_ip"],
    ) {
        if let Ok(wlan) = crate::device_network::wlan_status().await {
            if items.contains("wlan_enabled") {
                lines.push(format!(
                    "WLAN：{}，{}",
                    if wlan.available {
                        "可用"
                    } else {
                        "不可用"
                    },
                    if wlan.enabled {
                        "已启用"
                    } else {
                        "未启用"
                    }
                ));
            }
            if items.contains("wlan_connected") {
                lines.push(format!(
                    "WLAN 连接：{}",
                    if wlan.connected {
                        "已连接"
                    } else {
                        "未连接"
                    }
                ));
            }
            if items.contains("wlan_ssid") {
                lines.push(format!(
                    "SSID：{}",
                    wlan.ssid.unwrap_or_else(|| "-".to_string())
                ));
            }
            if items.contains("wlan_ip") {
                lines.push(format!(
                    "WLAN IP：{}，网关：{}",
                    wlan.ipv4_addresses
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "-".to_string()),
                    wlan.ipv4_gateway.unwrap_or_else(|| "-".to_string())
                ));
            }
        }
    }

    if items.contains("cpu_usage") {
        let usage = sample_cpu_usage()
            .await
            .ok()
            .or_else(|| read_cpu_load_sync().ok().map(|load| load.load_percent));
        if let Some(usage) = usage {
            lines.push(format!("CPU：{:.0}%", usage));
        }
    }
    if items.contains("memory_usage") {
        if let Ok((total, available, _, _)) = read_memory_info() {
            if total > 0 {
                let used_percent = (total.saturating_sub(available) as f64 / total as f64) * 100.0;
                lines.push(format!("内存：{:.0}%", used_percent));
            }
        }
    }
    if items.contains("root_disk") {
        if let Some(root) = read_disk_info()
            .into_iter()
            .find(|disk| disk.mount_point == "/")
        {
            lines.push(format!(
                "磁盘 `/`：可用 {}，已用 {:.0}%",
                format_bytes(root.available_bytes),
                root.used_percent
            ));
        }
    }
    if items.contains("top_temperatures") {
        let temps = top_temperatures(read_temperature_sensors(), 2);
        if !temps.is_empty() {
            lines.push(format!("双高温度：{}", temps.join("；")));
        }
    }

    if items.contains("service_version") {
        lines.push(format!(
            "SimAdmin：运行中，版本 {}",
            env!("CARGO_PKG_VERSION")
        ));
    }
    if items.contains("ddns_status") {
        let status = ddns_manager.status(&config_manager.get_ddns_config()).await;
        lines.push(format!(
            "DDNS：{}，上次同步 {}",
            if status.enabled {
                if status.running {
                    "运行中"
                } else {
                    "已启用"
                }
            } else {
                "未启用"
            },
            status.last_sync_at.unwrap_or_else(|| "-".to_string())
        ));
    }
    if items.contains("ota_status") {
        lines.push(collect_ota_status(Arc::clone(&config_manager)).await);
    }

    if any(
        &items,
        &[
            "forwarding_channels",
            "forwarding_rules",
            "sms_forwarding_stats",
        ],
    ) {
        let config = config_manager.get_notifications();
        if items.contains("forwarding_channels") {
            let enabled = config
                .channels
                .iter()
                .filter(|channel| channel.enabled)
                .count();
            lines.push(format!(
                "通道：已启用 {} / 共 {}",
                enabled,
                config.channels.len()
            ));
        }
        if items.contains("forwarding_rules") {
            let enabled = config.rules.iter().filter(|rule| rule.enabled).count();
            lines.push(format!(
                "规则：已启用 {} / 共 {}",
                enabled,
                config.rules.len()
            ));
        }
    }
    if items.contains("sms_forwarding_stats") {
        if let Ok(stats) =
            database.period_sms_stats(period_since(&rule.device_status_sms_period).as_deref())
        {
            lines.push(format_sms_forwarding_stats(
                &rule.device_status_sms_period,
                &stats,
            ));
        }
    }
    if items.contains("sms_stats") {
        if let Ok(stats) = database.get_sms_stats() {
            lines.push(format!(
                "短信：总计 {}，接收 {}，发送 {}，转发成功 {}",
                stats.total, stats.incoming, stats.outgoing, stats.pushed
            ));
        }
    }
    if any(&items, &["security_password", "security_session"]) {
        let security = config_manager.get_security();
        if items.contains("security_password") {
            lines.push(format!(
                "密码保护：{}",
                if security.password_protection_enabled {
                    "开启"
                } else {
                    "关闭"
                }
            ));
        }
        if items.contains("security_session") {
            lines.push(format!(
                "会话有效期：{}，空闲超时：{}",
                format_duration_seconds(security.session_ttl_seconds),
                format_duration_seconds(security.idle_timeout_seconds)
            ));
        }
    }

    DeviceStatusReport {
        lines,
        timestamp: now_string(),
    }
}

fn any(items: &HashSet<&str>, keys: &[&str]) -> bool {
    keys.iter().any(|key| items.contains(key))
}

fn dash(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        "-".to_string()
    } else {
        value.to_string()
    }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "是"
    } else {
        "否"
    }
}

fn mask_suffix(value: &str, keep: usize) -> String {
    let value = value.trim();
    if value.is_empty() {
        return "-".to_string();
    }
    let count = value.chars().count();
    if count <= keep {
        return value.to_string();
    }
    let suffix = value
        .chars()
        .rev()
        .take(keep)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    format!("***{suffix}")
}

fn registration_label(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "registered" | "home" => "已注册".to_string(),
        "roaming" => "已注册（漫游）".to_string(),
        "searching" => "搜索中".to_string(),
        "denied" => "被拒绝".to_string(),
        "idle" => "空闲".to_string(),
        "" => "-".to_string(),
        other => other.to_string(),
    }
}

fn format_ping(label: &str, result: &crate::models::PingResult) -> String {
    if result.success {
        match result.latency_ms {
            Some(latency) => format!("{label}：正常，{latency:.0}ms"),
            None => format!("{label}：正常"),
        }
    } else {
        format!("{label}：失败")
    }
}

fn format_default_route(interfaces: &[NetworkInterfaceInfo]) -> String {
    let ipv4 = interfaces
        .iter()
        .find(|iface| iface.is_default_ipv4)
        .map(|iface| iface.name.as_str())
        .unwrap_or("-");
    let ipv6 = interfaces
        .iter()
        .find(|iface| iface.is_default_ipv6)
        .map(|iface| iface.name.as_str())
        .unwrap_or("-");
    format!("出口：IPv4 via {ipv4}，IPv6 via {ipv6}")
}

fn format_default_ip(interfaces: &[NetworkInterfaceInfo]) -> String {
    let addresses = connection_addresses_from_interfaces(interfaces);
    format!(
        "IP：IPv4 {}，IPv6 {}",
        addresses
            .ipv4
            .first()
            .cloned()
            .unwrap_or_else(|| "-".to_string()),
        addresses
            .ipv6
            .first()
            .cloned()
            .unwrap_or_else(|| "-".to_string())
    )
}

fn format_key_interfaces(interfaces: &[NetworkInterfaceInfo]) -> String {
    let parts = interfaces
        .iter()
        .filter(|iface| {
            iface.is_cellular || iface.is_wireless || iface.is_default_ipv4 || iface.is_default_ipv6
        })
        .map(|iface| format!("{} {}", iface.name, iface.status))
        .collect::<Vec<_>>();
    format!(
        "接口：{}",
        if parts.is_empty() {
            "-".to_string()
        } else {
            parts.join("，")
        }
    )
}

fn format_traffic<'a>(
    label: &str,
    interfaces: impl Iterator<Item = &'a NetworkInterfaceInfo>,
) -> String {
    let (rx, tx) = interfaces.fold((0u64, 0u64), |(rx, tx), iface| {
        (
            rx.saturating_add(iface.rx_bytes),
            tx.saturating_add(iface.tx_bytes),
        )
    });
    format!(
        "{label}：下载 {}，上传 {}",
        format_bytes(rx),
        format_bytes(tx)
    )
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else if value >= 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

fn top_temperatures(mut sensors: Vec<ThermalZone>, limit: usize) -> Vec<String> {
    sensors.retain(|sensor| sensor.temperature.is_finite());
    sensors.sort_by(|a, b| b.temperature.total_cmp(&a.temperature));
    sensors
        .into_iter()
        .take(limit)
        .enumerate()
        .map(|(index, sensor)| {
            format!(
                "高温 {}：{} {:.1}°C",
                index + 1,
                if sensor.label.trim().is_empty() {
                    sensor.sensor_type.as_str()
                } else {
                    sensor.label.as_str()
                },
                sensor.temperature
            )
        })
        .collect()
}

async fn collect_ota_status(config_manager: Arc<ConfigManager>) -> String {
    let current = crate::ota::CURRENT_VERSION.to_string();
    let update_config = config_manager.get_version_update_notifications();
    let proxy_prefix = crate::ota::normalize_proxy_prefix(Some(update_config.proxy_prefix));
    let latest = fetch_latest_release_tag(&proxy_prefix).await;
    match latest {
        Ok(release) => {
            let latest_version = release.tag_name.trim_start_matches('v').to_string();
            let updatable = crate::ota::compare_versions(&release.tag_name, &current);
            format!(
                "OTA：{}，当前 {}，最新 {}",
                if updatable {
                    "可更新"
                } else {
                    "已是最新"
                },
                current,
                latest_version
            )
        }
        Err(_) => format!("OTA：最新版本检查失败，当前 {current}"),
    }
}

async fn fetch_latest_release_tag(proxy_prefix: &str) -> Result<OtaLatestReleaseResponse, String> {
    let client = crate::ota::build_ota_http_client()?;
    crate::ota::fetch_latest_github_release(&client, proxy_prefix, !proxy_prefix.is_empty()).await
}

fn period_since(period: &str) -> Option<String> {
    let now = Utc::now().with_timezone(&beijing_offset());
    let start = match period {
        "today" => now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .and_then(|time| beijing_offset().from_local_datetime(&time).single())?,
        "last_7d" => now.checked_sub_signed(ChronoDuration::days(7))?,
        "all" => return None,
        _ => now.checked_sub_signed(ChronoDuration::hours(24))?,
    };
    Some(start.format("%Y-%m-%d %H:%M:%S").to_string())
}

fn period_label(period: &str) -> &'static str {
    match period {
        "today" => "今日",
        "last_7d" => "近 7 天",
        "all" => "累计",
        _ => "近 24 小时",
    }
}

fn format_sms_forwarding_stats(period: &str, stats: &PeriodSmsStats) -> String {
    let counts = &stats.forwarding;
    let mut parts = vec![
        format!("{}接收 {}", period_label(period), stats.incoming),
        format!("转发成功 {}", counts.success),
        format!("失败 {}", counts.failed),
    ];
    if counts.quiet_hours > 0 {
        parts.push(format!("免打扰 {}", counts.quiet_hours));
    }
    if counts.unmatched > 0 {
        parts.push(format!("未匹配规则 {}", counts.unmatched));
    }
    if counts.no_available_channel > 0 {
        parts.push(format!("无可用通道 {}", counts.no_available_channel));
    }
    format!("短信：{}", parts.join("，"))
}

fn format_duration_seconds(seconds: i64) -> String {
    if seconds < 0 {
        return "永不过期".to_string();
    }
    if seconds >= 86_400 && seconds % 86_400 == 0 {
        format!("{} 天", seconds / 86_400)
    } else if seconds >= 3_600 && seconds % 3_600 == 0 {
        format!("{} 小时", seconds / 3_600)
    } else if seconds >= 60 && seconds % 60 == 0 {
        format!("{} 分钟", seconds / 60)
    } else {
        format!("{} 秒", seconds)
    }
}
