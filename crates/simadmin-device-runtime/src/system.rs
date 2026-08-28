use std::{collections::HashMap, fs, path::Path, time::Instant};

#[cfg(unix)]
use std::{
    collections::HashSet,
    ffi::CString,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use simadmin_protocol::{DeviceApiRequestPayload, DeviceApiResponsePayload};
use tokio::sync::Mutex;

use crate::{RuntimeError, RuntimeResult};

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct ThermalZone {
    pub zone: String,
    #[serde(rename = "type")]
    pub sensor_type: String,
    pub label: String,
    pub temperature: f64,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct NetworkSpeed {
    pub interface: String,
    pub rx_bytes_per_sec: u64,
    pub tx_bytes_per_sec: u64,
    pub total_rx_bytes: u64,
    pub total_tx_bytes: u64,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct NetworkSpeedResponse {
    pub interfaces: Vec<NetworkSpeed>,
    pub interval_seconds: f64,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct MemoryInfo {
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
    pub used_percent: f64,
    pub cached_bytes: u64,
    pub buffers_bytes: u64,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct UptimeInfo {
    pub uptime_seconds: u64,
    pub idle_seconds: u64,
    pub uptime_formatted: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct SystemInfo {
    pub sysname: String,
    pub nodename: String,
    pub release: String,
    pub version: String,
    pub machine: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub domainname: String,
    pub full_info: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct DiskInfo {
    pub mount_point: String,
    pub fs_type: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub used_percent: f64,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct CpuLoadInfo {
    pub load_1min: f64,
    pub load_5min: f64,
    pub load_15min: f64,
    pub core_count: u32,
    pub load_percent: f64,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct SystemStatsResponse {
    pub network_speed: NetworkSpeedResponse,
    pub memory: MemoryInfo,
    pub disk: Vec<DiskInfo>,
    pub cpu_load: CpuLoadInfo,
    pub uptime: UptimeInfo,
    pub system_info: SystemInfo,
    pub temperature: Vec<ThermalZone>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct PingResult {
    pub success: bool,
    pub latency_ms: Option<f64>,
    pub target: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct ConnectivityCheckResponse {
    pub ipv4: PingResult,
    pub ipv6: PingResult,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct IpAddress {
    pub address: String,
    pub prefix_len: u8,
    pub ip_type: String,
    pub scope: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct NetworkInterfaceInfo {
    pub name: String,
    pub status: String,
    pub is_wireless: bool,
    pub is_cellular: bool,
    pub is_default_ipv4: bool,
    pub is_default_ipv6: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mac_address: Option<String>,
    pub mtu: u32,
    pub ip_addresses: Vec<IpAddress>,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub rx_errors: u64,
    pub tx_errors: u64,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct NetworkInterfacesResponse {
    pub interfaces: Vec<NetworkInterfaceInfo>,
    pub total_count: usize,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct ConnectionAddressesResponse {
    pub ipv4: Vec<String>,
    pub ipv6: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipv4_interface: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipv6_interface: Option<String>,
}

#[derive(Default)]
struct SystemSamplerState {
    previous_network: HashMap<String, (u64, u64)>,
    previous_cpu: Option<(u64, u64)>,
    last_sample: Option<Instant>,
}

#[derive(Default)]
pub struct SystemRuntime {
    state: Mutex<SystemSamplerState>,
}

impl SystemRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn handles(path: &str) -> bool {
        matches!(
            path.split('?').next().unwrap_or(path),
            "/stats" | "/network/interfaces" | "/network/connection-addresses" | "/connectivity"
        )
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
        let data = match route {
            "/stats" => serde_json::to_value(self.stats().await?)?,
            "/network/interfaces" => serde_json::to_value(network_interfaces())?,
            "/network/connection-addresses" => serde_json::to_value(connection_addresses())?,
            "/connectivity" => serde_json::to_value(connectivity().await)?,
            _ => return Err(RuntimeError::UnsupportedRoute(method, route.to_owned())),
        };
        Ok(DeviceApiResponsePayload {
            status: 200,
            body: api_envelope(data),
        })
    }

    pub async fn stats(&self) -> RuntimeResult<SystemStatsResponse> {
        let mut state = self.state.lock().await;
        let now = Instant::now();
        let elapsed = state
            .last_sample
            .map(|previous| now.duration_since(previous).as_secs_f64())
            .unwrap_or(1.0)
            .max(0.001);
        state.last_sample = Some(now);

        let active = active_interfaces();
        let mut speeds = Vec::new();
        for interface in &active {
            let rx = interface_stat(interface, "rx_bytes");
            let tx = interface_stat(interface, "tx_bytes");
            let (rx_speed, tx_speed) = state
                .previous_network
                .get(interface)
                .map(|(previous_rx, previous_tx)| {
                    (
                        (rx.saturating_sub(*previous_rx) as f64 / elapsed) as u64,
                        (tx.saturating_sub(*previous_tx) as f64 / elapsed) as u64,
                    )
                })
                .unwrap_or_default();
            state.previous_network.insert(interface.clone(), (rx, tx));
            speeds.push(NetworkSpeed {
                interface: interface.clone(),
                rx_bytes_per_sec: rx_speed,
                tx_bytes_per_sec: tx_speed,
                total_rx_bytes: rx,
                total_tx_bytes: tx,
            });
        }
        state
            .previous_network
            .retain(|name, _| active.iter().any(|current| current == name));

        let current_cpu = cpu_totals();
        let load_percent = match (state.previous_cpu, current_cpu) {
            (Some((previous_total, previous_idle)), Some((total, idle))) => {
                let total_delta = total.saturating_sub(previous_total);
                let idle_delta = idle.saturating_sub(previous_idle);
                if total_delta == 0 {
                    0.0
                } else {
                    ((total_delta.saturating_sub(idle_delta)) as f64 / total_delta as f64 * 100.0)
                        .clamp(0.0, 100.0)
                }
            }
            _ => 0.0,
        };
        state.previous_cpu = current_cpu;

        Ok(SystemStatsResponse {
            network_speed: NetworkSpeedResponse {
                interfaces: speeds,
                interval_seconds: elapsed,
            },
            memory: memory_info(),
            disk: disk_info(),
            cpu_load: cpu_load(load_percent),
            uptime: uptime_info(),
            system_info: system_info(),
            temperature: temperature_sensors(),
        })
    }
}

fn api_envelope(data: Value) -> Value {
    json!({ "status": "ok", "message": "Success", "data": data })
}

fn active_interfaces() -> Vec<String> {
    let mut interfaces = fs::read_dir("/sys/class/net")
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == "lo" {
                return None;
            }
            let status = read_trimmed(entry.path().join("operstate")).unwrap_or_default();
            matches!(status.as_str(), "up" | "unknown").then_some(name)
        })
        .collect::<Vec<_>>();
    interfaces.sort();
    interfaces
}

fn interface_stat(interface: &str, name: &str) -> u64 {
    read_trimmed(
        Path::new("/sys/class/net")
            .join(interface)
            .join("statistics")
            .join(name),
    )
    .and_then(|value| value.parse().ok())
    .unwrap_or_default()
}

fn memory_info() -> MemoryInfo {
    let values = fs::read_to_string("/proc/meminfo")
        .ok()
        .map(|content| {
            content
                .lines()
                .filter_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    let kib = value.split_whitespace().next()?.parse::<u64>().ok()?;
                    Some((name.to_owned(), kib.saturating_mul(1024)))
                })
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();
    let total = values.get("MemTotal").copied().unwrap_or_default();
    let available = values.get("MemAvailable").copied().unwrap_or_default();
    let used = total.saturating_sub(available);
    MemoryInfo {
        total_bytes: total,
        available_bytes: available,
        used_bytes: used,
        used_percent: if total == 0 {
            0.0
        } else {
            used as f64 / total as f64 * 100.0
        },
        cached_bytes: values.get("Cached").copied().unwrap_or_default(),
        buffers_bytes: values.get("Buffers").copied().unwrap_or_default(),
    }
}

fn uptime_info() -> UptimeInfo {
    let values = fs::read_to_string("/proc/uptime")
        .ok()
        .map(|content| {
            content
                .split_whitespace()
                .filter_map(|value| value.parse::<f64>().ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let uptime = values.first().copied().unwrap_or_default().max(0.0) as u64;
    let idle = values.get(1).copied().unwrap_or_default().max(0.0) as u64;
    UptimeInfo {
        uptime_seconds: uptime,
        idle_seconds: idle,
        uptime_formatted: format_uptime(uptime),
    }
}

fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = seconds % 86_400 / 3_600;
    let minutes = seconds % 3_600 / 60;
    if days > 0 {
        format!("{days}天 {hours}小时 {minutes}分钟")
    } else if hours > 0 {
        format!("{hours}小时 {minutes}分钟")
    } else {
        format!("{minutes}分钟")
    }
}

fn cpu_load(load_percent: f64) -> CpuLoadInfo {
    let values = fs::read_to_string("/proc/loadavg")
        .ok()
        .map(|content| {
            content
                .split_whitespace()
                .take(3)
                .filter_map(|value| value.parse::<f64>().ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    CpuLoadInfo {
        load_1min: values.first().copied().unwrap_or_default(),
        load_5min: values.get(1).copied().unwrap_or_default(),
        load_15min: values.get(2).copied().unwrap_or_default(),
        core_count: std::thread::available_parallelism()
            .map(|value| value.get() as u32)
            .unwrap_or(1),
        load_percent,
    }
}

fn cpu_totals() -> Option<(u64, u64)> {
    let content = fs::read_to_string("/proc/stat").ok()?;
    let fields = content
        .lines()
        .next()?
        .split_whitespace()
        .collect::<Vec<_>>();
    if fields.first().copied() != Some("cpu") {
        return None;
    }
    let values = fields
        .iter()
        .skip(1)
        .filter_map(|value| value.parse::<u64>().ok())
        .collect::<Vec<_>>();
    let total = values.iter().copied().sum();
    let idle =
        values.get(3).copied().unwrap_or_default() + values.get(4).copied().unwrap_or_default();
    Some((total, idle))
}

fn temperature_sensors() -> Vec<ThermalZone> {
    let mut sensors = fs::read_dir("/sys/class/thermal")
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("thermal_zone")
        })
        .filter_map(|entry| {
            let zone = entry.file_name().to_string_lossy().into_owned();
            let sensor_type = read_trimmed(entry.path().join("type")).unwrap_or_default();
            let raw = read_trimmed(entry.path().join("temp"))?
                .parse::<f64>()
                .ok()?;
            let temperature = if raw.abs() > 1_000.0 {
                raw / 1_000.0
            } else {
                raw
            };
            temperature.is_finite().then(|| ThermalZone {
                zone: zone.clone(),
                label: temperature_label(&sensor_type, &zone),
                sensor_type,
                temperature,
            })
        })
        .collect::<Vec<_>>();
    sensors.sort_by(|left, right| left.zone.cmp(&right.zone));
    sensors
}

fn temperature_label(sensor_type: &str, zone: &str) -> String {
    let value = if sensor_type.trim().is_empty() {
        zone
    } else {
        sensor_type
    };
    let normalized = value.to_ascii_lowercase();
    for (patterns, label) in [
        (&["modem", "baseband", "wwan", "qmi", "mhi"][..], "基带"),
        (&["gpu", "adreno"][..], "GPU"),
        (&["camera", "cam", "isp"][..], "摄像头"),
        (&["wifi", "wlan"][..], "Wi-Fi"),
        (&["battery", "batt"][..], "电池"),
        (&["pmic", "power"][..], "电源管理"),
        (&["soc", "tsens"][..], "SoC"),
        (&["skin", "shell", "case"][..], "外壳"),
    ] {
        if patterns.iter().any(|pattern| normalized.contains(pattern)) {
            return label.to_owned();
        }
    }
    if normalized.contains("cpu") {
        return "CPU".into();
    }
    if normalized.contains("core") {
        return "核心".into();
    }
    value.to_owned()
}

#[cfg(unix)]
fn disk_info() -> Vec<DiskInfo> {
    let skip = [
        "proc",
        "sysfs",
        "tmpfs",
        "devtmpfs",
        "devpts",
        "cgroup",
        "cgroup2",
        "overlay",
        "squashfs",
        "debugfs",
        "tracefs",
        "securityfs",
        "pstore",
        "configfs",
        "fusectl",
    ];
    let mut seen = HashSet::new();
    let mut disks = fs::read_to_string("/proc/mounts")
        .ok()
        .into_iter()
        .flat_map(|content| content.lines().map(str::to_owned).collect::<Vec<_>>())
        .filter_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            let device = fields.first()?.to_string();
            let mount_point = fields.get(1)?.replace("\\040", " ");
            let fs_type = fields.get(2)?.to_string();
            if skip.contains(&fs_type.as_str()) || !seen.insert(device) {
                return None;
            }
            let path = CString::new(mount_point.as_str()).ok()?;
            let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
            if unsafe { libc::statvfs(path.as_ptr(), &mut stat) } != 0 {
                return None;
            }
            let block_size = stat.f_frsize as u64;
            let total = stat.f_blocks as u64 * block_size;
            if total < 1_048_576 {
                return None;
            }
            let available = stat.f_bavail as u64 * block_size;
            let used = total.saturating_sub(stat.f_bfree as u64 * block_size);
            Some(DiskInfo {
                mount_point,
                fs_type,
                total_bytes: total,
                used_bytes: used,
                available_bytes: available,
                used_percent: used as f64 / total as f64 * 100.0,
            })
        })
        .collect::<Vec<_>>();
    disks.sort_by_key(|disk| (disk.mount_point != "/", disk.mount_point.clone()));
    disks
}

#[cfg(not(unix))]
fn disk_info() -> Vec<DiskInfo> {
    Vec::new()
}

#[cfg(unix)]
fn system_info() -> SystemInfo {
    use std::ffi::CStr;
    unsafe {
        let mut value: libc::utsname = std::mem::zeroed();
        if libc::uname(&mut value) != 0 {
            return SystemInfo::default();
        }
        let read =
            |field: *const libc::c_char| CStr::from_ptr(field).to_string_lossy().into_owned();
        let sysname = read(value.sysname.as_ptr());
        let nodename = read(value.nodename.as_ptr());
        let release = read(value.release.as_ptr());
        let version = read(value.version.as_ptr());
        let machine = read(value.machine.as_ptr());
        let full_info = format!("{sysname} {nodename} {release} {version} {machine}");
        SystemInfo {
            sysname,
            nodename,
            release,
            version,
            machine,
            domainname: String::new(),
            full_info,
        }
    }
}

#[cfg(not(unix))]
fn system_info() -> SystemInfo {
    SystemInfo {
        sysname: std::env::consts::OS.into(),
        nodename: std::env::var("COMPUTERNAME").unwrap_or_default(),
        machine: std::env::consts::ARCH.into(),
        ..SystemInfo::default()
    }
}

pub fn network_interfaces() -> NetworkInterfacesResponse {
    let addresses = interface_addresses();
    let default_ipv4 = default_ipv4_interface();
    let default_ipv6 = default_ipv6_interface();
    let mut interfaces = fs::read_dir("/sys/class/net")
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == "lo" {
                return None;
            }
            let path = entry.path();
            Some(NetworkInterfaceInfo {
                status: read_trimmed(path.join("operstate")).unwrap_or_default(),
                is_wireless: path.join("wireless").exists(),
                is_cellular: is_cellular_interface(&name),
                is_default_ipv4: default_ipv4.as_deref() == Some(name.as_str()),
                is_default_ipv6: default_ipv6.as_deref() == Some(name.as_str()),
                mac_address: read_trimmed(path.join("address")).filter(|value| !value.is_empty()),
                mtu: read_trimmed(path.join("mtu"))
                    .and_then(|value| value.parse().ok())
                    .unwrap_or_default(),
                ip_addresses: addresses.get(&name).cloned().unwrap_or_default(),
                rx_bytes: interface_stat(&name, "rx_bytes"),
                tx_bytes: interface_stat(&name, "tx_bytes"),
                rx_packets: interface_stat(&name, "rx_packets"),
                tx_packets: interface_stat(&name, "tx_packets"),
                rx_errors: interface_stat(&name, "rx_errors"),
                tx_errors: interface_stat(&name, "tx_errors"),
                name,
            })
        })
        .collect::<Vec<_>>();
    interfaces.sort_by(|left, right| left.name.cmp(&right.name));
    NetworkInterfacesResponse {
        total_count: interfaces.len(),
        interfaces,
    }
}

pub fn connection_addresses() -> ConnectionAddressesResponse {
    let interfaces = network_interfaces().interfaces;
    let ipv4_interface = default_ipv4_interface();
    let ipv6_interface = default_ipv6_interface();
    let addresses = |family: &str, preferred: Option<&str>| {
        let selected = interfaces
            .iter()
            .filter(|interface| preferred.is_none_or(|name| interface.name == name))
            .flat_map(|interface| interface.ip_addresses.iter())
            .filter(|address| {
                address.ip_type == family
                    && !matches!(address.scope.as_str(), "loopback" | "link-local")
            })
            .map(|address| address.address.clone())
            .collect::<Vec<_>>();
        if selected.is_empty() && preferred.is_some() {
            interfaces
                .iter()
                .flat_map(|interface| interface.ip_addresses.iter())
                .filter(|address| {
                    address.ip_type == family
                        && !matches!(address.scope.as_str(), "loopback" | "link-local")
                })
                .map(|address| address.address.clone())
                .collect()
        } else {
            selected
        }
    };
    ConnectionAddressesResponse {
        ipv4: addresses("ipv4", ipv4_interface.as_deref()),
        ipv6: addresses("ipv6", ipv6_interface.as_deref()),
        ipv4_interface,
        ipv6_interface,
    }
}

#[cfg(unix)]
fn interface_addresses() -> HashMap<String, Vec<IpAddress>> {
    use std::ffi::CStr;

    let mut result = HashMap::<String, Vec<IpAddress>>::new();
    let mut head: *mut libc::ifaddrs = std::ptr::null_mut();
    if unsafe { libc::getifaddrs(&mut head) } != 0 {
        return result;
    }
    let mut current = head;
    while !current.is_null() {
        let item = unsafe { &*current };
        if !item.ifa_addr.is_null() && !item.ifa_name.is_null() {
            let name = unsafe { CStr::from_ptr(item.ifa_name) }
                .to_string_lossy()
                .into_owned();
            let family = unsafe { (*item.ifa_addr).sa_family as i32 };
            let address = match family {
                libc::AF_INET => {
                    let socket = unsafe { &*(item.ifa_addr as *const libc::sockaddr_in) };
                    let ip = IpAddr::V4(Ipv4Addr::from(socket.sin_addr.s_addr.to_ne_bytes()));
                    let prefix_len = if item.ifa_netmask.is_null() {
                        0
                    } else {
                        let mask = unsafe { &*(item.ifa_netmask as *const libc::sockaddr_in) };
                        mask.sin_addr
                            .s_addr
                            .to_ne_bytes()
                            .iter()
                            .map(|byte| byte.count_ones() as u8)
                            .sum()
                    };
                    Some(IpAddress {
                        address: ip.to_string(),
                        prefix_len,
                        ip_type: "ipv4".into(),
                        scope: ip_scope(&ip),
                    })
                }
                libc::AF_INET6 => {
                    let socket = unsafe { &*(item.ifa_addr as *const libc::sockaddr_in6) };
                    let ip = IpAddr::V6(Ipv6Addr::from(socket.sin6_addr.s6_addr));
                    let prefix_len = if item.ifa_netmask.is_null() {
                        0
                    } else {
                        let mask = unsafe { &*(item.ifa_netmask as *const libc::sockaddr_in6) };
                        mask.sin6_addr
                            .s6_addr
                            .iter()
                            .map(|byte| byte.count_ones() as u8)
                            .sum()
                    };
                    Some(IpAddress {
                        address: ip.to_string(),
                        prefix_len,
                        ip_type: "ipv6".into(),
                        scope: ip_scope(&ip),
                    })
                }
                _ => None,
            };
            if let Some(address) = address {
                result.entry(name).or_default().push(address);
            }
        }
        current = item.ifa_next;
    }
    unsafe { libc::freeifaddrs(head) };
    for addresses in result.values_mut() {
        addresses.sort_by(|left, right| left.address.cmp(&right.address));
        addresses.dedup();
    }
    result
}

#[cfg(not(unix))]
fn interface_addresses() -> HashMap<String, Vec<IpAddress>> {
    HashMap::new()
}

#[cfg(unix)]
fn ip_scope(ip: &IpAddr) -> String {
    match ip {
        IpAddr::V4(value) if value.is_loopback() => "loopback",
        IpAddr::V4(value) if value.is_link_local() => "link-local",
        IpAddr::V4(value) if value.is_private() => "private",
        IpAddr::V6(value) if value.is_loopback() => "loopback",
        IpAddr::V6(value) if value.is_unicast_link_local() => "link-local",
        IpAddr::V6(value) if value.segments()[0] & 0xfe00 == 0xfc00 => "private",
        _ => "public",
    }
    .into()
}

fn default_ipv4_interface() -> Option<String> {
    parse_default_ipv4_interface(&fs::read_to_string("/proc/net/route").ok()?)
}

fn parse_default_ipv4_interface(content: &str) -> Option<String> {
    content
        .lines()
        .skip(1)
        .filter_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if fields.get(1).copied() != Some("00000000") {
                return None;
            }
            Some((
                fields.first()?.to_string(),
                fields.get(6)?.parse::<u32>().ok()?,
            ))
        })
        .min_by_key(|(_, metric)| *metric)
        .map(|(interface, _)| interface)
}

fn default_ipv6_interface() -> Option<String> {
    parse_default_ipv6_interface(&fs::read_to_string("/proc/net/ipv6_route").ok()?)
}

fn parse_default_ipv6_interface(content: &str) -> Option<String> {
    content
        .lines()
        .filter_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if fields.first().copied() != Some("00000000000000000000000000000000")
                || fields.get(1).copied() != Some("00")
                || fields.get(9).copied() == Some("lo")
            {
                return None;
            }
            Some((
                fields.get(9)?.to_string(),
                u32::from_str_radix(fields.get(5)?, 16).ok()?,
            ))
        })
        .min_by_key(|(_, metric)| *metric)
        .map(|(interface, _)| interface)
}

fn is_cellular_interface(name: &str) -> bool {
    let value = name.to_ascii_lowercase();
    ["wwan", "wwp", "rmnet", "mbim", "mhi"]
        .iter()
        .any(|prefix| value.starts_with(prefix))
}

pub async fn connectivity() -> ConnectivityCheckResponse {
    let (ipv4, ipv6) = tokio::join!(
        ping_host("223.5.5.5", false),
        ping_host("2400:3200::1", true)
    );
    ConnectivityCheckResponse { ipv4, ipv6 }
}

pub async fn ping_host(target: &str, ipv6: bool) -> PingResult {
    let mut command = tokio::process::Command::new("ping");
    #[cfg(windows)]
    command.args(if ipv6 {
        vec!["-6", "-n", "1", "-w", "1000", target]
    } else {
        vec!["-4", "-n", "1", "-w", "1000", target]
    });
    #[cfg(not(windows))]
    command.args(if ipv6 {
        vec!["-6", "-c", "1", "-W", "1", target]
    } else {
        vec!["-4", "-c", "1", "-W", "1", target]
    });
    match command.output().await {
        Ok(output) => {
            let text = format!(
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            PingResult {
                success: output.status.success(),
                latency_ms: parse_ping_latency(&text),
                target: target.into(),
                error: (!output.status.success()).then(|| text.trim().to_owned()),
            }
        }
        Err(error) => PingResult {
            success: false,
            latency_ms: None,
            target: target.into(),
            error: Some(error.to_string()),
        },
    }
}

fn parse_ping_latency(output: &str) -> Option<f64> {
    output
        .split_whitespace()
        .find_map(|token| {
            token
                .strip_prefix("time=")
                .or_else(|| token.strip_prefix("time<"))
        })
        .and_then(|value| value.trim_end_matches("ms").parse().ok())
}

fn read_trimmed(path: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ping_latency() {
        assert_eq!(parse_ping_latency("time=12.4 ms"), Some(12.4));
        assert_eq!(parse_ping_latency("time<1ms"), Some(1.0));
    }

    #[test]
    fn system_routes_are_explicit() {
        assert!(SystemRuntime::handles("/stats"));
        assert!(SystemRuntime::handles("/network/interfaces?all=true"));
        assert!(!SystemRuntime::handles("/sim"));
    }

    #[test]
    fn selects_lowest_metric_default_routes() {
        let ipv4 = "Iface Destination Gateway Flags RefCnt Use Metric Mask\n\
                    wwan0 00000000 00000000 0003 0 0 600 00000000\n\
                    wlan0 00000000 00000000 0003 0 0 100 00000000\n";
        assert_eq!(parse_default_ipv4_interface(ipv4).as_deref(), Some("wlan0"));

        let ipv6 = "00000000000000000000000000000000 00 00000000000000000000000000000000 00 00000000000000000000000000000000 00000258 00000000 00000000 00000001 wwan0\n\
                    00000000000000000000000000000000 00 00000000000000000000000000000000 00 00000000000000000000000000000000 00000064 00000000 00000000 00000001 wlan0\n";
        assert_eq!(parse_default_ipv6_interface(ipv6).as_deref(), Some("wlan0"));
    }

    #[test]
    fn classifies_network_interfaces_and_sensor_labels() {
        assert!(is_cellular_interface("wwan0"));
        assert!(is_cellular_interface("rmnet_data0"));
        assert!(!is_cellular_interface("wlan0"));
        assert_eq!(temperature_label("modem-thermal", "thermal_zone0"), "基带");
        assert_eq!(temperature_label("gpu0", "thermal_zone1"), "GPU");
    }

    #[cfg(unix)]
    #[test]
    fn classifies_ip_address_scope() {
        assert_eq!(ip_scope(&"127.0.0.1".parse().unwrap()), "loopback");
        assert_eq!(ip_scope(&"169.254.1.1".parse().unwrap()), "link-local");
        assert_eq!(ip_scope(&"192.168.1.2".parse().unwrap()), "private");
        assert_eq!(ip_scope(&"2408:8000::1".parse().unwrap()), "public");
    }
}
