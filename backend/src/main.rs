//! SimAdmin - Debian SIM Management Service
//!
//! A backend service for managing Debian-based modem and SIM devices.
//! Built with Rust + Axum + zbus.
//!

use anyhow::Result;
use axum::{
    extract::DefaultBodyLimit,
    http::{StatusCode, Uri},
    middleware,
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Router,
};
use clap::{Args as ClapArgs, Parser, Subcommand};
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use zbus::Connection;

mod auth;
mod automation;
mod backup;
mod cell_lock_store;
mod config;
mod db;
mod device_network;
mod device_status;
mod esim;
mod handlers;
mod iptables;
mod models;
mod modem_manager;
mod notification;
mod notification_queue;
mod ota;
mod serial;
mod sms_listener;
mod state;
mod system_event;
mod system_event_monitor;
mod utils;
mod verification_code;

use config::{get_default_config_path, ConfigManager};
use db::Database;
use device_network::DdnsManager;
use esim::EsimSupervisor;
use handlers::*;
use modem_manager::{ensure_nm_modem_profile, init_data_connection};
use notification::NotificationSender;
use notification_queue::*;
use state::AppState;
use system_event::{
    codes as system_event_codes, severity as system_event_severity, status as system_event_status,
    SystemEventEmitter,
};

/// 获取二进制文件同级目录下的 www 目录路径
fn get_www_dir() -> PathBuf {
    // 获取当前可执行文件的路径
    let exe_path = std::env::current_exe().expect("Failed to get executable path");

    // 获取可执行文件所在目录
    let exe_dir = exe_path
        .parent()
        .expect("Failed to get executable directory");

    // 拼接 www 目录
    exe_dir.join("www")
}

fn get_data_db_path() -> PathBuf {
    std::env::current_exe()
        .expect("Failed to get executable path")
        .parent()
        .expect("Failed to get executable directory")
        .join("data.db")
}

/// SPA fallback handler - 对于所有前端路由返回 index.html
async fn spa_fallback(uri: Uri) -> Response {
    let path = uri.path();

    // 如果是 API 路由，返回 404（不应该走到这里，但作为保险）
    if path.starts_with("/api/") {
        return (StatusCode::NOT_FOUND, "API endpoint not found").into_response();
    }

    // 获取 www 目录的绝对路径
    let www_dir = get_www_dir();

    // 构建请求文件的完整路径
    let requested_path = if path == "/" { "/index.html" } else { path };
    let file_path = www_dir.join(requested_path.trim_start_matches('/'));

    // 如果文件存在，返回文件内容
    if let Ok(content) = tokio::fs::read(&file_path).await {
        // 根据文件扩展名设置正确的 Content-Type
        let content_type = match file_path.extension().and_then(|ext| ext.to_str()) {
            Some("html") => "text/html; charset=utf-8",
            Some("css") => "text/css; charset=utf-8",
            Some("js") => "application/javascript; charset=utf-8",
            Some("json") => "application/json",
            Some("png") => "image/png",
            Some("jpg") | Some("jpeg") => "image/jpeg",
            Some("gif") => "image/gif",
            Some("svg") => "image/svg+xml",
            Some("ico") => "image/x-icon",
            _ => "application/octet-stream",
        };

        return (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, content_type)],
            content,
        )
            .into_response();
    }

    // 如果文件不存在，返回 index.html（SPA 路由）
    let index_path = www_dir.join("index.html");
    match tokio::fs::read(&index_path).await {
        Ok(content) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
            content,
        )
            .into_response(),
        Err(_) => (
            StatusCode::NOT_FOUND,
            format!(
                "index.html not found at {:?}. Please build the frontend first.",
                index_path
            ),
        )
            .into_response(),
    }
}

/// 确保 ModemManager 开启 debug 提权模式，但为了防止日志爆炸，
/// 启动后立刻通过 ExecStartPost 将日志级别降回 INFO。
/// 这完美绕过了 Modem.Command 的 Unauthorized 限制，同时保持系统纯净。
fn ensure_modemmanager_debug_override() {
    let override_dir = "/etc/systemd/system/ModemManager.service.d";
    let override_file = "/etc/systemd/system/ModemManager.service.d/99-simadmin-debug.conf";

    let desired_content = "\
# SimAdmin: enable ModemManager debug mode so that Modem.Command D-Bus
# interface is available for AT+CRSM (SIM file read) and AT+CSCA? (SMSC query).
# We immediately set logging level back to INFO via ExecStartPost to prevent log spam.
[Service]
ExecStart=
ExecStart=/usr/sbin/ModemManager --debug
ExecStartPost=-/usr/bin/busctl call org.freedesktop.ModemManager1 /org/freedesktop/ModemManager1 org.freedesktop.ModemManager1 SetLogging s \"INFO\"
";

    let needs_update = match std::fs::read_to_string(override_file) {
        Ok(content) => content != desired_content,
        Err(_) => true,
    };

    if needs_update {
        tracing::info!("Applying ModemManager debug override (with silent logging)...");
        let _ = std::fs::create_dir_all(override_dir);
        if let Err(e) = std::fs::write(override_file, desired_content) {
            tracing::warn!("Failed to write MM debug override: {}", e);
            return;
        }

        // Reload systemd & restart ModemManager silently
        let _ = std::process::Command::new("systemctl")
            .arg("daemon-reload")
            .output();
        let _ = std::process::Command::new("systemctl")
            .args(["restart", "ModemManager.service"])
            .output();
        tracing::info!("ModemManager debug override applied and service restarted.");
    }
}

/// 解压 ZIP 文件到指定目录（供安装脚本使用，无需依赖 unzip / python3）
fn run_extract_zip(archive: &str, target: &str) -> Result<()> {
    use std::io;

    let file = std::fs::File::open(archive)
        .map_err(|e| anyhow::anyhow!("Failed to open {}: {}", archive, e))?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| anyhow::anyhow!("Invalid zip archive {}: {}", archive, e))?;

    let target_dir = std::path::Path::new(target);
    std::fs::create_dir_all(target_dir)
        .map_err(|e| anyhow::anyhow!("Failed to create target dir {}: {}", target, e))?;

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let Some(path) = entry.enclosed_name().map(|p| target_dir.join(p)) else {
            continue;
        };

        if entry.is_dir() {
            std::fs::create_dir_all(&path)?;
            continue;
        }

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut outfile = std::fs::File::create(&path)?;
        io::copy(&mut entry, &mut outfile)?;

        // 保留可执行权限
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = entry.unix_mode() {
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode))?;
            }
        }
    }

    println!("Extracted {} entries to {}", zip.len(), target);
    Ok(())
}

#[derive(Parser, Debug)]
#[command(name = "simadmin")]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<CliCommand>,

    #[command(flatten)]
    serve: ServeArgs,
}

#[derive(Subcommand, Debug)]
enum CliCommand {
    /// 启动 Web 管理服务
    Serve(ServeArgs),
    /// 管理 Web 登录认证（用于 SSH 本机恢复）
    Auth {
        #[command(subcommand)]
        command: AuthCommand,
    },
    /// 解压 ZIP 文件到指定目录（供安装脚本调用）
    ExtractZip {
        /// ZIP 文件路径
        archive: String,
        /// 解压目标目录
        target: String,
    },
}

#[derive(Subcommand, Debug)]
enum AuthCommand {
    /// 交互式重置管理员密码，并清空所有 Web 会话
    ResetPassword,
    /// 清除管理员密码，让 Web UI 下次进入首次设置
    Clear,
}

#[derive(ClapArgs, Debug, Clone)]
struct ServeArgs {
    /// 监听端口 (默认: 3000)
    #[arg(short, long, default_value = "3000", env = "PORT")]
    port: u16,

    /// 监听地址 (默认: ::，双栈监听 IPv4/IPv6)
    #[arg(short = 'H', long, default_value = "::", env = "HOST")]
    host: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化 tracing 日志框架
    // 通过 RUST_LOG 环境变量控制日志级别，默认为 info
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .init();

    // 解析命令行参数
    let cli = Cli::parse();

    // 处理非服务子命令
    if let Some(CliCommand::ExtractZip { archive, target }) = &cli.command {
        return run_extract_zip(archive, target);
    }
    if let Some(CliCommand::Auth { command }) = &cli.command {
        let db = Database::new(get_data_db_path())?;
        let config_manager = ConfigManager::new(get_default_config_path());
        let security = config_manager.get_security();
        return match command {
            AuthCommand::ResetPassword => auth::reset_admin_password_interactive(&db, &security),
            AuthCommand::Clear => auth::clear_admin_auth(&db),
        };
    }

    let args = match cli.command {
        Some(CliCommand::Serve(args)) => args,
        None => cli.serve,
        _ => unreachable!(),
    };
    let bind_addr = display_bind_addr(&args.host, args.port);

    // 确保 ModemManager 已提权以支持 AT 指令读取短信中心
    ensure_modemmanager_debug_override();

    // Connect to system D-Bus
    let dbus_conn = Arc::new(Connection::system().await?);

    // 创建 SMS 数据库（存储在可执行文件同级目录）
    let db_path = get_data_db_path();
    let app_db = Arc::new(Database::new(db_path)?);

    // 初始化配置管理器
    let config_path = get_default_config_path();
    info!(path = ?config_path, "Loading config");
    let config_manager = Arc::new(ConfigManager::new(config_path));
    let data_user_disabled = Arc::new(AtomicBool::new(!config_manager.get_data_enabled()));
    let airplane_mode_requested = Arc::new(AtomicBool::new(false));
    let cell_monitoring_active = Arc::new(AtomicBool::new(false));
    let esim_supervisor = Arc::new(EsimSupervisor::new(Arc::clone(&config_manager)));

    let nm_result = ensure_nm_modem_profile().await;
    tracing::info!(result = %nm_result, "NetworkManager modem profile setup completed");

    // 初始化通知发送器
    let notification_sender = Arc::new(NotificationSender::new(
        Arc::clone(&config_manager),
        Arc::clone(&dbus_conn),
        Arc::clone(&app_db),
    ));
    let system_event_emitter = Arc::new(SystemEventEmitter::new(Arc::clone(&notification_sender)));
    let (sms_resync, sms_resync_rx) = sms_listener::sms_resync_channel();
    let ddns_manager = Arc::new(DdnsManager::new());
    {
        let notification_queue_worker = Arc::clone(&notification_sender);
        tokio::spawn(async move {
            notification_queue_worker.run_queue_worker().await;
        });
    }
    system_event_monitor::spawn_system_event_monitor(
        Arc::clone(&system_event_emitter),
        Arc::clone(&dbus_conn),
    );
    device_status::spawn_device_status_scheduler(
        Arc::clone(&config_manager),
        Arc::clone(&notification_sender),
        Arc::clone(&app_db),
        Arc::clone(&dbus_conn),
        Arc::clone(&ddns_manager),
    );

    {
        let ddns_manager_clone = Arc::clone(&ddns_manager);
        let config_clone = Arc::clone(&config_manager);
        let notification_clone = Arc::clone(&notification_sender);
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
            loop {
                let config = config_clone.get_ddns_config();
                let interval = config.interval_seconds.max(60);
                if config.enabled {
                    if let Err(err) = ddns_manager_clone
                        .sync_now(Arc::clone(&config_clone), Arc::clone(&notification_clone))
                        .await
                    {
                        tracing::warn!(error = %err, "DDNS background sync failed");
                    }
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(interval)).await;
            }
        });
    }

    {
        let config_clone = Arc::clone(&config_manager);
        let notification_clone = Arc::clone(&notification_sender);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(crate::ota::duration_until_next_update_check()).await;
                let config = config_clone.get_version_update_notifications();
                if config.enabled {
                    if let Err(err) = crate::ota::check_and_notify_version_update(
                        Arc::clone(&config_clone),
                        Arc::clone(&notification_clone),
                    )
                    .await
                    {
                        tracing::warn!(error = %err, "Version update notification check failed");
                    }
                }
            }
        });
    }

    // 启动 SMS 监听线程
    {
        let conn_clone = Connection::system().await?;
        let db_clone = Arc::clone(&app_db);
        let notification_clone = Arc::clone(&notification_sender);
        let resync_rx = sms_resync_rx;
        tokio::spawn(async move {
            let _ = sms_listener::start_sms_listener(
                conn_clone,
                db_clone,
                notification_clone,
                resync_rx,
            )
            .await;
        });
    }

    // 电话监听暂不启用

    // 自动初始化数据连接
    {
        let conn_clone = Arc::clone(&dbus_conn);
        let user_off = Arc::clone(&data_user_disabled);
        let cfg = Arc::clone(&config_manager);
        tokio::spawn(async move {
            // 等待 2 秒让 modem 完全初始化
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            let allow_roaming = cfg.get_roaming_allowed();
            let apn_config = cfg.get_apn_config();
            let result = init_data_connection(
                conn_clone.as_ref(),
                user_off.as_ref(),
                allow_roaming,
                Some(apn_config),
            )
            .await;
            tracing::info!(result = %result, "Auto-connect completed");
        });
    }

    // 启动数据连接 Watchdog（每 15 秒检查一次）
    {
        let conn_clone = Arc::clone(&dbus_conn);
        let user_off = Arc::clone(&data_user_disabled);
        let airplane_requested = Arc::clone(&airplane_mode_requested);
        let cfg = Arc::clone(&config_manager);
        let system_events = Arc::clone(&system_event_emitter);
        tokio::spawn(async move {
            // 初始延迟 5 秒，等待系统稳定
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            tracing::info!(interval = 15, "Watchdog started");
            modem_manager::data_connection_watchdog(
                conn_clone,
                15,
                user_off,
                airplane_requested,
                cfg,
                system_events,
            )
            .await;
        });
    }

    system_event_emitter
        .emit_code(
            system_event_codes::SYSTEM_SERVICE_STARTED,
            system_event_severity::INFO,
            system_event_status::SUCCEEDED,
            "simadmin",
            "SimAdmin 服务启动完成",
        )
        .await;

    // CORS 配置：允许前端开发服务器跨域访问
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // 创建统一的应用状态
    spawn_system_stats_sampler(Arc::clone(&dbus_conn));

    let app_state = AppState::new(
        dbus_conn,
        app_db,
        config_manager,
        notification_sender,
        system_event_emitter,
        ddns_manager,
        Arc::clone(&esim_supervisor),
        sms_resync,
        data_user_disabled,
        airplane_mode_requested,
        cell_monitoring_active,
    );

    // 启动自动化中心后台调度引擎
    automation::spawn_automation_scheduler(app_state.clone());

    // Build protected routes - 使用统一的 AppState
    let protected_routes = Router::new()
        // ========== 设备信息接口 ==========
        .route("/api/device", get(get_device_info).options(options_handler))
        // ========== SIM 卡接口 ==========
        .route("/api/sim", get(get_sim_info).options(options_handler))
        .route(
            "/api/sim/details/refresh",
            post(refresh_sim_details_handler).options(options_handler),
        )
        .route(
            "/api/sim/cache",
            post(update_sim_cache_handler).options(options_handler),
        )
        // ========== 网络接口 ==========
        .route(
            "/api/network",
            get(get_network_info).options(options_handler),
        )
        .route("/api/cells", get(get_cells).options(options_handler))
        .route(
            "/api/cell-monitor/start",
            post(start_cell_monitor_handler).options(options_handler),
        )
        .route(
            "/api/cell-monitor/stop",
            post(stop_cell_monitor_handler).options(options_handler),
        )
        .route(
            "/api/radio-mode",
            get(get_radio_mode_handler)
                .post(set_radio_mode_handler)
                .options(options_handler),
        )
        .route(
            "/api/band-lock",
            get(get_band_lock_handler)
                .post(set_band_lock_handler)
                .options(options_handler),
        )
        .route(
            "/api/network/interfaces",
            get(get_network_interfaces_info).options(options_handler),
        )
        .route(
            "/api/network/connection-addresses",
            get(get_network_connection_addresses).options(options_handler),
        )
        .route(
            "/api/device-network/ddns/config",
            get(get_device_ddns_config_handler)
                .post(set_device_ddns_config_handler)
                .options(options_handler),
        )
        .route(
            "/api/device-network/ddns/status",
            get(get_device_ddns_status_handler).options(options_handler),
        )
        .route(
            "/api/device-network/ddns/sync",
            post(sync_device_ddns_handler).options(options_handler),
        )
        .route(
            "/api/device-network/ddns/logs",
            get(get_device_ddns_logs_handler).options(options_handler),
        )
        .route(
            "/api/device-network/ddns/logs/clear",
            post(clear_device_ddns_logs_handler).options(options_handler),
        )
        .route(
            "/api/device-network/wlan/status",
            get(get_device_wlan_status_handler).options(options_handler),
        )
        .route(
            "/api/device-network/wlan/enabled",
            post(set_device_wlan_enabled_handler).options(options_handler),
        )
        .route(
            "/api/device-network/wlan/scan",
            post(scan_device_wlan_handler).options(options_handler),
        )
        .route(
            "/api/device-network/wlan/profiles",
            get(get_device_wlan_profiles_handler).options(options_handler),
        )
        .route(
            "/api/device-network/wlan/forget",
            post(forget_device_wlan_handler).options(options_handler),
        )
        .route(
            "/api/device-network/wlan/connect",
            post(connect_device_wlan_handler).options(options_handler),
        )
        .route(
            "/api/device-network/wlan/disconnect",
            post(disconnect_device_wlan_handler).options(options_handler),
        )
        .route(
            "/api/device-network/wlan/profile",
            post(save_device_wlan_profile_handler).options(options_handler),
        )
        .route(
            "/api/network/signal-strength",
            get(get_signal_strength_handler).options(options_handler),
        )
        .route(
            "/api/location/cell-info",
            get(get_cell_location_handler).options(options_handler),
        )
        .route(
            "/api/network/operators",
            get(get_network_operators).options(options_handler),
        )
        .route(
            "/api/network/operators/scan",
            get(scan_network_operators).options(options_handler),
        )
        .route(
            "/api/network/register-manual",
            post(register_network_manual).options(options_handler),
        )
        .route(
            "/api/network/register-auto",
            post(register_network_auto).options(options_handler),
        )
        .route(
            "/api/apn",
            get(get_apn_list_handler)
                .post(set_apn_handler)
                .options(options_handler),
        )
        .route(
            "/api/cell-lock",
            get(get_cell_lock_status_handler)
                .post(set_cell_lock_handler)
                .options(options_handler),
        )
        .route(
            "/api/cell-lock/unlock-all",
            post(unlock_all_cells_handler).options(options_handler),
        )
        // ========== 数据连接接口 ==========
        .route(
            "/api/data",
            get(get_data_status)
                .post(set_data_status)
                .options(options_handler),
        )
        .route(
            "/api/roaming",
            get(get_roaming_status_handler)
                .post(set_roaming_status_handler)
                .options(options_handler),
        )
        .route(
            "/api/airplane-mode",
            get(get_airplane_mode_handler)
                .post(set_airplane_mode_handler)
                .options(options_handler),
        )
        .route(
            "/api/baseband/restart",
            post(restart_baseband_handler).options(options_handler),
        )
        .route(
            "/api/baseband/restart/status",
            get(get_baseband_restart_status_handler).options(options_handler),
        )
        // ========== 工作模式 / eSIM 管理 ==========
        .route(
            "/api/work-mode",
            get(get_work_mode_handler)
                .post(set_work_mode_handler)
                .options(options_handler),
        )
        .route(
            "/api/esim/config",
            get(get_esim_config_handler)
                .post(set_esim_config_handler)
                .options(options_handler),
        )
        .route(
            "/api/esim/lpac/status",
            get(get_esim_lpac_status_handler).options(options_handler),
        )
        .route(
            "/api/esim/lpac/repair",
            post(repair_esim_lpac_handler).options(options_handler),
        )
        .route(
            "/api/esim/euicc",
            get(get_esim_euicc_handler).options(options_handler),
        )
        .route(
            "/api/esim/profiles",
            get(get_esim_profiles_handler)
                .post(download_esim_profile_handler)
                .options(options_handler),
        )
        .route(
            "/api/esim/profiles/{iccid}/enable",
            post(enable_esim_profile_handler).options(options_handler),
        )
        .route(
            "/api/esim/profiles/{iccid}/rename",
            post(rename_esim_profile_handler).options(options_handler),
        )
        .route(
            "/api/esim/profiles/{iccid}",
            delete(delete_esim_profile_handler).options(options_handler),
        )
        // ========== 电话功能接口 ==========
        .route(
            "/api/calls",
            get(get_calls_handler).options(options_handler),
        )
        .route(
            "/api/call/dial",
            post(dial_call_handler).options(options_handler),
        )
        .route(
            "/api/call/hangup",
            post(hangup_call_handler).options(options_handler),
        )
        .route(
            "/api/call/hangup-all",
            post(hangup_all_calls_handler).options(options_handler),
        )
        .route(
            "/api/call/answer",
            post(answer_call_handler).options(options_handler),
        )
        .route(
            "/api/call/volume",
            get(get_call_volume_handler)
                .post(set_call_volume_handler)
                .options(options_handler),
        )
        .route(
            "/api/call/forwarding",
            get(get_call_forwarding_handler)
                .post(set_call_forwarding_handler)
                .options(options_handler),
        )
        .route(
            "/api/call/settings",
            get(get_call_settings_handler)
                .post(set_call_settings_handler)
                .options(options_handler),
        )
        .route(
            "/api/call/history",
            get(get_call_history_handler).options(options_handler),
        )
        .route(
            "/api/call/history/{id}",
            axum::routing::delete(delete_call_history_handler).options(options_handler),
        )
        .route(
            "/api/call/history/clear",
            post(clear_call_history_handler).options(options_handler),
        )
        .route(
            "/api/ims/status",
            get(get_ims_status_handler).options(options_handler),
        )
        .route(
            "/api/voicemail/status",
            get(get_voicemail_status_handler).options(options_handler),
        )
        // ========== 短信功能接口 ==========
        .route(
            "/api/sms/send",
            post(send_sms_handler).options(options_handler),
        )
        .route(
            "/api/sms/list",
            get(get_sms_list_handler).options(options_handler),
        )
        .route(
            "/api/sms/conversation",
            get(get_sms_conversation_handler).options(options_handler),
        )
        .route(
            "/api/sms/stats",
            get(get_sms_stats_handler).options(options_handler),
        )
        .route(
            "/api/sms/batch-delete",
            post(delete_sms_batch_handler).options(options_handler),
        )
        .route(
            "/api/sms/conversation/{phone_number}",
            axum::routing::delete(delete_sms_conversation_handler).options(options_handler),
        )
        .route(
            "/api/sms/message/{id}",
            axum::routing::delete(delete_sms_message_handler).options(options_handler),
        )
        .route(
            "/api/sms/clear",
            post(clear_sms_handler).options(options_handler),
        )
        // ========== 系统接口 ==========
        .route("/api/stats", get(get_system_stats).options(options_handler))
        .route("/api/stats/cpu", get(get_cpu_info).options(options_handler))
        .route(
            "/api/connectivity",
            get(get_connectivity_check).options(options_handler),
        )
        .route(
            "/api/system/reboot",
            post(system_reboot).options(options_handler),
        )
        .route(
            "/api/service/restart",
            post(restart_service_handler).options(options_handler),
        )
        // ========== 通知配置接口 ==========
        .route(
            "/api/notifications/config",
            get(get_notification_config_handler)
                .post(set_notification_config_handler)
                .options(options_handler),
        )
        .route(
            "/api/notifications/test/{channel}",
            post(test_notification_channel_handler).options(options_handler),
        )
        // ========== OTA 更新接口 ==========
        .route(
            "/api/notifications/logs",
            get(get_notification_logs_handler).options(options_handler),
        )
        .route(
            "/api/notifications/logs/clear",
            post(clear_notification_logs_handler).options(options_handler),
        )
        .route(
            "/api/notifications/queue",
            get(get_notification_queue_handler).options(options_handler),
        )
        .route(
            "/api/notifications/queue/retry-all",
            post(retry_all_notification_queue_handler).options(options_handler),
        )
        .route(
            "/api/notifications/queue/clear",
            post(clear_notification_queue_handler).options(options_handler),
        )
        .route(
            "/api/notifications/queue/{id}",
            delete(delete_notification_queue_item_handler).options(options_handler),
        )
        .route(
            "/api/notifications/queue/{id}/retry",
            post(retry_notification_queue_item_handler).options(options_handler),
        )
        // ========== 自动化中心接口 ==========
        .route(
            "/api/automation/config",
            get(get_automation_config_handler)
                .post(set_automation_config_handler)
                .options(options_handler),
        )
        .route(
            "/api/automation/logs",
            get(get_automation_logs_handler).options(options_handler),
        )
        .route(
            "/api/automation/logs/clear",
            post(clear_automation_logs_handler).options(options_handler),
        )
        .route(
            "/api/automation/test/{task_id}",
            post(test_automation_task_handler).options(options_handler),
        )
        // ========== 备份与恢复接口 ==========
        .route(
            "/api/backup/options",
            get(backup::get_backup_options_handler).options(options_handler),
        )
        .route(
            "/api/backup/config",
            get(backup::get_backup_config_handler)
                .post(backup::set_backup_config_handler)
                .options(options_handler),
        )
        .route(
            "/api/backup/export",
            post(backup::export_backup_handler).options(options_handler),
        )
        .route(
            "/api/backup/export-local",
            post(backup::export_backup_local_handler).options(options_handler),
        )
        .route(
            "/api/backup/import/preview",
            post(backup::preview_backup_import_handler)
                .options(options_handler)
                .layer(DefaultBodyLimit::max(50 * 1024 * 1024)),
        )
        .route(
            "/api/backup/import/apply",
            post(backup::apply_backup_import_handler)
                .options(options_handler)
                .layer(DefaultBodyLimit::max(50 * 1024 * 1024)),
        )
        .route(
            "/api/backup/files",
            get(backup::get_backup_files_handler).options(options_handler),
        )
        .route(
            "/api/backup/files/{filename}/preview",
            get(backup::preview_backup_file_handler).options(options_handler),
        )
        .route(
            "/api/backup/files/{filename}/apply",
            post(backup::apply_backup_file_handler).options(options_handler),
        )
        .route(
            "/api/backup/files/{filename}",
            get(backup::download_backup_file_handler)
                .delete(backup::delete_backup_file_handler)
                .options(options_handler),
        )
        .route(
            "/api/ota/status",
            get(get_ota_status_handler).options(options_handler),
        )
        .route(
            "/api/ota/upload",
            post(upload_ota_handler)
                .options(options_handler)
                .layer(DefaultBodyLimit::max(50 * 1024 * 1024)),
        )
        .route(
            "/api/ota/latest-release",
            post(get_latest_ota_release_handler).options(options_handler),
        )
        .route(
            "/api/ota/online-prepare",
            post(prepare_online_ota_handler).options(options_handler),
        )
        .route(
            "/api/ota/apply",
            post(apply_ota_handler).options(options_handler),
        )
        .route(
            "/api/ota/cancel",
            post(cancel_ota_handler).options(options_handler),
        )
        .route(
            "/api/auth/password",
            post(auth::change_password).options(options_handler),
        )
        .route(
            "/api/auth/settings",
            get(auth::get_settings)
                .post(auth::set_settings)
                .options(options_handler),
        )
        .route_layer(middleware::from_fn_with_state(
            app_state.clone(),
            auth::auth_middleware,
        ));

    let app = Router::new()
        .route("/api/health", get(health_check).options(options_handler))
        .route(
            "/api/auth/status",
            get(auth::status).options(options_handler),
        )
        .route(
            "/api/auth/setup",
            post(auth::setup).options(options_handler),
        )
        .route(
            "/api/auth/login",
            post(auth::login).options(options_handler),
        )
        .route(
            "/api/auth/logout",
            post(auth::logout).options(options_handler),
        )
        .merge(protected_routes)
        .with_state(app_state)
        .layer(cors)
        .fallback(spa_fallback);

    // Start server - 显示版权信息
    info!(
        version = env!("APP_VERSION"),
        branch = env!("GIT_BRANCH"),
        commit = env!("GIT_COMMIT"),
        "SimAdmin - Debian SIM Management Service"
    );
    info!("Copyright © 2026 GitHub 3899 - SimAdmin");

    // 绑定端口，如果被占用则轮询等待（最多 30 秒）
    let listener = bind_with_retry(&args.host, args.port, 30).await?;
    info!(addr = %bind_addr, "Server listening");
    // 使用优雅关闭
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

/// 绑定端口，如果被占用则轮询等待
fn display_bind_addr(host: &str, port: u16) -> String {
    let host = host.trim_matches(|c| c == '[' || c == ']');
    if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

async fn bind_listener(host: &str, port: u16) -> std::io::Result<tokio::net::TcpListener> {
    let normalized_host = host.trim_matches(|c| c == '[' || c == ']');
    if normalized_host == "::" {
        let socket = Socket::new(Domain::IPV6, Type::STREAM, Some(Protocol::TCP))?;
        socket.set_reuse_address(true)?;
        socket.set_only_v6(false)?;
        socket.bind(&SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), port).into())?;
        socket.listen(1024)?;
        socket.set_nonblocking(true)?;
        let listener: std::net::TcpListener = socket.into();
        return tokio::net::TcpListener::from_std(listener);
    }

    tokio::net::TcpListener::bind((normalized_host, port)).await
}

async fn bind_with_retry(
    host: &str,
    port: u16,
    max_retries: u32,
) -> Result<tokio::net::TcpListener> {
    use std::time::Duration;
    let addr = display_bind_addr(host, port);

    for i in 0..max_retries {
        match bind_listener(host, port).await {
            Ok(listener) => return Ok(listener),
            Err(e) => {
                if i == 0 {
                    warn!(addr = %addr, "Port busy, waiting for release...");
                }
                if i + 1 < max_retries {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                } else {
                    return Err(anyhow::anyhow!("Failed to bind to {}: {}", addr, e));
                }
            }
        }
    }
    unreachable!()
}

/// 监听 Ctrl+C 和 SIGTERM 信号，用于优雅关闭
async fn shutdown_signal() {
    use tokio::signal;

    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C signal handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
