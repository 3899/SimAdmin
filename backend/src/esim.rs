//! eUICC profile management for removable eSIM cards.
//!
//! In this project "eSIM mode" is a feature gate for managing profiles stored
//! on a physical eUICC SIM card inserted in the device. It does not switch
//! board-level SIM hardware and does not start background workers.

use std::env;
use std::fs;
use std::io::{self, Cursor};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;
use tokio::sync::Mutex;

use crate::config::ConfigManager;
use crate::models::{
    EsimCommandResponse, EsimDownloadRequest, EsimEuiccInfo, EsimLpacRepairRequest,
    EsimLpacRepairResponse, EsimLpacStatusResponse, EsimProfile, EsimProfilesResponse, WorkMode,
    WorkModeResponse,
};

const ESIM_SHORT_TIMEOUT_SECS: u64 = 20;
const ESIM_LONG_TIMEOUT_SECS: u64 = 60;
const ESIM_SWITCH_PREFLIGHT_TIMEOUT_SECS: u64 = 5;
const LPAC_REPAIR_TIMEOUT_SECS: u64 = 120;
const LPAC_PROBE_TIMEOUT_SECS: u64 = 3;
const MAX_LPAC_DOWNLOAD_BYTES: usize = 25 * 1024 * 1024;
const LPAC_OFFICIAL_RELEASE_BASE_URL: &str =
    "https://github.com/estkme-group/lpac/releases/latest/download";
const LPAC_COMPAT_RELEASE_BASE_URL: &str =
    "https://github.com/3899/SimAdmin/releases/download/lpac";
const LPAC_COMPAT_MANIFEST_NAME: &str = "lpac.json";
const PRIVATE_LPAC_DIR: &str = "/opt/simadmin/lpac";
const PRIVATE_LPAC_PATH: &str = "/opt/simadmin/lpac/lpac";

#[derive(Debug, Clone)]
struct LpacAssetCandidate {
    name: String,
    url: String,
}

#[derive(Debug)]
pub enum EsimApiError {
    Disabled,
    Unavailable(String),
    Command(String),
}

impl EsimApiError {
    pub fn message(&self) -> String {
        match self {
            Self::Disabled => "eSIM module is disabled in current work mode".to_string(),
            Self::Unavailable(message) | Self::Command(message) => message.clone(),
        }
    }
}

pub struct EsimSupervisor {
    config_manager: Arc<ConfigManager>,
    lpac_lock: Mutex<()>,
}

impl EsimSupervisor {
    pub fn new(config_manager: Arc<ConfigManager>) -> Self {
        Self {
            config_manager,
            lpac_lock: Mutex::new(()),
        }
    }

    pub async fn worker_running(&self) -> bool {
        self.config_manager.get_work_mode() == WorkMode::Esim
    }

    pub async fn switch_mode(&self, target: WorkMode) -> Result<WorkModeResponse, String> {
        self.config_manager.set_work_mode(target)?;
        let mode = self.config_manager.get_work_mode();
        Ok(WorkModeResponse {
            mode,
            // Kept for API compatibility with v1.0.5 clients. There is no
            // worker after the simplification; true means eSIM APIs are enabled.
            worker_running: mode == WorkMode::Esim,
        })
    }

    pub async fn get_lpac_status(&self) -> Result<EsimLpacStatusResponse, EsimApiError> {
        if self.config_manager.get_work_mode() != WorkMode::Esim {
            return Err(EsimApiError::Disabled);
        }

        let _guard = self.lpac_lock.lock().await;
        let raw_arch = detect_machine_arch()
            .await
            .unwrap_or_else(|err| format!("unknown ({err})"));
        let arch = normalize_lpac_arch(&raw_arch).unwrap_or("").to_string();
        let glibc_version = detect_glibc_version().await.unwrap_or_default();
        let asset_name = if arch.is_empty() {
            String::new()
        } else {
            recommended_lpac_asset_name(&arch, &glibc_version)
        };
        let command_path = resolve_lpac_path(&self.config_manager.get_esim_config().lpac_path);
        let probe = probe_lpac_binary(&command_path).await;
        let message = if arch.is_empty() && !probe.usable {
            format!("unsupported device architecture: {raw_arch}")
        } else {
            probe.message
        };

        Ok(EsimLpacStatusResponse {
            installed: probe.installed,
            usable: probe.usable,
            path: command_path.to_string_lossy().to_string(),
            arch,
            glibc_version,
            asset_name,
            message,
            source: read_lpac_source(),
        })
    }

    pub async fn repair_lpac(
        &self,
        request: EsimLpacRepairRequest,
    ) -> Result<EsimLpacRepairResponse, EsimApiError> {
        if self.config_manager.get_work_mode() != WorkMode::Esim {
            return Err(EsimApiError::Disabled);
        }

        let _guard = self.lpac_lock.lock().await;
        let raw_arch = detect_machine_arch()
            .await
            .map_err(|err| EsimApiError::Command(format!("Failed to detect device arch: {err}")))?;
        let arch = normalize_lpac_arch(&raw_arch).ok_or_else(|| {
            EsimApiError::Command(format!("unsupported device architecture: {raw_arch}"))
        })?;
        let glibc_version = detect_glibc_version().await.unwrap_or_default();
        let requested_asset_url = request
            .asset_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let proxy_prefix = crate::ota::normalize_proxy_prefix(request.proxy_prefix);
        let candidates = match requested_asset_url {
            Some(asset_url) => vec![LpacAssetCandidate {
                name: asset_url
                    .rsplit('/')
                    .next()
                    .filter(|value| !value.is_empty())
                    .unwrap_or("custom-lpac.zip")
                    .to_string(),
                url: asset_url,
            }],
            None => resolve_lpac_asset_candidates(arch, &glibc_version, &proxy_prefix).await,
        };

        let mut errors = Vec::new();
        for candidate in candidates {
            let result = async {
                let bytes = download_lpac_asset(&candidate.url, &proxy_prefix).await?;
                install_lpac_asset(&bytes, &candidate.url).await?;
                let probe = probe_lpac_binary(Path::new(PRIVATE_LPAC_PATH)).await;
                if !probe.usable {
                    return Err(EsimApiError::Command(format!(
                        "lpac was installed but is not usable: {}",
                        probe.message
                    )));
                }
                Ok::<(), EsimApiError>(())
            }
            .await;

            match result {
                Ok(()) => {
                    return Ok(EsimLpacRepairResponse {
                        installed: true,
                        path: PRIVATE_LPAC_PATH.to_string(),
                        arch: arch.to_string(),
                        asset_name: candidate.name,
                        asset_url: candidate.url,
                        message: "lpac installed and verified".to_string(),
                    });
                }
                Err(err) => errors.push(format!("{}: {}", candidate.name, err.message())),
            }
        }

        Err(EsimApiError::Command(format!(
            "No compatible lpac asset worked for arch={arch}, glibc={}. Tried: {}",
            if glibc_version.is_empty() {
                "unknown"
            } else {
                glibc_version.as_str()
            },
            errors.join(" | ")
        )))
    }

    async fn call_lpac(
        &self,
        action: &str,
        args: &[&str],
        timeout_seconds: u64,
    ) -> Result<EsimCommandResponse, EsimApiError> {
        if self.config_manager.get_work_mode() != WorkMode::Esim {
            return Err(EsimApiError::Disabled);
        }

        let _guard = self.lpac_lock.lock().await;
        run_lpac_command(
            &self.config_manager.get_esim_config().lpac_path,
            action,
            args,
            timeout_seconds,
        )
        .await
    }

    pub async fn get_euicc_info(&self) -> Result<EsimEuiccInfo, EsimApiError> {
        let response = self
            .call_lpac("info", &["chip", "info"], ESIM_SHORT_TIMEOUT_SECS)
            .await?;
        if !command_succeeded(&response) {
            return Err(EsimApiError::Command(response.msg));
        }
        let mut info = normalize_euicc_info(response);
        if info.memory_total_kb.is_none() {
            info.memory_total_customizable = Some(true);
            let esim_config = self.config_manager.get_esim_config();
            if let Some(total_kb) = esim_config.custom_memory_total_kb {
                info.memory_total_kb = Some(total_kb as f64);
            }
        } else {
            info.memory_total_customizable = Some(false);
        }
        Ok(info)
    }

    pub async fn get_profiles(&self) -> Result<EsimProfilesResponse, EsimApiError> {
        let response = self
            .call_lpac("profiles", &["profile", "list"], ESIM_SHORT_TIMEOUT_SECS)
            .await?;
        if !command_succeeded(&response) {
            return Err(EsimApiError::Command(response.msg));
        }
        Ok(normalize_profiles(response))
    }

    pub async fn get_profiles_for_switch(&self) -> Result<EsimProfilesResponse, EsimApiError> {
        let response = self
            .call_lpac(
                "profiles",
                &["profile", "list"],
                ESIM_SWITCH_PREFLIGHT_TIMEOUT_SECS,
            )
            .await?;
        if !command_succeeded(&response) {
            return Err(EsimApiError::Command(response.msg));
        }
        Ok(normalize_profiles(response))
    }

    pub async fn enable_profile(&self, iccid: String) -> Result<EsimCommandResponse, EsimApiError> {
        self.enable_profile_with_refresh_flag(iccid, true).await
    }

    pub async fn enable_profile_with_refresh_flag(
        &self,
        iccid: String,
        refresh: bool,
    ) -> Result<EsimCommandResponse, EsimApiError> {
        let refresh_flag = if refresh { "1" } else { "0" };
        self.call_lpac(
            "enable",
            &["profile", "enable", iccid.as_str(), refresh_flag],
            ESIM_LONG_TIMEOUT_SECS,
        )
        .await
    }

    pub async fn rename_profile(
        &self,
        iccid: String,
        name: String,
    ) -> Result<EsimCommandResponse, EsimApiError> {
        self.call_lpac(
            "rename",
            &["profile", "nickname", iccid.as_str(), name.as_str()],
            ESIM_LONG_TIMEOUT_SECS,
        )
        .await
    }

    pub async fn delete_profile(&self, iccid: String) -> Result<EsimCommandResponse, EsimApiError> {
        self.call_lpac(
            "delete",
            &["profile", "delete", iccid.as_str()],
            ESIM_LONG_TIMEOUT_SECS,
        )
        .await
    }

    pub async fn download_profile(
        &self,
        request: EsimDownloadRequest,
    ) -> Result<EsimCommandResponse, EsimApiError> {
        let mut args = vec![
            "profile",
            "download",
            "-s",
            request.smdp.as_str(),
            "-m",
            request.matching_id.as_str(),
        ];

        let cc = request.confirmation_code.as_deref().unwrap_or("").trim();
        if !cc.is_empty() {
            args.push("-c");
            args.push(cc);
        }

        let imei = request.imei.as_deref().unwrap_or("").trim();
        if !imei.is_empty() {
            args.push("-i");
            args.push(imei);
        }

        // download can take up to 120 seconds
        self.call_lpac("download", &args, 120).await
    }
}

fn command_succeeded(response: &EsimCommandResponse) -> bool {
    response.code == 0
        && (response.status.is_empty()
            || response.status.eq_ignore_ascii_case("success")
            || response.status.eq_ignore_ascii_case("ok"))
}

struct LpacProbe {
    installed: bool,
    usable: bool,
    message: String,
}

async fn detect_machine_arch() -> Result<String, String> {
    let output = tokio::time::timeout(
        Duration::from_secs(LPAC_PROBE_TIMEOUT_SECS),
        tokio::process::Command::new("uname").arg("-m").output(),
    )
    .await
    .map_err(|_| "uname -m timed out".to_string())?
    .map_err(|err| format!("failed to run uname -m: {err}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("uname -m exited with {}", output.status)
        } else {
            stderr
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn normalize_lpac_arch(raw: &str) -> Option<&'static str> {
    match raw.trim() {
        "aarch64" | "arm64" => Some("aarch64"),
        "x86_64" | "amd64" => Some("x86_64"),
        _ => None,
    }
}

async fn detect_glibc_version() -> Result<String, String> {
    if let Ok(output) = tokio::time::timeout(
        Duration::from_secs(LPAC_PROBE_TIMEOUT_SECS),
        tokio::process::Command::new("getconf")
            .arg("GNU_LIBC_VERSION")
            .output(),
    )
    .await
    {
        if let Ok(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if let Some(version) = stdout.split_whitespace().last() {
                    if !version.trim().is_empty() {
                        return Ok(version.trim().to_string());
                    }
                }
            }
        }
    }

    let output = tokio::time::timeout(
        Duration::from_secs(LPAC_PROBE_TIMEOUT_SECS),
        tokio::process::Command::new("ldd")
            .arg("--version")
            .output(),
    )
    .await
    .map_err(|_| "ldd --version timed out".to_string())?
    .map_err(|err| format!("failed to run ldd --version: {err}"))?;

    let text = String::from_utf8_lossy(&output.stdout);
    find_version_token(&text).ok_or_else(|| "failed to parse glibc version".to_string())
}

fn find_version_token(text: &str) -> Option<String> {
    text.split_whitespace().find_map(|token| {
        let token = token.trim_matches(|ch: char| !ch.is_ascii_digit() && ch != '.');
        let has_dot = token.contains('.');
        let numeric = token.chars().all(|ch| ch.is_ascii_digit() || ch == '.');
        (has_dot && numeric).then(|| token.to_string())
    })
}

fn recommended_lpac_asset_name(arch: &str, glibc_version: &str) -> String {
    if matches!(arch, "aarch64" | "x86_64") && version_le("2.31", glibc_version).unwrap_or(false) {
        return format!("lpac-linux-{arch}-glibc2.31.zip");
    }
    format!("lpac-linux-{arch}-with-qmi.zip")
}

fn official_lpac_asset_names(arch: &str) -> [String; 3] {
    [
        format!("lpac-linux-{arch}-with-qmi.zip"),
        format!("lpac-linux-{arch}.zip"),
        format!("lpac-linux-{arch}-without-lto.zip"),
    ]
}

async fn resolve_lpac_asset_candidates(
    arch: &str,
    glibc_version: &str,
    proxy_prefix: &str,
) -> Vec<LpacAssetCandidate> {
    let mut candidates = Vec::new();

    if let Ok(mut manifest_candidates) =
        fetch_compatible_lpac_candidates(arch, glibc_version, proxy_prefix).await
    {
        candidates.append(&mut manifest_candidates);
    }

    for name in official_lpac_asset_names(arch) {
        candidates.push(LpacAssetCandidate {
            url: format!("{LPAC_OFFICIAL_RELEASE_BASE_URL}/{name}"),
            name,
        });
    }

    dedupe_lpac_candidates(candidates)
}

async fn fetch_compatible_lpac_candidates(
    arch: &str,
    glibc_version: &str,
    proxy_prefix: &str,
) -> Result<Vec<LpacAssetCandidate>, EsimApiError> {
    let manifest_url = format!("{LPAC_COMPAT_RELEASE_BASE_URL}/{LPAC_COMPAT_MANIFEST_NAME}");
    let bytes = download_lpac_asset(&manifest_url, proxy_prefix).await?;
    let manifest = serde_json::from_slice::<Value>(&bytes)
        .map_err(|err| EsimApiError::Command(format!("Invalid lpac asset manifest: {err}")))?;
    let Some(assets) = manifest.get("assets").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };

    let mut items = assets
        .iter()
        .filter_map(|asset| {
            let name = asset.get("name").and_then(Value::as_str)?;
            let asset_arch = asset.get("arch").and_then(Value::as_str)?;
            if asset_arch != arch {
                return None;
            }
            let asset_glibc = asset.get("glibc").and_then(Value::as_str).unwrap_or("");
            if !asset_glibc.is_empty()
                && !glibc_version.is_empty()
                && !version_le(asset_glibc, glibc_version).unwrap_or(false)
            {
                return None;
            }
            Some((asset_glibc.to_string(), name.to_string()))
        })
        .collect::<Vec<_>>();

    items.sort_by(|left, right| compare_version(&right.0, &left.0).then(left.1.cmp(&right.1)));

    Ok(items
        .into_iter()
        .map(|(_, name)| LpacAssetCandidate {
            url: format!("{LPAC_COMPAT_RELEASE_BASE_URL}/{name}"),
            name,
        })
        .collect())
}

fn dedupe_lpac_candidates(candidates: Vec<LpacAssetCandidate>) -> Vec<LpacAssetCandidate> {
    let mut seen = std::collections::HashSet::new();
    candidates
        .into_iter()
        .filter(|candidate| seen.insert(candidate.url.clone()))
        .collect()
}

fn version_le(left: &str, right: &str) -> Option<bool> {
    Some(compare_version(left, right) != std::cmp::Ordering::Greater)
}

fn compare_version(left: &str, right: &str) -> std::cmp::Ordering {
    let left = parse_version_parts(left);
    let right = parse_version_parts(right);
    let len = left.len().max(right.len());
    for index in 0..len {
        let a = *left.get(index).unwrap_or(&0);
        let b = *right.get(index).unwrap_or(&0);
        match a.cmp(&b) {
            std::cmp::Ordering::Equal => continue,
            ordering => return ordering,
        }
    }
    std::cmp::Ordering::Equal
}

fn parse_version_parts(value: &str) -> Vec<u32> {
    value
        .split('.')
        .filter_map(|part| part.parse::<u32>().ok())
        .collect()
}

async fn probe_lpac_binary(command_path: &Path) -> LpacProbe {
    let mut command = tokio::process::Command::new(command_path);
    command.args(["driver", "list"]);
    configure_lpac_environment(&mut command, command_path);
    command.env("LPAC_APDU", "stdio");
    command.env("LPAC_HTTP", "stdio");

    let output = match tokio::time::timeout(
        Duration::from_secs(LPAC_PROBE_TIMEOUT_SECS),
        command.output(),
    )
    .await
    {
        Ok(Ok(output)) => output,
        Ok(Err(err)) if err.kind() == std::io::ErrorKind::NotFound => {
            return LpacProbe {
                installed: false,
                usable: false,
                message: "lpac is not installed or not found in PATH".to_string(),
            };
        }
        Ok(Err(err)) => {
            return LpacProbe {
                installed: command_path.exists(),
                usable: false,
                message: format!("Failed to run lpac: {err}"),
            };
        }
        Err(_) => {
            return LpacProbe {
                installed: command_path.exists(),
                usable: false,
                message: "lpac probe timed out".to_string(),
            };
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let combined = format!("{stdout}\n{stderr}");
    if !output.status.success() {
        return LpacProbe {
            installed: true,
            usable: false,
            message: if stderr.is_empty() {
                if stdout.is_empty() {
                    format!("lpac driver probe exited with {}", output.status)
                } else {
                    stdout
                }
            } else {
                stderr
            },
        };
    }

    if !lpac_driver_list_has_required_drivers(&combined) {
        return LpacProbe {
            installed: true,
            usable: false,
            message: "lpac does not provide the required qmi APDU and curl HTTP drivers"
                .to_string(),
        };
    }

    LpacProbe {
        installed: true,
        usable: true,
        message: "lpac is available".to_string(),
    }
}

fn lpac_driver_list_has_required_drivers(output: &str) -> bool {
    let value = output
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str::<Value>(line.trim()).ok());
    let Some(value) = value else {
        return false;
    };
    let payload = value.get("payload").unwrap_or(&value);
    let has_driver = |kind: &str, name: &str| {
        payload
            .get(kind)
            .and_then(Value::as_array)
            .map(|drivers| drivers.iter().any(|driver| driver.as_str() == Some(name)))
            .unwrap_or(false)
    };
    has_driver("LPAC_APDU", "qmi") && has_driver("LPAC_HTTP", "curl")
}

fn read_lpac_source() -> Option<String> {
    fs::read_to_string(Path::new(PRIVATE_LPAC_DIR).join("SOURCE.txt"))
        .ok()
        .map(|content| content.trim().to_string())
        .filter(|content| !content.is_empty())
}

async fn download_lpac_asset(asset_url: &str, proxy_prefix: &str) -> Result<Vec<u8>, EsimApiError> {
    let client = reqwest::Client::builder()
        .user_agent("SimAdmin lpac repair")
        .timeout(Duration::from_secs(LPAC_REPAIR_TIMEOUT_SECS))
        .build()
        .map_err(|err| EsimApiError::Command(format!("Failed to create HTTP client: {err}")))?;

    let mut urls = Vec::new();
    if !proxy_prefix.is_empty() {
        urls.push(format!("{proxy_prefix}{asset_url}"));
    }
    urls.push(asset_url.to_string());

    let mut last_error = String::new();
    for url in urls {
        match client.get(&url).send().await {
            Ok(response) => {
                let status = response.status();
                if !status.is_success() {
                    last_error = format!("lpac download failed: HTTP {status}");
                    continue;
                }
                if let Some(size) = response.content_length() {
                    if size > MAX_LPAC_DOWNLOAD_BYTES as u64 {
                        return Err(EsimApiError::Command(format!(
                            "lpac asset is too large: {size} bytes"
                        )));
                    }
                }
                let bytes = response.bytes().await.map_err(|err| {
                    EsimApiError::Command(format!("Failed to read lpac asset: {err}"))
                })?;
                if bytes.len() > MAX_LPAC_DOWNLOAD_BYTES {
                    return Err(EsimApiError::Command(format!(
                        "lpac asset is too large: {} bytes",
                        bytes.len()
                    )));
                }
                return Ok(bytes.to_vec());
            }
            Err(err) => {
                last_error = format!("Failed to download lpac asset: {err}");
            }
        }
    }

    Err(EsimApiError::Command(last_error))
}

async fn install_lpac_asset(bytes: &[u8], asset_url: &str) -> Result<(), EsimApiError> {
    let stamp = current_millis();
    let temp_dir = env::temp_dir().join(format!("simadmin-lpac-repair-{stamp}"));
    let extract_dir = temp_dir.join("extract");
    let install_parent = Path::new(PRIVATE_LPAC_DIR)
        .parent()
        .ok_or_else(|| EsimApiError::Command("Invalid lpac install path".to_string()))?
        .to_path_buf();
    let new_dir = install_parent.join(format!(".lpac-new-{stamp}"));
    let backup_dir = install_parent.join(format!(".lpac-old-{stamp}"));

    let result = async {
        fs::create_dir_all(&extract_dir)
            .map_err(|err| EsimApiError::Command(format!("Failed to create temp dir: {err}")))?;
        extract_lpac_archive(bytes, &extract_dir).await?;

        let bundle_root = find_lpac_root(&extract_dir).ok_or_else(|| {
            EsimApiError::Command("downloaded lpac asset does not contain lpac executable".to_string())
        })?;

        if new_dir.exists() {
            fs::remove_dir_all(&new_dir).map_err(|err| {
                EsimApiError::Command(format!("Failed to clean pending lpac dir: {err}"))
            })?;
        }
        fs::create_dir_all(&new_dir)
            .map_err(|err| EsimApiError::Command(format!("Failed to create lpac dir: {err}")))?;
        copy_dir_recursive(&bundle_root, &new_dir)?;
        copy_optional_lpac_libs(&extract_dir, &new_dir)?;
        fs::write(
            new_dir.join("SOURCE.txt"),
            format!(
                "lpac is installed from:\n{asset_url}\n\nProject:\nhttps://github.com/estkme-group/lpac\n"
            ),
        )
        .map_err(|err| EsimApiError::Command(format!("Failed to write lpac source: {err}")))?;
        chmod_lpac_tree(&new_dir).await;
        activate_lpac_tree(&new_dir, &backup_dir).await
    }
    .await;

    let _ = fs::remove_dir_all(&temp_dir);
    if result.is_err() {
        let _ = fs::remove_dir_all(&new_dir);
    }
    result
}

async fn extract_lpac_archive(bytes: &[u8], target_dir: &Path) -> Result<(), EsimApiError> {
    let bytes = bytes.to_vec();
    let target_dir = target_dir.to_path_buf();
    tokio::task::spawn_blocking(move || extract_zip_archive(bytes, &target_dir))
        .await
        .map_err(|err| EsimApiError::Command(format!("lpac extraction task failed: {err}")))?
}

fn extract_zip_archive(bytes: Vec<u8>, target_dir: &Path) -> Result<(), EsimApiError> {
    let reader = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|err| EsimApiError::Command(format!("Invalid lpac zip archive: {err}")))?;

    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(|err| {
            EsimApiError::Command(format!("Failed to read lpac zip entry: {err}"))
        })?;
        let Some(path) = file.enclosed_name().map(|path| target_dir.join(path)) else {
            continue;
        };

        if file.is_dir() {
            fs::create_dir_all(&path).map_err(|err| {
                EsimApiError::Command(format!("Failed to create extracted directory: {err}"))
            })?;
            continue;
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                EsimApiError::Command(format!("Failed to create extracted parent dir: {err}"))
            })?;
        }
        let mut output = fs::File::create(&path).map_err(|err| {
            EsimApiError::Command(format!("Failed to create extracted file: {err}"))
        })?;
        io::copy(&mut file, &mut output)
            .map_err(|err| EsimApiError::Command(format!("Failed to extract lpac file: {err}")))?;
    }

    Ok(())
}

fn find_lpac_root(root: &Path) -> Option<PathBuf> {
    let direct = root.join("lpac");
    if direct.is_file() {
        return Some(root.to_path_buf());
    }

    let executables = root.join("executables").join("lpac");
    if executables.is_file() {
        return executables.parent().map(Path::to_path_buf);
    }

    find_file_named(root, "lpac").and_then(|path| path.parent().map(Path::to_path_buf))
}

fn find_file_named(root: &Path, name: &str) -> Option<PathBuf> {
    let entries = fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.file_name().and_then(|value| value.to_str()) == Some(name) && path.is_file() {
            return Some(path);
        }
        if path.is_dir() {
            if let Some(found) = find_file_named(&path, name) {
                return Some(found);
            }
        }
    }
    None
}

fn copy_optional_lpac_libs(extract_dir: &Path, target_dir: &Path) -> Result<(), EsimApiError> {
    let target_lib = target_dir.join("lib");
    if target_lib.exists() {
        return Ok(());
    }

    for name in ["lib", "libraries"] {
        let source = extract_dir.join(name);
        if source.is_dir() {
            fs::create_dir_all(&target_lib).map_err(|err| {
                EsimApiError::Command(format!("Failed to create lpac lib dir: {err}"))
            })?;
            copy_dir_recursive(&source, &target_lib)?;
            return Ok(());
        }
    }
    Ok(())
}

fn copy_dir_recursive(source: &Path, target: &Path) -> Result<(), EsimApiError> {
    fs::create_dir_all(target)
        .map_err(|err| EsimApiError::Command(format!("Failed to create directory: {err}")))?;
    for entry in fs::read_dir(source)
        .map_err(|err| EsimApiError::Command(format!("Failed to read directory: {err}")))?
    {
        let entry =
            entry.map_err(|err| EsimApiError::Command(format!("Failed to read entry: {err}")))?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        let file_type = entry
            .file_type()
            .map_err(|err| EsimApiError::Command(format!("Failed to read file type: {err}")))?;
        if file_type.is_dir() {
            copy_dir_recursive(&source_path, &target_path)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &target_path).map_err(|err| {
                EsimApiError::Command(format!("Failed to copy {}: {err}", source_path.display()))
            })?;
        }
    }
    Ok(())
}

async fn chmod_lpac_tree(target_dir: &Path) {
    let _ = tokio::process::Command::new("chmod")
        .arg("-R")
        .arg("a+rX")
        .arg(target_dir)
        .output()
        .await;
    let _ = tokio::process::Command::new("chmod")
        .arg("0755")
        .arg(target_dir.join("lpac"))
        .output()
        .await;
}

async fn activate_lpac_tree(new_dir: &Path, backup_dir: &Path) -> Result<(), EsimApiError> {
    let target_dir = Path::new(PRIVATE_LPAC_DIR);
    if let Some(parent) = target_dir.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| EsimApiError::Command(format!("Failed to create install dir: {err}")))?;
    }

    if backup_dir.exists() {
        fs::remove_dir_all(backup_dir)
            .map_err(|err| EsimApiError::Command(format!("Failed to clean backup dir: {err}")))?;
    }

    let had_existing = target_dir.exists();
    if had_existing {
        fs::rename(target_dir, backup_dir).map_err(|err| {
            EsimApiError::Command(format!("Failed to backup existing lpac: {err}"))
        })?;
    }

    let install_result = fs::rename(new_dir, target_dir)
        .map_err(|err| EsimApiError::Command(format!("Failed to install lpac: {err}")));
    if let Err(err) = install_result {
        if had_existing {
            let _ = fs::rename(backup_dir, target_dir);
        }
        return Err(err);
    }

    let probe = probe_lpac_binary(Path::new(PRIVATE_LPAC_PATH)).await;
    if !probe.usable {
        let _ = fs::remove_dir_all(target_dir);
        if had_existing {
            let _ = fs::rename(backup_dir, target_dir);
        }
        return Err(EsimApiError::Command(format!(
            "Installed lpac is not usable: {}",
            probe.message
        )));
    }

    if had_existing {
        let _ = fs::remove_dir_all(backup_dir);
    }
    Ok(())
}

fn current_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

async fn run_lpac_command(
    lpac_path: &str,
    action: &str,
    args: &[&str],
    timeout_seconds: u64,
) -> Result<EsimCommandResponse, EsimApiError> {
    let command_path = resolve_lpac_path(lpac_path);
    let mut command = tokio::process::Command::new(&command_path);
    command.args(args);
    configure_lpac_environment(&mut command, &command_path);

    let output = tokio::time::timeout(Duration::from_secs(timeout_seconds), command.output())
        .await
        .map_err(|_| {
            EsimApiError::Command(format!("lpac {action} timed out after {timeout_seconds}s"))
        })?
        .map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                EsimApiError::Unavailable(
                    "lpac is unavailable; use the eSIM Manager repair action, run \
                     install_latest.sh, or set esim.lpac_path"
                        .to_string(),
                )
            } else {
                EsimApiError::Command(format!("Failed to spawn lpac: {err}"))
            }
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if stdout.is_empty() {
        if output.status.success() {
            return Ok(EsimCommandResponse {
                code: 0,
                status: "ok".to_string(),
                action: action.to_string(),
                msg: "ok".to_string(),
                data: None,
            });
        }
        return Err(EsimApiError::Command(if stderr.is_empty() {
            format!("lpac {action} exited with status {}", output.status)
        } else {
            stderr
        }));
    }

    // Since lpac stdout can contain multiple JSON objects separated by whitespace/newlines (e.g. progress updates followed by the final lpa object),
    // we search from the end of the stdout to find the last valid JSON block that starts with {"type" or simply {.
    let mut parsed_value = None;
    let mut search_pos = stdout.len();
    while let Some(pos) = stdout[..search_pos].rfind(r#"{"type"#) {
        if let Ok(val) = serde_json::from_str::<Value>(&stdout[pos..]) {
            parsed_value = Some(val);
            break;
        }
        if pos == 0 {
            break;
        }
        search_pos = pos;
    }

    if parsed_value.is_none() {
        let mut search_pos = stdout.len();
        while let Some(pos) = stdout[..search_pos].rfind('{') {
            if let Ok(val) = serde_json::from_str::<Value>(&stdout[pos..]) {
                parsed_value = Some(val);
                break;
            }
            if pos == 0 {
                break;
            }
            search_pos = pos;
        }
    }

    let value = match parsed_value {
        Some(val) => val,
        None => serde_json::from_str::<Value>(&stdout).map_err(|err| {
            EsimApiError::Command(format!(
                "Invalid JSON from lpac {action}: {err}; stdout: {stdout}"
            ))
        })?,
    };

    Ok(normalize_lpac_response(
        action,
        value,
        stderr,
        output.status.success(),
    ))
}

fn resolve_lpac_path(lpac_path: &str) -> PathBuf {
    let configured = lpac_path.trim();
    if configured.is_empty() || configured == "lpac" || configured == PRIVATE_LPAC_PATH {
        let private_lpac = Path::new(PRIVATE_LPAC_PATH);
        if private_lpac.exists() {
            return private_lpac.to_path_buf();
        }
        return PathBuf::from("lpac");
    }

    PathBuf::from(configured)
}

fn configure_lpac_environment(command: &mut tokio::process::Command, command_path: &Path) {
    set_env_default(command, "LPAC_APDU", "qmi");
    set_env_default(command, "LPAC_HTTP", "curl");
    let qmi_device = discover_lpac_qmi_device();
    set_env_default(command, "LPAC_APDU_QMI_DEVICE", &qmi_device);
    set_env_default(command, "LPAC_APDU_QMI_UIM_SLOT", "1");
    set_env_default(command, "LPAC_APDU_AT_DEVICE", "/dev/wwan0at0");

    if let Some(parent) = command_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        let lib_dir = parent.join("lib");
        if lib_dir.is_dir() {
            let mut ld_library_path = lib_dir.to_string_lossy().to_string();
            if let Some(existing) = env::var_os("LD_LIBRARY_PATH") {
                ld_library_path.push(':');
                ld_library_path.push_str(&existing.to_string_lossy());
            }
            command.env("LD_LIBRARY_PATH", ld_library_path);
        }
    }
}

fn discover_lpac_qmi_device() -> String {
    let candidates = fs::read_dir("/dev")
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            name.starts_with("cdc-wdm") || (name.starts_with("wwan") && name.contains("qmi"))
        })
        .collect::<Vec<_>>();

    select_lpac_qmi_device(candidates)
        .unwrap_or_else(|| PathBuf::from("/dev/cdc-wdm0"))
        .to_string_lossy()
        .to_string()
}

fn select_lpac_qmi_device(mut candidates: Vec<PathBuf>) -> Option<PathBuf> {
    candidates.sort_by(|left, right| {
        let priority = |path: &Path| {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            (!name.starts_with("cdc-wdm"), name.to_string())
        };
        priority(left).cmp(&priority(right))
    });
    candidates.into_iter().next()
}

fn set_env_default(command: &mut tokio::process::Command, key: &str, value: &str) {
    if env::var_os(key).is_none() {
        command.env(key, value);
    }
}

fn normalize_lpac_response(
    action: &str,
    value: Value,
    stderr: String,
    process_success: bool,
) -> EsimCommandResponse {
    if value.get("code").is_some() && value.get("status").is_some() {
        let mut response: EsimCommandResponse =
            serde_json::from_value(value).unwrap_or_else(|_| EsimCommandResponse {
                code: if process_success { 0 } else { 1 },
                status: if process_success { "ok" } else { "error" }.to_string(),
                action: action.to_string(),
                msg: stderr.clone(),
                data: None,
            });
        if response.action.is_empty() {
            response.action = action.to_string();
        }
        if response.msg.is_empty() && !stderr.is_empty() {
            response.msg = stderr;
        }
        return response;
    }

    let payload = value.get("payload").unwrap_or(&value);
    let code = payload
        .get("code")
        .and_then(|item| item.as_i64())
        .unwrap_or(if process_success { 0 } else { 1 }) as i32;
    let msg = string_from(payload, &["message", "msg", "error"])
        .or_else(|| (!stderr.is_empty()).then_some(stderr))
        .unwrap_or_else(|| {
            if code == 0 {
                "success".to_string()
            } else {
                "lpac command failed".to_string()
            }
        });
    let data = payload
        .get("data")
        .cloned()
        .or_else(|| value.get("data").cloned());

    EsimCommandResponse {
        code,
        status: if code == 0 { "ok" } else { "error" }.to_string(),
        action: action.to_string(),
        msg,
        data,
    }
}

fn eum_from_eid(eid: &str) -> Option<&'static str> {
    let normalized = eid
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>()
        .to_ascii_uppercase();
    if normalized.len() < 8 {
        return None;
    }

    match &normalized[..8] {
        "89033023" => Some("Thales"),
        "89033024" | "89039011" => Some("Idemia"),
        "89044011" | "89044020" => Some("Giesecke+Devrient"),
        "89049032" | "89041030" => Some("STMicroelectronics"),
        "89043051" => Some("NXP Semiconductors"),
        "89034011" => Some("Valid"),
        "8904C012" => Some("Workz"),
        "89014022" | "89014052" => Some("Kigen"),
        "89046031" => Some("Infineon Technologies"),
        "89086030" => Some("EastcomPeace"),
        "89086016" => Some("HED"),
        "89086011" => Some("China Mobile IoT"),
        "89086002" => Some("Tongxin Micro"),
        "89086026" | "89086027" => Some("Tianyu"),
        "89086014" => Some("Hengbao"),
        "89086012" => Some("Datang Micro"),
        "89086004" => Some("ZTE ICT"),
        _ => None,
    }
}

fn normalize_euicc_info(response: EsimCommandResponse) -> EsimEuiccInfo {
    let data = response.data.unwrap_or(Value::Null);
    let root = data
        .get("euicc")
        .or_else(|| data.get("euiccInfo"))
        .unwrap_or(&data);
    let euicc_info2 = object_from(root, &["EUICCInfo2", "euiccInfo2", "euicc_info2"]);
    let ext_resource = euicc_info2
        .and_then(|info| object_from(info, &["extCardResource", "ExtCardResource"]))
        .or_else(|| object_from(root, &["extCardResource", "ExtCardResource"]));
    let eid = string_from(root, &["eid", "EID", "eidValue", "eid_value"])
        .or_else(|| string_from(&data, &["eid", "EID", "eidValue", "eid_value"]))
        .unwrap_or_default();
    let raw_manufacturer = string_from(root, &["manufacturer", "vendor", "maker"]).or_else(|| {
        euicc_info2.and_then(|info| string_from(info, &["manufacturer", "vendor", "maker"]))
    });

    EsimEuiccInfo {
        eid: eid.clone(),
        status: string_from(root, &["status", "state"]).unwrap_or_else(|| "ready".to_string()),
        manufacturer: eum_from_eid(&eid)
            .map(ToString::to_string)
            .or(raw_manufacturer)
            .unwrap_or_default(),
        memory_total_kb: number_from(
            root,
            &[
                "memory_total_kb",
                "total_kb",
                "total",
                "totalNonVolatileMemoryKb",
            ],
        )
        .or_else(|| {
            ext_resource.and_then(|resource| {
                memory_kb_from_bytes(
                    resource,
                    &[
                        "totalNonVolatileMemory",
                        "total_non_volatile_memory",
                        "nonVolatileMemory",
                    ],
                )
            })
        }),
        memory_available_kb: number_from(
            root,
            &[
                "memory_available_kb",
                "available_kb",
                "free_kb",
                "available",
                "freeNonVolatileMemoryKb",
            ],
        )
        .or_else(|| {
            ext_resource.and_then(|resource| {
                memory_kb_from_bytes(
                    resource,
                    &[
                        "freeNonVolatileMemory",
                        "free_non_volatile_memory",
                        "availableNonVolatileMemory",
                    ],
                )
            })
        }),
        memory_total_customizable: None,
        updated_at: None,
        raw: data,
    }
}

fn normalize_profiles(response: EsimCommandResponse) -> EsimProfilesResponse {
    let data = response.data.unwrap_or(Value::Null);
    let null = Value::Null;
    let profiles_value = if let Some(profiles) = data.get("profiles") {
        profiles
    } else if let Some(profile_info) = data.get("profileInfo") {
        profile_info
    } else if let Some(profile_info) = data.get("profile_info") {
        profile_info
    } else if data.is_array() {
        &data
    } else {
        &null
    };

    let profiles = profiles_value
        .as_array()
        .map(|items| items.iter().map(normalize_profile).collect())
        .unwrap_or_default();

    EsimProfilesResponse { profiles }
}

pub fn normalize_profile(value: &Value) -> EsimProfile {
    let null = Value::Null;
    let ppr = value.get("ppr").unwrap_or(&null);
    let operator = value
        .get("originalOperator")
        .or_else(|| value.get("original_operator"))
        .or_else(|| value.get("operator"))
        .unwrap_or(&null);
    let imsi = string_from(value, &["imsi", "IMSI", "profileImsi", "profile_imsi"]);
    let mut mcc = string_from(value, &["mcc", "MCC"])
        .or_else(|| string_from(operator, &["mcc", "MCC"]))
        .or_else(|| mccmnc_part(value, 0))
        .or_else(|| mccmnc_part(operator, 0));
    let mut mnc = string_from(value, &["mnc", "MNC"])
        .or_else(|| string_from(operator, &["mnc", "MNC"]))
        .or_else(|| mccmnc_part(value, 1))
        .or_else(|| mccmnc_part(operator, 1));
    if (mcc.is_none() || mnc.is_none()) && imsi.as_deref().is_some() {
        if let Some((imsi_mcc, imsi_mnc)) = split_mcc_mnc_from_imsi(imsi.as_deref().unwrap_or("")) {
            if mcc.is_none() {
                mcc = Some(imsi_mcc);
            }
            if mnc.is_none() {
                mnc = Some(imsi_mnc);
            }
        }
    }

    EsimProfile {
        iccid: {
            let raw_iccid = string_from(value, &["iccid", "ICCID", "id"]).unwrap_or_default();
            raw_iccid.chars().filter(|c| c.is_ascii_digit()).collect()
        },
        name: string_from(
            value,
            &[
                "profileNickname",
                "profile_nickname",
                "nickname",
                "name",
                "profileName",
                "profile_name",
                "serviceProviderName",
            ],
        )
        .unwrap_or_default(),
        provider: string_from(
            value,
            &[
                "serviceProviderName",
                "service_provider_name",
                "provider",
                "service_provider",
                "spn",
                "carrier",
                "operatorName",
                "profileOwner",
                "profileOwer",
            ],
        )
        .or_else(|| string_from(operator, &["name", "operatorName", "displayName"]))
        .or_else(|| string_value(operator))
        .or_else(|| string_from(value, &["profileName", "profile_name"]))
        .unwrap_or_default(),
        state: profile_state_from(value),
        profile_class: profile_class_from(value).unwrap_or_default(),
        imsi,
        msisdn: string_from(
            value,
            &[
                "msisdn",
                "MSISDN",
                "phone_number",
                "phoneNumber",
                "phone",
                "ownNumber",
                "own_number",
                "number",
            ],
        ),
        smsc: string_from(
            value,
            &[
                "smsc",
                "SMSC",
                "sms_center",
                "smsCenter",
                "smscAddress",
                "smsc_address",
            ],
        ),
        smdp: string_from(
            value,
            &[
                "smdp",
                "smdp_address",
                "smdpAddress",
                "smdp+",
                "smdpServer",
                "smdp_server",
                "dpAddress",
                "defaultDpAddress",
            ],
        ),
        matching_id: None,
        isdp_aid: string_from(value, &["isdpAid", "isdp_aid", "aid"]),
        mcc,
        mnc,
        disable_allowed: bool_from(value, &["disable_allowed", "disableAllowed"])
            .or_else(|| bool_from(ppr, &["disableAllowed", "disable_allowed"]))
            .or_else(|| policy_allows(value, &["disable", "disabling"]))
            .or(Some(true)),
        delete_allowed: bool_from(value, &["delete_allowed", "deleteAllowed"])
            .or_else(|| bool_from(ppr, &["deleteAllowed", "delete_allowed"]))
            .or_else(|| policy_allows(value, &["delete", "deletion"]))
            .or(Some(true)),
        updated_at: None,
        raw: value.clone(),
    }
}

fn object_from<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter()
        .find_map(|key| value.get(*key).filter(|item| item.is_object()))
}

fn string_value(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => {
            let trimmed = text.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        }
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

fn string_from(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(string_value))
}

fn number_from(value: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|item| match item {
            Value::Number(number) => number.as_f64(),
            Value::String(text) => text.trim().parse::<f64>().ok(),
            _ => None,
        })
    })
}

fn memory_kb_from_bytes(value: &Value, keys: &[&str]) -> Option<f64> {
    number_from(value, keys).map(|bytes| bytes / 1000.0)
}

fn bool_from(value: &Value, keys: &[&str]) -> Option<bool> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|item| match item {
            Value::Bool(flag) => Some(*flag),
            Value::Number(number) => number.as_i64().map(|n| n != 0),
            Value::String(text) if matches_bool(text, true) => Some(true),
            Value::String(text) if matches_bool(text, false) => Some(false),
            _ => None,
        })
    })
}

fn matches_bool(value: &str, expected: bool) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    let truthy = ["1", "true", "yes", "on", "enabled", "active", "allowed"];
    let falsy = ["0", "false", "no", "off", "disabled", "inactive", "denied"];
    if expected {
        truthy.contains(&normalized.as_str())
    } else {
        falsy.contains(&normalized.as_str())
    }
}

fn profile_state_from(value: &Value) -> String {
    for key in ["state", "status", "profileState", "profile_state"] {
        if let Some(raw) = value.get(key) {
            if let Some(state) = normalize_profile_state(raw) {
                return state;
            }
        }
    }
    bool_from(value, &["enabled", "active", "is_enabled", "is_active"])
        .map(|enabled| {
            if enabled {
                "enabled".to_string()
            } else {
                "disabled".to_string()
            }
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn normalize_profile_state(value: &Value) -> Option<String> {
    match value {
        Value::Number(number) => number.as_i64().map(|state| {
            if state == 1 {
                "enabled".to_string()
            } else if state == 0 {
                "disabled".to_string()
            } else {
                state.to_string()
            }
        }),
        Value::Bool(enabled) => Some(if *enabled {
            "enabled".to_string()
        } else {
            "disabled".to_string()
        }),
        Value::String(text) => {
            let normalized = text.trim().to_ascii_lowercase();
            if normalized.is_empty() {
                None
            } else if matches!(normalized.as_str(), "1" | "enabled" | "active") {
                Some("enabled".to_string())
            } else if matches!(normalized.as_str(), "0" | "disabled" | "inactive") {
                Some("disabled".to_string())
            } else {
                Some(normalized)
            }
        }
        _ => None,
    }
}

fn profile_class_from(value: &Value) -> Option<String> {
    for key in ["class", "profile_class", "profileClass"] {
        let Some(raw) = value.get(key) else {
            continue;
        };
        match raw {
            Value::Number(number) => {
                return number.as_i64().map(|class| match class {
                    0 => "test".to_string(),
                    1 => "provisioning".to_string(),
                    2 => "operational".to_string(),
                    _ => class.to_string(),
                });
            }
            _ => {
                if let Some(text) = string_value(raw) {
                    return Some(text);
                }
            }
        }
    }
    None
}

fn mccmnc_part(value: &Value, part: usize) -> Option<String> {
    let mccmnc = string_from(
        value,
        &[
            "mccmnc",
            "mcc_mnc",
            "mccMnc",
            "plmn",
            "operatorCode",
            "operator_code",
            "operatorIdentifier",
            "operator_identifier",
        ],
    )?;
    let mccmnc = mccmnc.trim();
    if part == 0 {
        (mccmnc.len() >= 3).then(|| mccmnc[..3].to_string())
    } else {
        (mccmnc.len() > 3).then(|| mccmnc[3..].to_string())
    }
}

fn split_mcc_mnc_from_imsi(imsi: &str) -> Option<(String, String)> {
    let digits = imsi.trim();
    if digits.len() < 5 || !digits.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let mcc = digits[..3].to_string();
    let mnc_len = if mcc == "460" {
        2
    } else {
        3.min(digits.len() - 3)
    };
    if digits.len() < 3 + mnc_len {
        return None;
    }
    Some((mcc, digits[3..3 + mnc_len].to_string()))
}

fn policy_allows(value: &Value, deny_markers: &[&str]) -> Option<bool> {
    let rules = value
        .get("profilePolicyRules")
        .or_else(|| value.get("profile_policy_rules"))
        .or_else(|| value.get("policyRules"))
        .or_else(|| value.get("rules"))?;
    let mut saw_rule = false;
    if let Some(items) = rules.as_array() {
        for item in items {
            let text = string_value(item).unwrap_or_else(|| item.to_string());
            let text = text.to_ascii_lowercase();
            if deny_markers.iter().any(|marker| text.contains(marker)) {
                return Some(false);
            }
            saw_rule = true;
        }
    } else {
        let text = string_value(rules).unwrap_or_else(|| rules.to_string());
        let text = text.to_ascii_lowercase();
        if deny_markers.iter().any(|marker| text.contains(marker)) {
            return Some(false);
        }
        saw_rule = !text.trim().is_empty();
    }
    saw_rule.then_some(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn recommends_compatible_lpac_for_supported_architectures() {
        assert_eq!(
            recommended_lpac_asset_name("x86_64", "2.39"),
            "lpac-linux-x86_64-glibc2.31.zip"
        );
        assert_eq!(
            recommended_lpac_asset_name("aarch64", "2.39"),
            "lpac-linux-aarch64-glibc2.31.zip"
        );
        assert_eq!(
            official_lpac_asset_names("x86_64")[0],
            "lpac-linux-x86_64-with-qmi.zip"
        );
    }

    #[test]
    fn validates_required_lpac_drivers() {
        let valid = json!({
            "type": "driver",
            "payload": {
                "LPAC_APDU": ["qmi", "pcsc", "stdio"],
                "LPAC_HTTP": ["curl", "stdio"]
            }
        });
        let missing_qmi = json!({
            "type": "driver",
            "payload": {
                "LPAC_APDU": ["pcsc", "at", "stdio"],
                "LPAC_HTTP": ["curl", "stdio"]
            }
        });

        assert!(lpac_driver_list_has_required_drivers(&valid.to_string()));
        assert!(!lpac_driver_list_has_required_drivers(
            &missing_qmi.to_string()
        ));
    }

    #[test]
    fn qmi_device_selection_prefers_cdc_wdm() {
        let selected = select_lpac_qmi_device(vec![
            PathBuf::from("/dev/wwan0qmi0"),
            PathBuf::from("/dev/cdc-wdm1"),
            PathBuf::from("/dev/cdc-wdm0"),
        ]);

        assert_eq!(selected, Some(PathBuf::from("/dev/cdc-wdm0")));
    }

    #[test]
    fn parses_lpac_chip_info_aliases() {
        let response = EsimCommandResponse {
            data: Some(json!({
                "eidValue": "89086030EXAMPLE",
                "EUICCInfo2": {
                    "extCardResource": {
                        "freeNonVolatileMemory": 405123,
                        "totalNonVolatileMemory": 478900
                    }
                }
            })),
            ..Default::default()
        };

        let info = normalize_euicc_info(response);
        assert_eq!(info.eid, "89086030EXAMPLE");
        assert_eq!(info.manufacturer, "EastcomPeace");
        assert_eq!(info.memory_available_kb, Some(405.123));
        assert_eq!(info.memory_total_kb, Some(478.9));
    }

    #[test]
    fn parses_lpac_profile_aliases() {
        let profile = normalize_profile(&json!({
            "iccid": "89812000EXAMPLEICCID00",
            "isdpAid": "TEST_ISDP_AID",
            "profileState": 1,
            "profileNickname": "主卡",
            "serviceProviderName": "BillionConnect",
            "profileName": "BillionConnect",
            "profileClass": 2,
            "imsi": "001010"
        }));

        assert_eq!(profile.name, "主卡");
        assert_eq!(profile.provider, "BillionConnect");
        assert_eq!(profile.state, "enabled");
        assert_eq!(profile.profile_class, "operational");
        assert_eq!(profile.mcc.as_deref(), Some("001"));
        assert_eq!(profile.mnc.as_deref(), Some("010"));
        assert_eq!(profile.disable_allowed, Some(true));
        assert_eq!(profile.delete_allowed, Some(true));
    }
}
