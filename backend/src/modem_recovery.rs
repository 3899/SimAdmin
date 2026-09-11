//! 调制解调器开机自愈资产管理模块
//!
//! 将开机看门狗自愈脚本 (simadmin-modem-recovery.sh) 及对应的 systemd 单元
//! (simadmin-modem-recovery.service) 编译期内嵌进二进制。
//! 在服务启动时自动检查宿主机环境并执行原子自愈，解决 OTA 升级场景下宿主机自愈脚本
//! 无法静默更新的问题，并保证文件意外丢失时可自动补齐。

#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::path::Path;
#[cfg(unix)]
use std::process::Command;
#[cfg(unix)]
use tracing::{info, warn};

#[cfg(unix)]
pub const SCRIPT_PATH: &str = "/usr/local/bin/simadmin-modem-recovery.sh";
#[cfg(unix)]
pub const SERVICE_PATH: &str = "/etc/systemd/system/simadmin-modem-recovery.service";
#[cfg(unix)]
pub const SERVICE_NAME: &str = "simadmin-modem-recovery.service";

pub const EMBEDDED_SCRIPT: &str =
    include_str!("../../scripts/system/simadmin-modem-recovery.sh");
pub const EMBEDDED_SERVICE: &str =
    include_str!("../../scripts/system/simadmin-modem-recovery.service");

#[cfg(unix)]
fn atomic_write_file(target_path: &Path, content: &str, mode: u32) -> std::io::Result<()> {
    let parent = target_path
        .parent()
        .unwrap_or_else(|| Path::new("/"));
    if !parent.exists() {
        fs::create_dir_all(parent)?;
    }

    let file_name = target_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("asset");
    let tmp_path = parent.join(format!(".{}.tmp.{}", file_name, std::process::id()));

    fs::write(&tmp_path, content.as_bytes())?;
    fs::set_permissions(&tmp_path, fs::Permissions::from_mode(mode))?;

    if let Err(err) = fs::rename(&tmp_path, target_path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(err);
    }
    Ok(())
}

/// 检查并确保宿主机上的开机看门狗脚本与 systemd 单元已同步最新内嵌版本并处于启用状态
#[cfg(unix)]
pub fn ensure_modem_recovery_assets_installed() {
    let script_target = Path::new(SCRIPT_PATH);
    let mut script_updated = false;

    if script_target.exists() {
        match fs::read_to_string(script_target) {
            Ok(current) => {
                if current != EMBEDDED_SCRIPT {
                    match atomic_write_file(script_target, EMBEDDED_SCRIPT, 0o755) {
                        Ok(()) => {
                            info!("Updated {} with latest embedded recovery logic", SCRIPT_PATH);
                            script_updated = true;
                        }
                        Err(e) => warn!(error = %e, "Failed to update {}", SCRIPT_PATH),
                    }
                } else if let Ok(metadata) = script_target.metadata() {
                    let mode = metadata.permissions().mode();
                    if mode & 0o111 != 0o111 {
                        let _ = fs::set_permissions(
                            script_target,
                            fs::Permissions::from_mode(0o755),
                        );
                        info!("Restored executable permissions (0755) on {}", SCRIPT_PATH);
                    }
                }
            }
            Err(e) => warn!(error = %e, "Failed to read {}", SCRIPT_PATH),
        }
    } else {
        match atomic_write_file(script_target, EMBEDDED_SCRIPT, 0o755) {
            Ok(()) => {
                info!("Installed embedded recovery script to {}", SCRIPT_PATH);
                script_updated = true;
            }
            Err(e) => warn!(error = %e, "Failed to install {}", SCRIPT_PATH),
        }
    }

    let service_target = Path::new(SERVICE_PATH);
    let mut service_updated = false;

    if service_target.exists() {
        match fs::read_to_string(service_target) {
            Ok(current) => {
                if current != EMBEDDED_SERVICE {
                    match atomic_write_file(service_target, EMBEDDED_SERVICE, 0o644) {
                        Ok(()) => {
                            info!("Updated {} with latest embedded service unit", SERVICE_PATH);
                            service_updated = true;
                        }
                        Err(e) => warn!(error = %e, "Failed to update {}", SERVICE_PATH),
                    }
                }
            }
            Err(e) => warn!(error = %e, "Failed to read {}", SERVICE_PATH),
        }
    } else {
        match atomic_write_file(service_target, EMBEDDED_SERVICE, 0o644) {
            Ok(()) => {
                info!("Installed embedded recovery service unit to {}", SERVICE_PATH);
                service_updated = true;
            }
            Err(e) => warn!(error = %e, "Failed to install {}", SERVICE_PATH),
        }
    }

    // 若服务单元被创建或更新，触发 systemd 重载配置
    if service_updated {
        let reload_status = Command::new("systemctl")
            .arg("daemon-reload")
            .status();
        match reload_status {
            Ok(status) if status.success() => {
                info!("Reloaded systemd daemon configuration");
            }
            Ok(status) => {
                warn!(exit_code = ?status.code(), "systemctl daemon-reload returned non-zero");
            }
            Err(e) => warn!(error = %e, "Failed to invoke systemctl daemon-reload"),
        }
    }

    // 检查服务是否启用（若是全新安装或被禁用则主动启用）
    if service_updated || script_updated {
        let is_enabled = Command::new("systemctl")
            .args(["is-enabled", "--quiet", SERVICE_NAME])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if !is_enabled {
            match Command::new("systemctl").args(["enable", SERVICE_NAME]).status() {
                Ok(status) if status.success() => {
                    info!("Enabled {}", SERVICE_NAME);
                }
                Ok(status) => {
                    warn!(exit_code = ?status.code(), "Failed to enable {}", SERVICE_NAME);
                }
                Err(e) => warn!(error = %e, "Failed to invoke systemctl enable {}", SERVICE_NAME),
            }
        }
    }
}

/// 非 Unix 平台空实现
#[cfg(not(unix))]
pub fn ensure_modem_recovery_assets_installed() {}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_recovery_assets_are_valid() {
        use super::*;
        assert!(!EMBEDDED_SCRIPT.trim().is_empty(), "Recovery script should not be empty");
        assert!(!EMBEDDED_SERVICE.trim().is_empty(), "Recovery service should not be empty");

        assert!(
            EMBEDDED_SCRIPT.contains("SimAdmin-ModemRecovery"),
            "Recovery script should contain tag identifier"
        );
        assert!(
            EMBEDDED_SCRIPT.contains("STARTUP_TIMEOUT_SECONDS"),
            "Recovery script should contain timeout config"
        );

        assert!(
            EMBEDDED_SERVICE.contains("ExecStart=/usr/local/bin/simadmin-modem-recovery.sh"),
            "Recovery service must execute the correct binary path"
        );
        assert!(
            EMBEDDED_SERVICE.contains("WantedBy=multi-user.target"),
            "Recovery service must install into multi-user.target"
        );
    }

    #[test]
    #[cfg(unix)]
    fn atomic_write_works_as_expected() {
        use super::*;
        let temp_dir = std::env::temp_dir().join(format!("simadmin_test_{}", std::process::id()));
        let target_file = temp_dir.join("test_script.sh");
        let content = "#!/bin/sh\necho test\n";

        let res = atomic_write_file(&target_file, content, 0o755);
        assert!(res.is_ok(), "atomic write should succeed");
        assert!(target_file.exists(), "target file should exist");

        let read_back = fs::read_to_string(&target_file).expect("should read back");
        assert_eq!(read_back, content);

        let mode = target_file.metadata().unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755, "mode should match 0755");

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
