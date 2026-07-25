//! API 处理器模块 (ModemManager 版)
//!
//! 包含所有 HTTP API 的处理函数

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::process::{Command, Output};
use std::sync::atomic::Ordering;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use zbus::Connection;

use crate::{
    config::ApnConfig,
    esim::EsimApiError,
    models::*,
    modem_manager::{
        self, answer_call, apply_roaming_policy, current_sim_identity,
        find_nm_modem_connection_pub, get_airplane_mode, get_band_lock_status,
        get_baseband_restart_progress, get_call_by_path, get_call_settings, get_cell_location,
        get_cells_data, get_data_connection_status, get_device_info_data, get_is_roaming_mm,
        get_network_info_data, get_operators_list, get_radio_mode, get_signal_strength,
        get_sim_info_data_with_cache, hangup_all_calls, hangup_call, list_apn_contexts,
        list_current_calls, make_call, nm_set_autoconnect_pub, power_cycle_sim_for_profile_switch,
        refresh_sim_details_background, register_operator_auto, register_operator_manual,
        restart_baseband, scan_operators, send_sms, set_airplane_mode, set_apn_on_bearer,
        set_band_lock, set_call_waiting, set_data_connection_with_apn, set_radio_mode,
        sim_details_cache_missing, start_cell_monitoring, stop_cell_monitoring,
    },
    state::AppState,
    system_event::{
        codes as system_event_codes, mask_identifier, severity as system_event_severity,
        status as system_event_status,
    },
    utils::{
        connection_addresses_from_interfaces, format_uptime, get_active_interfaces, read_cpu_info,
        read_cpu_load_sync, read_disk_info, read_interface_stats, read_memory_info,
        read_network_interfaces, read_system_info, read_uptime, sample_cpu_usage,
    },
};

const ESIM_SIM_IDENTITY_TIMEOUT_SECS: u64 = 3;
const ESIM_CACHED_SIM_IDENTITY_TIMEOUT_MS: u64 = 800;
const SMS_DB_MAINTENANCE_DELETE_THRESHOLD: usize = 100;
const SMS_DB_MAINTENANCE_DELAY_SECS: u64 = 60;

// ============ 基础接口 ============

/// 处理 OPTIONS 请求（CORS 预检）
pub async fn options_handler() -> impl IntoResponse {
    StatusCode::NO_CONTENT
}

/// GET /api/health
pub async fn health_check() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(json!({
            "status": "ok",
            "message": "Service is running",
            "platform": "linux-modem",
            "version": env!("CARGO_PKG_VERSION"),
        })),
    )
}

fn esim_error_response<T: Default>(error: EsimApiError) -> (StatusCode, Json<ApiResponse<T>>) {
    let status = match error {
        EsimApiError::Disabled => StatusCode::FORBIDDEN,
        EsimApiError::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
        EsimApiError::Command(_) => StatusCode::OK,
    };
    (status, Json(ApiResponse::<T>::error(error.message())))
}

fn esim_command_succeeded(response: &EsimCommandResponse) -> bool {
    response.code == 0
        && (response.status.is_empty()
            || response.status.eq_ignore_ascii_case("success")
            || response.status.eq_ignore_ascii_case("ok"))
}

fn esim_command_failure(action: &str, message: impl Into<String>) -> EsimCommandResponse {
    EsimCommandResponse {
        code: 1,
        status: "error".to_string(),
        action: action.to_string(),
        msg: message.into(),
        data: None,
    }
}

fn esim_enable_success(message: impl Into<String>) -> EsimCommandResponse {
    EsimCommandResponse {
        code: 0,
        status: "ok".to_string(),
        action: "enable".to_string(),
        msg: message.into(),
        data: None,
    }
}

fn esim_profile_is_active(profile: &EsimProfile) -> bool {
    matches!(
        profile.state.trim().to_ascii_lowercase().as_str(),
        "enabled" | "active" | "1" | "true"
    )
}

fn esim_profile_matches_iccid(profile: &EsimProfile, normalized_iccid: &str) -> bool {
    !normalized_iccid.is_empty()
        && crate::utils::normalize_iccid(&profile.iccid) == normalized_iccid
}

fn esim_enable_failure_is_retryable(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    [
        "es10c_enable_profile",
        "apdu",
        "busy",
        "catbusy",
        "cat_busy",
        "logical channel",
        "uim",
        "qmi",
        "refresh",
        "timeout",
        "timed out",
    ]
    .iter()
    .any(|marker| message.contains(marker))
}

fn esim_profile_state_is_unknown(state: &str) -> bool {
    let state = state.trim();
    state.is_empty() || state.eq_ignore_ascii_case("unknown")
}

fn sort_esim_profiles_for_display(profiles: &mut [EsimProfile]) {
    profiles.sort_by(|left, right| {
        let left_active = esim_profile_is_active(left);
        let right_active = esim_profile_is_active(right);
        right_active
            .cmp(&left_active)
            .then_with(|| {
                left.name
                    .to_ascii_lowercase()
                    .cmp(&right.name.to_ascii_lowercase())
            })
            .then_with(|| left.iccid.cmp(&right.iccid))
    });
}

fn split_profile_operator_code(code: &str) -> (String, String) {
    let digits: String = code.chars().filter(|ch| ch.is_ascii_digit()).collect();
    if digits.len() >= 6 {
        (digits[..3].to_string(), digits[3..6].to_string())
    } else if digits.len() >= 5 {
        (digits[..3].to_string(), digits[3..].to_string())
    } else {
        (String::new(), String::new())
    }
}

fn enrich_profiles_with_current_identity(
    profiles: &mut [EsimProfile],
    identity: &crate::modem_manager::SimIdentity,
) {
    let current_index = profiles
        .iter()
        .position(|profile| !identity.iccid.is_empty() && profile.iccid == identity.iccid)
        .or_else(|| profiles.iter().position(esim_profile_is_active));

    let Some(profile) = current_index.and_then(|index| profiles.get_mut(index)) else {
        return;
    };

    if esim_profile_state_is_unknown(&profile.state)
        || !identity.iccid.is_empty() && profile.iccid == identity.iccid
    {
        profile.state = "enabled".to_string();
    }
    if profile.imsi.is_none() && !identity.imsi.is_empty() {
        profile.imsi = Some(identity.imsi.clone());
    }
    let (mcc, mnc) = split_profile_operator_code(&identity.operator_id);
    if profile.mcc.is_none() && !mcc.is_empty() {
        profile.mcc = Some(mcc);
    }
    if profile.mnc.is_none() && !mnc.is_empty() {
        profile.mnc = Some(mnc);
    }

    if !identity.iccid.is_empty() {
        for item in profiles {
            if item.iccid != identity.iccid && esim_profile_state_is_unknown(&item.state) {
                item.state = "disabled".to_string();
            }
        }
    }
}

fn profile_cache_value(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn optional_profile_cache_value(value: &Option<String>) -> Option<String> {
    value.as_deref().and_then(profile_cache_value)
}

fn profile_cache_entry(profile: &EsimProfile) -> EsimProfileCacheEntry {
    EsimProfileCacheEntry {
        iccid: profile.iccid.trim().to_string(),
        name: profile_cache_value(&profile.name),
        provider: profile_cache_value(&profile.provider),
        state: profile_cache_value(&profile.state),
        profile_class: profile_cache_value(&profile.profile_class),
        imsi: optional_profile_cache_value(&profile.imsi),
        msisdn: optional_profile_cache_value(&profile.msisdn),
        smsc: optional_profile_cache_value(&profile.smsc),
        smdp: optional_profile_cache_value(&profile.smdp),
        matching_id: optional_profile_cache_value(&profile.matching_id),
        isdp_aid: optional_profile_cache_value(&profile.isdp_aid),
        mcc: optional_profile_cache_value(&profile.mcc),
        mnc: optional_profile_cache_value(&profile.mnc),
        disable_allowed: profile.disable_allowed,
        delete_allowed: profile.delete_allowed,
        updated_at: String::new(),
    }
}

fn fill_cached_string(target: &mut String, cached: Option<String>) {
    if target.trim().is_empty() {
        if let Some(value) = cached.and_then(|item| profile_cache_value(&item)) {
            *target = value;
        }
    }
}

fn fill_cached_option(target: &mut Option<String>, cached: Option<String>) {
    if target.as_deref().unwrap_or("").trim().is_empty() {
        if let Some(value) = cached.and_then(|item| profile_cache_value(&item)) {
            *target = Some(value);
        }
    }
}

fn hydrate_profile_from_cache(db: &Database, profile: &mut EsimProfile) {
    let cache = match db.get_esim_profile_cache(&profile.iccid) {
        Ok(Some(cache)) => cache,
        Ok(None) => return,
        Err(err) => {
            warn!(iccid = %profile.iccid, error = %err, "Failed to read eSIM profile cache");
            return;
        }
    };

    fill_cached_string(&mut profile.name, cache.name);
    fill_cached_string(&mut profile.provider, cache.provider);
    fill_cached_string(&mut profile.state, cache.state);
    fill_cached_string(&mut profile.profile_class, cache.profile_class);
    fill_cached_option(&mut profile.imsi, cache.imsi);
    fill_cached_option(&mut profile.msisdn, cache.msisdn);
    fill_cached_option(&mut profile.smsc, cache.smsc);
    fill_cached_option(&mut profile.smdp, cache.smdp);
    fill_cached_option(&mut profile.matching_id, cache.matching_id);
    fill_cached_option(&mut profile.isdp_aid, cache.isdp_aid);
    fill_cached_option(&mut profile.mcc, cache.mcc);
    fill_cached_option(&mut profile.mnc, cache.mnc);
}

fn hydrate_profiles_from_cache(db: &Database, profiles: &mut [EsimProfile]) {
    for profile in profiles {
        hydrate_profile_from_cache(db, profile);
    }
}

fn cache_esim_profiles(db: &Database, profiles: &[EsimProfile]) {
    for profile in profiles {
        if let Err(err) = db.upsert_esim_profile_cache(&profile_cache_entry(profile)) {
            warn!(iccid = %profile.iccid, error = %err, "Failed to write eSIM profile cache");
        }
    }
}

fn profile_from_cache_entry(entry: EsimProfileCacheEntry) -> EsimProfile {
    EsimProfile {
        iccid: entry.iccid,
        name: entry.name.unwrap_or_default(),
        provider: entry.provider.unwrap_or_default(),
        state: entry.state.unwrap_or_else(|| "unknown".to_string()),
        profile_class: entry.profile_class.unwrap_or_default(),
        imsi: entry.imsi,
        msisdn: entry.msisdn,
        smsc: entry.smsc,
        smdp: entry.smdp,
        matching_id: entry.matching_id,
        isdp_aid: entry.isdp_aid,
        mcc: entry.mcc,
        mnc: entry.mnc,
        disable_allowed: entry.disable_allowed.or(Some(true)),
        delete_allowed: entry.delete_allowed.or(Some(true)),
        updated_at: Some(entry.updated_at.clone()),
        raw: json!({
            "source": "cache",
            "updated_at": entry.updated_at,
        }),
    }
}

fn cached_profiles_requested(query: &std::collections::HashMap<String, String>) -> bool {
    query
        .get("cached")
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false)
}

// ============ 工作模式 / eSIM ============

fn live_refresh_requested(query: &std::collections::HashMap<String, String>) -> bool {
    query
        .get("live")
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false)
}

enum EsimProfileEnableOutcome {
    Enabled(EsimCommandResponse),
    AlreadyEnabled(EsimCommandResponse),
    Failed(EsimCommandResponse),
}

async fn refresh_profile_for_switch(
    app: &AppState,
    normalized_iccid: &str,
) -> Result<Option<EsimProfile>, EsimApiError> {
    let response = app.esim_supervisor.get_profiles_for_switch().await?;
    Ok(response
        .profiles
        .into_iter()
        .find(|profile| esim_profile_matches_iccid(profile, normalized_iccid)))
}

async fn retry_enable_profile_after_refresh(
    app: &AppState,
    iccid: &str,
    normalized_iccid: &str,
    first_message: String,
) -> Result<EsimProfileEnableOutcome, EsimApiError> {
    modem_manager::record_restart_step(
        "检查 eUICC 切卡状态",
        "running",
        Some("首次启用未确认成功，短暂刷新后自动重试".to_string()),
    );
    tokio::time::sleep(Duration::from_millis(800)).await;

    match refresh_profile_for_switch(app, normalized_iccid).await {
        Ok(Some(profile)) if esim_profile_is_active(&profile) => {
            modem_manager::record_restart_step(
                "检查 eUICC 切卡状态",
                "ok",
                Some("刷新后已检测到目标 Profile 生效".to_string()),
            );
            return Ok(EsimProfileEnableOutcome::Enabled(esim_enable_success(
                "Profile enabled after status refresh",
            )));
        }
        Ok(Some(_)) => modem_manager::record_restart_step(
            "检查 eUICC 切卡状态",
            "ok",
            Some("目标 Profile 仍未启用，使用兼容参数重试".to_string()),
        ),
        Ok(None) => modem_manager::record_restart_step(
            "检查 eUICC 切卡状态",
            "warning",
            Some("刷新后暂未返回目标 Profile，继续尝试兼容重试".to_string()),
        ),
        Err(err) => modem_manager::record_restart_step(
            "检查 eUICC 切卡状态",
            "warning",
            Some(format!("刷新失败，继续尝试兼容重试: {}", err.message())),
        ),
    }

    modem_manager::record_restart_step(
        "重试启用 eSIM Profile",
        "running",
        Some("使用 refreshFlag=0 兼容部分基带 REFRESH 处理异常".to_string()),
    );
    match app
        .esim_supervisor
        .enable_profile_with_refresh_flag(iccid.to_string(), false)
        .await
    {
        Ok(data) if esim_command_succeeded(&data) => {
            modem_manager::record_restart_step("重试启用 eSIM Profile", "ok", None);
            Ok(EsimProfileEnableOutcome::Enabled(data))
        }
        Ok(mut data) => {
            data.msg = if data.msg.is_empty() {
                format!("{first_message}; retry failed")
            } else {
                format!("{first_message}; retry failed: {}", data.msg)
            };
            modem_manager::record_restart_step(
                "重试启用 eSIM Profile",
                "error",
                Some(data.msg.clone()),
            );
            Ok(EsimProfileEnableOutcome::Failed(data))
        }
        Err(err) => {
            let message = format!("{first_message}; retry failed: {}", err.message());
            modem_manager::record_restart_step(
                "重试启用 eSIM Profile",
                "error",
                Some(message.clone()),
            );
            Err(EsimApiError::Command(message))
        }
    }
}

async fn enable_esim_profile_for_switch(
    app: &AppState,
    iccid: &str,
) -> Result<EsimProfileEnableOutcome, EsimApiError> {
    let normalized_iccid = crate::utils::normalize_iccid(iccid);
    if normalized_iccid.is_empty() {
        let message = "Profile ICCID is empty".to_string();
        modem_manager::record_restart_step(
            "同步 eUICC Profile 状态",
            "error",
            Some(message.clone()),
        );
        return Ok(EsimProfileEnableOutcome::Failed(esim_command_failure(
            "enable", message,
        )));
    }

    modem_manager::record_restart_step("同步 eUICC Profile 状态", "running", None);
    match refresh_profile_for_switch(app, &normalized_iccid).await {
        Ok(Some(profile)) if esim_profile_is_active(&profile) => {
            modem_manager::record_restart_step(
                "同步 eUICC Profile 状态",
                "ok",
                Some("目标 Profile 已是启用状态".to_string()),
            );
            return Ok(EsimProfileEnableOutcome::AlreadyEnabled(
                esim_enable_success("Profile already enabled"),
            ));
        }
        Ok(Some(_)) => modem_manager::record_restart_step(
            "同步 eUICC Profile 状态",
            "ok",
            Some("目标 Profile 已确认，开始切换".to_string()),
        ),
        Ok(None) => {
            let message = "目标 Profile 未在 eUICC 芯片中找到，请刷新列表后重试".to_string();
            modem_manager::record_restart_step(
                "同步 eUICC Profile 状态",
                "error",
                Some(message.clone()),
            );
            return Ok(EsimProfileEnableOutcome::Failed(esim_command_failure(
                "enable", message,
            )));
        }
        Err(err) => modem_manager::record_restart_step(
            "同步 eUICC Profile 状态",
            "warning",
            Some(format!("预刷新失败，继续尝试切卡: {}", err.message())),
        ),
    }

    match app.esim_supervisor.enable_profile(iccid.to_string()).await {
        Ok(data) if esim_command_succeeded(&data) => Ok(EsimProfileEnableOutcome::Enabled(data)),
        Ok(data) if esim_enable_failure_is_retryable(&data.msg) => {
            retry_enable_profile_after_refresh(app, iccid, &normalized_iccid, data.msg.clone())
                .await
        }
        Ok(data) => Ok(EsimProfileEnableOutcome::Failed(data)),
        Err(err) if esim_enable_failure_is_retryable(&err.message()) => {
            retry_enable_profile_after_refresh(app, iccid, &normalized_iccid, err.message()).await
        }
        Err(err) => Err(err),
    }
}

fn euicc_cache_key(info: &EsimEuiccInfo) -> String {
    let eid = info.eid.trim();
    if eid.is_empty() {
        "default".to_string()
    } else {
        format!("eid:{eid}")
    }
}

fn euicc_cache_entry(info: &EsimEuiccInfo) -> EsimEuiccCacheEntry {
    EsimEuiccCacheEntry {
        cache_key: euicc_cache_key(info),
        eid: info.eid.clone(),
        status: info.status.clone(),
        manufacturer: info.manufacturer.clone(),
        memory_total_kb: info.memory_total_kb,
        memory_available_kb: info.memory_available_kb,
        memory_total_customizable: info.memory_total_customizable,
        raw: info.raw.to_string(),
        updated_at: info.updated_at.clone().unwrap_or_default(),
    }
}

fn euicc_from_cache_entry(entry: EsimEuiccCacheEntry) -> EsimEuiccInfo {
    let mut raw: serde_json::Value = serde_json::from_str(&entry.raw).unwrap_or_else(|_| json!({}));
    if let Some(object) = raw.as_object_mut() {
        object.insert("source".to_string(), json!("cache"));
        object.insert("updated_at".to_string(), json!(entry.updated_at.clone()));
    } else {
        raw = json!({
            "source": "cache",
            "updated_at": entry.updated_at,
        });
    }

    EsimEuiccInfo {
        eid: entry.eid,
        status: entry.status,
        manufacturer: entry.manufacturer,
        memory_total_kb: entry.memory_total_kb,
        memory_available_kb: entry.memory_available_kb,
        memory_total_customizable: entry.memory_total_customizable,
        updated_at: Some(entry.updated_at),
        raw,
    }
}

/// GET /api/work-mode
pub async fn get_work_mode_handler(State(app): State<AppState>) -> impl IntoResponse {
    let mode = app.config_manager.get_work_mode();
    let worker_running = app.esim_supervisor.worker_running().await;
    (
        StatusCode::OK,
        Json(ApiResponse::success_with_message(
            "Success",
            WorkModeResponse {
                mode,
                worker_running,
            },
        )),
    )
}

/// POST /api/work-mode
pub async fn set_work_mode_handler(
    State(app): State<AppState>,
    Json(payload): Json<WorkModeRequest>,
) -> impl IntoResponse {
    if !payload.confirm {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<WorkModeResponse>::error(
                "Changing work mode requires confirm=true",
            )),
        );
    }

    let previous_mode = app.config_manager.get_work_mode();
    match app.esim_supervisor.switch_mode(payload.mode).await {
        Ok(data) => {
            if previous_mode != data.mode {
                app.system_event_emitter
                    .emit_code(
                        system_event_codes::ESIM_WORK_MODE_CHANGED,
                        system_event_severity::INFO,
                        system_event_status::CHANGED,
                        "work_mode",
                        format!("工作模式从 {} 切换为 {}", previous_mode, data.mode),
                    )
                    .await;
            }
            (
                StatusCode::OK,
                Json(ApiResponse::success_with_message("Work mode updated", data)),
            )
        }
        Err(err) => (
            StatusCode::OK,
            Json(ApiResponse::<WorkModeResponse>::error(err)),
        ),
    }
}

/// GET /api/esim/lpac/status
pub async fn get_esim_lpac_status_handler(State(app): State<AppState>) -> impl IntoResponse {
    match app.esim_supervisor.get_lpac_status().await {
        Ok(data) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message("Success", data)),
        ),
        Err(err) => esim_error_response::<EsimLpacStatusResponse>(err),
    }
}

/// POST /api/esim/lpac/repair
pub async fn repair_esim_lpac_handler(
    State(app): State<AppState>,
    Json(payload): Json<EsimLpacRepairRequest>,
) -> impl IntoResponse {
    match app.esim_supervisor.repair_lpac(payload).await {
        Ok(data) => {
            app.system_event_emitter
                .emit_code(
                    system_event_codes::ESIM_LPAC_REPAIR_SUCCEEDED,
                    system_event_severity::INFO,
                    system_event_status::SUCCEEDED,
                    "lpac",
                    "lpac 修复成功",
                )
                .await;
            (
                StatusCode::OK,
                Json(ApiResponse::success_with_message("lpac repaired", data)),
            )
        }
        Err(err) => {
            let message = err.message();
            app.system_event_emitter
                .emit_code(
                    system_event_codes::ESIM_LPAC_REPAIR_FAILED,
                    system_event_severity::WARNING,
                    system_event_status::FAILED,
                    "lpac",
                    format!("lpac 修复失败: {message}"),
                )
                .await;
            esim_error_response::<EsimLpacRepairResponse>(err)
        }
    }
}

/// GET /api/esim/config
pub async fn get_esim_config_handler(State(app): State<AppState>) -> impl IntoResponse {
    let esim_config = app.config_manager.get_esim_config();
    (
        StatusCode::OK,
        Json(ApiResponse::success_with_message("Success", esim_config)),
    )
}

/// POST /api/esim/config
pub async fn set_esim_config_handler(
    State(app): State<AppState>,
    Json(payload): Json<crate::config::EsimConfig>,
) -> impl IntoResponse {
    match app.config_manager.set_esim_config(payload) {
        Ok(_) => (
            StatusCode::OK,
            Json(ApiResponse::<()>::success_with_message(
                "eSIM config updated successfully",
                (),
            )),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<()>::error(err)),
        ),
    }
}

/// GET /api/esim/euicc
pub async fn get_esim_euicc_handler(
    State(app): State<AppState>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    if !live_refresh_requested(&query) {
        match app.database.latest_esim_euicc_cache() {
            Ok(Some(entry)) => {
                return (
                    StatusCode::OK,
                    Json(ApiResponse::success_with_message(
                        "Cached eUICC",
                        euicc_from_cache_entry(entry),
                    )),
                );
            }
            Ok(None) => {}
            Err(err) => warn!(error = %err, "Failed to read eUICC cache"),
        }
    }

    match app.esim_supervisor.get_euicc_info().await {
        Ok(mut data) => {
            data.updated_at = Some(chrono::Utc::now().to_rfc3339());
            if let Err(err) = app
                .database
                .upsert_esim_euicc_cache(&euicc_cache_entry(&data))
            {
                warn!(error = %err, "Failed to write eUICC cache");
            }
            (
                StatusCode::OK,
                Json(ApiResponse::success_with_message("Success", data)),
            )
        }
        Err(err) => esim_error_response::<EsimEuiccInfo>(err),
    }
}

/// GET /api/esim/profiles
pub async fn get_esim_profiles_handler(
    State(app): State<AppState>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    if cached_profiles_requested(&query) {
        return match app.database.list_esim_profile_cache() {
            Ok(entries) => {
                let mut profiles: Vec<EsimProfile> =
                    entries.into_iter().map(profile_from_cache_entry).collect();
                let needs_identity = profiles
                    .iter()
                    .any(|profile| esim_profile_state_is_unknown(&profile.state));
                if needs_identity {
                    match tokio::time::timeout(
                        std::time::Duration::from_millis(ESIM_CACHED_SIM_IDENTITY_TIMEOUT_MS),
                        current_sim_identity(&app.dbus_conn),
                    )
                    .await
                    {
                        Ok(Some(identity)) => {
                            enrich_profiles_with_current_identity(&mut profiles, &identity)
                        }
                        Ok(None) => {}
                        Err(_) => warn!(
                            timeout_ms = ESIM_CACHED_SIM_IDENTITY_TIMEOUT_MS,
                            "Timed out enriching cached eSIM profiles with current SIM identity"
                        ),
                    }
                }
                sort_esim_profiles_for_display(&mut profiles);
                (
                    StatusCode::OK,
                    Json(ApiResponse::success_with_message(
                        "Cached profiles",
                        EsimProfilesResponse { profiles },
                    )),
                )
            }
            Err(err) => (
                StatusCode::OK,
                Json(ApiResponse::<EsimProfilesResponse>::error(format!(
                    "Failed to read cached profiles: {err}"
                ))),
            ),
        };
    }

    match app.esim_supervisor.get_profiles().await {
        Ok(mut data) => {
            hydrate_profiles_from_cache(&app.database, &mut data.profiles);
            match tokio::time::timeout(
                std::time::Duration::from_secs(ESIM_SIM_IDENTITY_TIMEOUT_SECS),
                current_sim_identity(&app.dbus_conn),
            )
            .await
            {
                Ok(Some(identity)) => {
                    enrich_profiles_with_current_identity(&mut data.profiles, &identity)
                }
                Ok(None) => {}
                Err(_) => warn!(
                    timeout_secs = ESIM_SIM_IDENTITY_TIMEOUT_SECS,
                    "Timed out enriching eSIM profiles with current SIM identity"
                ),
            }
            cache_esim_profiles(&app.database, &data.profiles);
            sort_esim_profiles_for_display(&mut data.profiles);
            (
                StatusCode::OK,
                Json(ApiResponse::success_with_message("Success", data)),
            )
        }
        Err(err) => esim_error_response::<EsimProfilesResponse>(err),
    }
}

/// POST /api/esim/profiles/{iccid}/enable
pub async fn enable_esim_profile_handler(
    State(app): State<AppState>,
    Path(iccid): Path<String>,
) -> impl IntoResponse {
    let event_entity = mask_identifier(&iccid);

    modem_manager::reset_baseband_restart_progress();
    modem_manager::record_restart_step("启用 eSIM Profile", "running", None);

    let bg_app = app.clone();
    let bg_iccid = iccid.clone();
    let bg_event_entity = event_entity.clone();

    tokio::spawn(async move {
        let _guard = modem_manager::BasebandRestartRunGuard;

        match enable_esim_profile_for_switch(&bg_app, &bg_iccid).await {
            Ok(EsimProfileEnableOutcome::Enabled(data)) => {
                if esim_command_succeeded(&data) {
                    modem_manager::record_restart_step("启用 eSIM Profile", "ok", None);
                    let auto_connect_data = !bg_app.data_user_disabled.load(Ordering::SeqCst);
                    let allow_roaming = bg_app.config_manager.get_roaming_allowed();
                    let apn_config = bg_app.config_manager.get_apn_config();
                    match power_cycle_sim_for_profile_switch(
                        &bg_app.dbus_conn,
                        auto_connect_data,
                        allow_roaming,
                        Some(apn_config),
                    )
                    .await
                    {
                        Ok(_recovery) => {
                            if bg_app.sms_resync.request_scan("profile-switch") {
                                info!("Requested SMS resync after eSIM profile switch");
                            } else {
                                warn!("Failed to request SMS resync after eSIM profile switch");
                            }
                            bg_app
                                .system_event_emitter
                                .emit_code(
                                    system_event_codes::ESIM_PROFILE_ENABLE_SUCCEEDED,
                                    system_event_severity::INFO,
                                    system_event_status::SUCCEEDED,
                                    bg_event_entity,
                                    "Profile 启用成功，基带恢复完成",
                                )
                                .await;
                        }
                        Err(err) => {
                            bg_app
                                .system_event_emitter
                                .emit_code(
                                    system_event_codes::ESIM_PROFILE_SWITCH_BASEBAND_RECOVERY_FAILED,
                                    system_event_severity::CRITICAL,
                                    system_event_status::FAILED,
                                    bg_event_entity,
                                    format!("Profile 切换后基带恢复失败: {err}"),
                                )
                                .await;
                            if bg_app
                                .sms_resync
                                .request_scan("profile-switch-recovery-failed")
                            {
                                info!("Requested SMS resync after failed eSIM profile recovery");
                            } else {
                                warn!(
                                    "Failed to request SMS resync after failed eSIM profile recovery"
                                );
                            }
                        }
                    }
                } else {
                    modem_manager::record_restart_step(
                        "启用 eSIM Profile",
                        "error",
                        Some(data.msg.clone()),
                    );
                    bg_app
                        .system_event_emitter
                        .emit_code(
                            system_event_codes::ESIM_PROFILE_ENABLE_FAILED,
                            system_event_severity::WARNING,
                            system_event_status::FAILED,
                            bg_event_entity.clone(),
                            format!("Profile 启用失败: {}", data.msg),
                        )
                        .await;
                }
            }
            Ok(EsimProfileEnableOutcome::AlreadyEnabled(data)) => {
                modem_manager::record_restart_step(
                    "启用 eSIM Profile",
                    "ok",
                    Some(data.msg.clone()),
                );
                bg_app
                    .system_event_emitter
                    .emit_code(
                        system_event_codes::ESIM_PROFILE_ENABLE_SUCCEEDED,
                        system_event_severity::INFO,
                        system_event_status::SUCCEEDED,
                        bg_event_entity.clone(),
                        "Profile 已是启用状态，无需重复切换",
                    )
                    .await;
            }
            Ok(EsimProfileEnableOutcome::Failed(data)) => {
                modem_manager::record_restart_step(
                    "启用 eSIM Profile",
                    "error",
                    Some(data.msg.clone()),
                );
                bg_app
                    .system_event_emitter
                    .emit_code(
                        system_event_codes::ESIM_PROFILE_ENABLE_FAILED,
                        system_event_severity::WARNING,
                        system_event_status::FAILED,
                        bg_event_entity.clone(),
                        format!("Profile 启用失败: {}", data.msg),
                    )
                    .await;
            }
            Err(err) => {
                let message = err.message();
                modem_manager::record_restart_step(
                    "启用 eSIM Profile",
                    "error",
                    Some(message.clone()),
                );
                bg_app
                    .system_event_emitter
                    .emit_code(
                        system_event_codes::ESIM_PROFILE_ENABLE_FAILED,
                        system_event_severity::WARNING,
                        system_event_status::FAILED,
                        bg_event_entity.clone(),
                        format!("Profile 启用失败: {message}"),
                    )
                    .await;
            }
        }
    });

    let response = EsimCommandResponse {
        code: 0,
        status: "success".to_string(),
        action: "enable".to_string(),
        msg: "Profile enable task started in background".to_string(),
        data: None,
    };

    (
        StatusCode::OK,
        Json(ApiResponse::success_with_message(
            "Profile enable requested",
            response,
        )),
    )
}

/// POST /api/esim/profiles/{iccid}/rename
pub async fn rename_esim_profile_handler(
    State(app): State<AppState>,
    Path(iccid): Path<String>,
    Json(payload): Json<EsimRenameRequest>,
) -> impl IntoResponse {
    let name = payload.name.trim().to_string();
    if name.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<EsimCommandResponse>::error(
                "Profile name cannot be empty",
            )),
        );
    }
    match app.esim_supervisor.rename_profile(iccid, name).await {
        Ok(data) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message("Profile renamed", data)),
        ),
        Err(err) => esim_error_response::<EsimCommandResponse>(err),
    }
}

/// DELETE /api/esim/profiles/{iccid}
pub async fn delete_esim_profile_handler(
    State(app): State<AppState>,
    Path(iccid): Path<String>,
) -> impl IntoResponse {
    match app.esim_supervisor.delete_profile(iccid.clone()).await {
        Ok(data) => {
            if esim_command_succeeded(&data) {
                if let Err(err) = app.database.delete_esim_profile_cache(&iccid) {
                    warn!(iccid = %iccid, error = %err, "Failed to delete eSIM profile cache");
                }
                app.system_event_emitter
                    .emit_code(
                        system_event_codes::ESIM_PROFILE_DELETED,
                        system_event_severity::WARNING,
                        system_event_status::SUCCEEDED,
                        mask_identifier(&iccid),
                        "Profile 已删除",
                    )
                    .await;
            }
            (
                StatusCode::OK,
                Json(ApiResponse::success_with_message("Profile deleted", data)),
            )
        }
        Err(err) => esim_error_response::<EsimCommandResponse>(err),
    }
}

fn find_and_normalize_profile(value: &serde_json::Value) -> Option<EsimProfile> {
    if let Some(obj) = value.as_object() {
        if obj.contains_key("iccid") || obj.contains_key("ICCID") {
            return Some(crate::esim::normalize_profile(value));
        }
        for (_, val) in obj {
            if let Some(p) = find_and_normalize_profile(val) {
                return Some(p);
            }
        }
    } else if let Some(arr) = value.as_array() {
        for val in arr {
            if let Some(p) = find_and_normalize_profile(val) {
                return Some(p);
            }
        }
    }
    None
}

/// POST /api/esim/profiles
pub async fn download_esim_profile_handler(
    State(app): State<AppState>,
    Json(payload): Json<EsimDownloadRequest>,
) -> impl IntoResponse {
    let smdp = payload.smdp.trim().to_string();
    let matching_id = payload.matching_id.trim().to_string();
    if smdp.is_empty() || matching_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<EsimCommandResponse>::error(
                "SM-DP+ server and Matching ID cannot be empty",
            )),
        );
    }

    // 在写卡前，先异步读取一次卡上的所有 profile ICCID 集合，用于后续新卡判定
    let initial_iccids_opt: Option<std::collections::HashSet<String>> =
        app.esim_supervisor.get_profiles().await.ok().map(|resp| {
            resp.profiles
                .into_iter()
                .map(|p| crate::utils::normalize_iccid(&p.iccid))
                .collect()
        });

    match app.esim_supervisor.download_profile(payload.clone()).await {
        Ok(data) => {
            if esim_command_succeeded(&data) {
                // Attempt to recursively find the downloaded profile details in lpac's response
                let profile_val = data.data.clone().unwrap_or(serde_json::Value::Null);
                if let Some(mut profile) = find_and_normalize_profile(&profile_val) {
                    // Supplement SM-DP+ if not returned
                    if profile.smdp.as_deref().unwrap_or("").trim().is_empty() {
                        profile.smdp = Some(smdp.clone());
                    }
                    if profile
                        .matching_id
                        .as_deref()
                        .unwrap_or("")
                        .trim()
                        .is_empty()
                    {
                        profile.matching_id = Some(matching_id.clone());
                    }

                    let entry = EsimProfileCacheEntry {
                        iccid: profile.iccid.clone(),
                        name: Some(profile.name.clone()),
                        provider: Some(profile.provider.clone()),
                        state: Some(profile.state.clone()),
                        profile_class: Some(profile.profile_class.clone()),
                        imsi: profile.imsi.clone(),
                        msisdn: profile.msisdn.clone(),
                        smsc: profile.smsc.clone(),
                        smdp: profile.smdp.clone(),
                        matching_id: profile.matching_id.clone(),
                        isdp_aid: profile.isdp_aid.clone(),
                        mcc: profile.mcc.clone(),
                        mnc: profile.mnc.clone(),
                        disable_allowed: profile.disable_allowed,
                        delete_allowed: profile.delete_allowed,
                        updated_at: chrono::Utc::now().to_rfc3339(),
                    };

                    if let Err(err) = app.database.upsert_esim_profile_cache(&entry) {
                        warn!(iccid = %entry.iccid, error = %err, "Failed to cache downloaded eSIM profile to database");
                    }

                    app.system_event_emitter
                        .emit_code(
                            system_event_codes::ESIM_PROFILE_DOWNLOAD_SUCCEEDED,
                            system_event_severity::INFO,
                            system_event_status::SUCCEEDED,
                            mask_identifier(&entry.iccid),
                            "Profile 写入并缓存成功",
                        )
                        .await;
                } else {
                    // Fallback if we couldn't parse the profile details from lpac.
                    // Query the profiles on the card to identify the new one(s) that lack smdp/matching_id in cache.
                    let mut cached_fallback_iccid = None;

                    // 1. 等待 1.5 秒，让 eUICC 卡片状态恢复稳定
                    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

                    // 2. 尝试读取最新列表，最多重试 4 次，每次间隔 1.5 秒
                    let mut profiles_resp = None;
                    for attempt in 1..=4 {
                        match app.esim_supervisor.get_profiles().await {
                            Ok(resp) => {
                                profiles_resp = Some(resp);
                                break;
                            }
                            Err(err) => {
                                warn!(attempt = attempt, error = ?err, "Failed to get profiles during fallback retry");
                                if attempt < 4 {
                                    tokio::time::sleep(std::time::Duration::from_millis(1500))
                                        .await;
                                }
                            }
                        }
                    }

                    if let Some(resp) = profiles_resp {
                        if let Some(ref init_iccids) = initial_iccids_opt {
                            for p in resp.profiles {
                                let norm_iccid = crate::utils::normalize_iccid(&p.iccid);
                                let is_new_profile = !init_iccids.contains(&norm_iccid);

                                if is_new_profile {
                                    let needs_cache =
                                        match app.database.get_esim_profile_cache(&p.iccid) {
                                            Ok(Some(cached_entry)) => cached_entry
                                                .smdp
                                                .as_deref()
                                                .unwrap_or("")
                                                .trim()
                                                .is_empty(),
                                            _ => true,
                                        };
                                    if needs_cache {
                                        let entry = EsimProfileCacheEntry {
                                            iccid: p.iccid.clone(),
                                            name: Some(p.name.clone()),
                                            provider: Some(p.provider.clone()),
                                            state: Some(p.state.clone()),
                                            profile_class: Some(p.profile_class.clone()),
                                            imsi: p.imsi.clone(),
                                            msisdn: p.msisdn.clone(),
                                            smsc: p.smsc.clone(),
                                            smdp: Some(smdp.clone()),
                                            matching_id: Some(matching_id.clone()),
                                            isdp_aid: p.isdp_aid.clone(),
                                            mcc: p.mcc.clone(),
                                            mnc: p.mnc.clone(),
                                            disable_allowed: p.disable_allowed,
                                            delete_allowed: p.delete_allowed,
                                            updated_at: chrono::Utc::now().to_rfc3339(),
                                        };
                                        if let Err(err) =
                                            app.database.upsert_esim_profile_cache(&entry)
                                        {
                                            warn!(iccid = %entry.iccid, error = %err, "Failed to cache fallback eSIM profile to database");
                                        } else {
                                            cached_fallback_iccid = Some(p.iccid.clone());
                                        }
                                    }
                                }
                            }
                        } else {
                            warn!("Initial ICCIDs list was unavailable before writing; fallback difference detection skipped to prevent profile mismatch");
                        }
                    } else {
                        error!("Failed to fetch profiles list after writing even with retries; fallback profile caching cannot proceed");
                    }

                    let event_entity = cached_fallback_iccid
                        .as_ref()
                        .map(|iccid| mask_identifier(iccid))
                        .unwrap_or_else(|| "esim".to_string());

                    app.system_event_emitter
                        .emit_code(
                            system_event_codes::ESIM_PROFILE_DOWNLOAD_SUCCEEDED,
                            system_event_severity::INFO,
                            system_event_status::SUCCEEDED,
                            event_entity,
                            "Profile 写入成功，已通过列表扫描更新缓存",
                        )
                        .await;
                }
            } else {
                let msg = data.msg.clone();
                let is_refused = msg.contains("MatchingID is refused")
                    || msg.contains("es9p_initiate_authentication")
                    || msg.contains("es10b_load_bound_profile_package")
                    || data
                        .data
                        .as_ref()
                        .map(|v| {
                            let s = v.to_string();
                            s.contains("MatchingID is refused")
                                || s.contains("es9p_initiate_authentication")
                                || s.contains("es10b_load_bound_profile_package")
                        })
                        .unwrap_or(false);

                if is_refused {
                    info!("MatchingID is refused, attempting to bind matching info to the profile if it exists");
                    let mut cached_fallback_iccid = None;
                    if let Ok(profiles_resp) = app.esim_supervisor.get_profiles().await {
                        for p in profiles_resp.profiles {
                            let needs_cache = match app.database.get_esim_profile_cache(&p.iccid) {
                                Ok(Some(cached_entry)) => {
                                    cached_entry.smdp.as_deref().unwrap_or("").trim().is_empty()
                                }
                                _ => true,
                            };
                            if needs_cache {
                                let entry = EsimProfileCacheEntry {
                                    iccid: p.iccid.clone(),
                                    name: Some(p.name.clone()),
                                    provider: Some(p.provider.clone()),
                                    state: Some(p.state.clone()),
                                    profile_class: Some(p.profile_class.clone()),
                                    imsi: p.imsi.clone(),
                                    msisdn: p.msisdn.clone(),
                                    smsc: p.smsc.clone(),
                                    smdp: Some(smdp.clone()),
                                    matching_id: Some(matching_id.clone()),
                                    isdp_aid: p.isdp_aid.clone(),
                                    mcc: p.mcc.clone(),
                                    mnc: p.mnc.clone(),
                                    disable_allowed: p.disable_allowed,
                                    delete_allowed: p.delete_allowed,
                                    updated_at: chrono::Utc::now().to_rfc3339(),
                                };
                                if let Ok(_) = app.database.upsert_esim_profile_cache(&entry) {
                                    cached_fallback_iccid = Some(p.iccid.clone());
                                    break;
                                }
                            }
                        }
                    }
                    if let Some(ref iccid) = cached_fallback_iccid {
                        app.system_event_emitter
                            .emit_code(
                                system_event_codes::ESIM_PROFILE_DOWNLOAD_SUCCEEDED,
                                system_event_severity::INFO,
                                system_event_status::SUCCEEDED,
                                mask_identifier(iccid),
                                "Profile 已被使用，成功将 Matching ID 绑定至对应卡片",
                            )
                            .await;
                    }
                }

                app.system_event_emitter
                    .emit_code(
                        system_event_codes::ESIM_PROFILE_DOWNLOAD_FAILED,
                        system_event_severity::WARNING,
                        system_event_status::FAILED,
                        "esim",
                        format!("Profile 写入失败: {}", data.msg),
                    )
                    .await;
            }
            (
                StatusCode::OK,
                Json(ApiResponse::success_with_message(
                    "Profile downloaded",
                    data,
                )),
            )
        }
        Err(err) => {
            let message = err.message();
            app.system_event_emitter
                .emit_code(
                    system_event_codes::ESIM_PROFILE_DOWNLOAD_FAILED,
                    system_event_severity::WARNING,
                    system_event_status::FAILED,
                    "esim",
                    format!("Profile 写入失败: {message}"),
                )
                .await;
            esim_error_response::<EsimCommandResponse>(err)
        }
    }
}

// ============ 设备信息 ============

/// GET /api/device
pub async fn get_device_info(State(conn): State<Arc<Connection>>) -> impl IntoResponse {
    match get_device_info_data(&conn).await {
        Ok(data) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message("Success", data)),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<DeviceInfoResponse>::error(format!(
                "Failed: {}",
                e
            ))),
        ),
    }
}

// ============ SIM 卡 ============

fn sim_identity_from_response(data: &SimInfoResponse) -> modem_manager::SimIdentity {
    let mut operator_id = data.registered_operator_code.trim().to_string();
    if operator_id.is_empty() && !data.mcc.is_empty() && !data.mnc.is_empty() {
        operator_id = format!("{}{}", data.mcc, data.mnc);
    }
    modem_manager::SimIdentity {
        iccid: data.iccid.clone(),
        imsi: data.imsi.clone(),
        operator_id,
    }
}

fn maybe_refresh_sim_details_after_fast_response(
    conn: &Arc<Connection>,
    db: &Arc<Database>,
    data: &SimInfoResponse,
) {
    if !data.present {
        return;
    }
    let identity = sim_identity_from_response(data);
    if !sim_details_cache_missing(db, &identity) {
        return;
    }
    let conn_bg = Arc::clone(conn);
    let db_bg = Arc::clone(db);
    tokio::spawn(async move {
        refresh_sim_details_background(&conn_bg, &db_bg, false).await;
    });
}

/// GET /api/sim
pub async fn get_sim_info(
    State((conn, db)): State<(Arc<Connection>, Arc<Database>)>,
) -> impl IntoResponse {
    match get_sim_info_data_with_cache(&conn, Some(&db)).await {
        Ok(data) => {
            maybe_refresh_sim_details_after_fast_response(&conn, &db, &data);
            (
                StatusCode::OK,
                Json(ApiResponse::success_with_message("Success", data)),
            )
        }
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<SimInfoResponse>::error(format!(
                "Failed: {}",
                e
            ))),
        ),
    }
}

/// POST /api/sim/details/refresh
pub async fn refresh_sim_details_handler(
    State((conn, db)): State<(Arc<Connection>, Arc<Database>)>,
) -> impl IntoResponse {
    let conn_bg = Arc::clone(&conn);
    let db_bg = Arc::clone(&db);
    tokio::spawn(async move {
        refresh_sim_details_background(&conn_bg, &db_bg, true).await;
    });

    (
        StatusCode::OK,
        Json(ApiResponse::success_with_message(
            "SIM details refresh started",
            json!({}),
        )),
    )
}

/// POST /api/sim/cache
pub async fn update_sim_cache_handler(
    State(app): State<AppState>,
    Json(payload): Json<UpdateSimCacheRequest>,
) -> impl IntoResponse {
    let identity = match tokio::time::timeout(
        std::time::Duration::from_secs(ESIM_SIM_IDENTITY_TIMEOUT_SECS),
        current_sim_identity(&app.dbus_conn),
    )
    .await
    {
        Ok(Some(identity)) => identity,
        _ => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ApiResponse::<serde_json::Value>::error(
                    "Unable to get current SIM identity",
                )),
            );
        }
    };

    if let Some(sms_center) = &payload.sms_center {
        crate::modem_manager::cache_smsc_for_identity(
            &app.database,
            &identity,
            sms_center,
            "manual",
        );
    }

    if let Some(phone_number) = &payload.phone_number {
        crate::modem_manager::cache_own_numbers_for_identity(
            &app.database,
            &identity,
            &[phone_number.clone()],
            "manual",
        );
    }

    (
        StatusCode::OK,
        Json(ApiResponse::success_with_message(
            "SIM cache updated",
            json!({}),
        )),
    )
}

// ============ 网络信息 ============

/// GET /api/network
pub async fn get_network_info(State(conn): State<Arc<Connection>>) -> impl IntoResponse {
    match get_network_info_data(&conn).await {
        Ok(data) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message("Success", data)),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<NetworkInfoResponse>::error(format!(
                "Failed: {}",
                e
            ))),
        ),
    }
}

/// GET /api/cells
pub async fn get_cells(State(conn): State<Arc<Connection>>) -> impl IntoResponse {
    match get_cells_data(&conn).await {
        Ok(data) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message("Success", data)),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<CellsResponse>::error(format!(
                "Failed: {}",
                e
            ))),
        ),
    }
}

/// POST /api/cell-monitor/start
pub async fn start_cell_monitor_handler(State(app): State<AppState>) -> impl IntoResponse {
    if app.cell_monitoring_active.swap(true, Ordering::SeqCst) {
        return (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "Cell monitor already active",
                json!({}),
            )),
        );
    }

    match start_cell_monitoring().await {
        Ok(()) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "Cell monitor activated",
                json!({}),
            )),
        ),
        Err(e) => {
            app.cell_monitoring_active.store(false, Ordering::SeqCst);
            (
                StatusCode::OK,
                Json(ApiResponse::<serde_json::Value>::error(format!(
                    "Failed: {}",
                    e
                ))),
            )
        }
    }
}

/// POST /api/cell-monitor/stop
pub async fn stop_cell_monitor_handler(State(app): State<AppState>) -> impl IntoResponse {
    if !app.cell_monitoring_active.swap(false, Ordering::SeqCst) {
        return (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "Cell monitor already inactive",
                json!({}),
            )),
        );
    }

    match stop_cell_monitoring().await {
        Ok(()) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "Cell monitor deactivated",
                json!({}),
            )),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<serde_json::Value>::error(format!(
                "Failed: {}",
                e
            ))),
        ),
    }
}

/// GET /api/radio-mode
pub async fn get_radio_mode_handler(State(conn): State<Arc<Connection>>) -> impl IntoResponse {
    match get_radio_mode(&conn).await {
        Ok(data) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message("Success", data)),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<RadioModeResponse>::error(format!(
                "Failed: {}",
                e
            ))),
        ),
    }
}

/// POST /api/radio-mode
pub async fn set_radio_mode_handler(
    State(conn): State<Arc<Connection>>,
    Json(payload): Json<RadioModeRequest>,
) -> impl IntoResponse {
    match set_radio_mode(&conn, payload.mode).await {
        Ok(()) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "Radio mode updated",
                json!({}),
            )),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<serde_json::Value>::error(format!(
                "Failed: {}",
                e
            ))),
        ),
    }
}

/// GET /api/band-lock
pub async fn get_band_lock_handler(State(conn): State<Arc<Connection>>) -> impl IntoResponse {
    match get_band_lock_status(&conn).await {
        Ok(data) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message("Success", data)),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<BandLockStatus>::error(format!(
                "Failed: {}",
                e
            ))),
        ),
    }
}

/// POST /api/band-lock
pub async fn set_band_lock_handler(
    State(conn): State<Arc<Connection>>,
    Json(payload): Json<BandLockRequest>,
) -> impl IntoResponse {
    match set_band_lock(&conn, &payload).await {
        Ok(()) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "Band selection updated",
                json!({}),
            )),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<serde_json::Value>::error(format!(
                "Failed: {}",
                e
            ))),
        ),
    }
}

/// GET /api/location/cell-info
pub async fn get_cell_location_handler(State(conn): State<Arc<Connection>>) -> impl IntoResponse {
    match get_cell_location(&conn).await {
        Ok(data) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message("Success", data)),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<CellLocationResponse>::error(format!(
                "Failed: {}",
                e
            ))),
        ),
    }
}

/// GET /api/network/operators
pub async fn get_network_operators(State(conn): State<Arc<Connection>>) -> impl IntoResponse {
    match get_operators_list(&conn).await {
        Ok(data) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message("Success", data)),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<OperatorListResponse>::error(format!(
                "Failed: {}",
                e
            ))),
        ),
    }
}

/// GET /api/network/operators/scan
pub async fn scan_network_operators(State(conn): State<Arc<Connection>>) -> impl IntoResponse {
    match scan_operators(&conn).await {
        Ok(data) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message("Success", data)),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<OperatorListResponse>::error(format!(
                "Failed: {}",
                e
            ))),
        ),
    }
}

/// POST /api/network/register-manual
pub async fn register_network_manual(
    State(conn): State<Arc<Connection>>,
    Json(payload): Json<ManualRegisterRequest>,
) -> impl IntoResponse {
    match register_operator_manual(&conn, &payload.mccmnc).await {
        Ok(()) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "Registration started",
                json!({}),
            )),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<serde_json::Value>::error(format!(
                "Failed: {}",
                e
            ))),
        ),
    }
}

/// POST /api/network/register-auto
pub async fn register_network_auto(State(conn): State<Arc<Connection>>) -> impl IntoResponse {
    match register_operator_auto(&conn).await {
        Ok(()) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "Auto registration started",
                json!({}),
            )),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<serde_json::Value>::error(format!(
                "Failed: {}",
                e
            ))),
        ),
    }
}

/// GET /api/apn
pub async fn get_apn_list_handler(State(app): State<AppState>) -> impl IntoResponse {
    let apn_config = app.config_manager.get_apn_config();
    match list_apn_contexts(&app.dbus_conn, Some(&apn_config)).await {
        Ok(data) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message("Success", data)),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<ApnListResponse>::error(format!(
                "Failed: {}",
                e
            ))),
        ),
    }
}

/// POST /api/apn
pub async fn set_apn_handler(
    State(app): State<AppState>,
    Json(payload): Json<SetApnRequest>,
) -> impl IntoResponse {
    let mut apn_config = app.config_manager.get_apn_config();
    if let Some(apn) = &payload.apn {
        apn_config.apn = apn.trim().to_string();
    }
    if let Some(protocol) = &payload.protocol {
        apn_config.protocol = protocol.trim().to_string();
    }
    if let Some(username) = &payload.username {
        apn_config.username = username.trim().to_string();
    }
    if let Some(password) = &payload.password {
        apn_config.password = password.clone();
    }
    if let Some(auth_method) = &payload.auth_method {
        apn_config.auth_method = auth_method.trim().to_string();
    }
    if apn_config.protocol.trim().is_empty() {
        apn_config.protocol = ApnConfig::default().protocol;
    }
    if apn_config.auth_method.trim().is_empty() {
        apn_config.auth_method = ApnConfig::default().auth_method;
    }

    if let Err(err) = app.config_manager.set_apn_config(apn_config) {
        return (
            StatusCode::OK,
            Json(ApiResponse::<serde_json::Value>::error(format!(
                "Failed to save APN config: {}",
                err
            ))),
        );
    }

    let context_path = payload.context_path.trim();
    if context_path.is_empty() || context_path.ends_with("/bearer/default") {
        return (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "APN config saved",
                json!({}),
            )),
        );
    }

    match set_apn_on_bearer(&app.dbus_conn, &payload).await {
        Ok(()) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message("APN updated", json!({}))),
        ),
        Err(e) => {
            warn!(error = %e, "APN config saved but bearer update failed");
            (
                StatusCode::OK,
                Json(ApiResponse::success_with_message(
                    "APN config saved",
                    json!({ "bearer_update_error": e.to_string() }),
                )),
            )
        }
    }
}

/// GET /api/cell-lock
pub async fn get_cell_lock_status_handler(State(app): State<AppState>) -> impl IntoResponse {
    let store = app.cell_lock.lock().await;
    let data = store.status();
    drop(store);
    (
        StatusCode::OK,
        Json(ApiResponse::success_with_message("Success", data)),
    )
}

/// POST /api/cell-lock
pub async fn set_cell_lock_handler(
    State(app): State<AppState>,
    Json(payload): Json<CellLockRequest>,
) -> impl IntoResponse {
    let mut store = app.cell_lock.lock().await;
    match store.apply(&payload) {
        Ok(()) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "OK",
                CellLockResult { success: true },
            )),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<CellLockResult>::error(e)),
        ),
    }
}

/// POST /api/cell-lock/unlock-all
pub async fn unlock_all_cells_handler(State(app): State<AppState>) -> impl IntoResponse {
    let mut store = app.cell_lock.lock().await;
    store.unlock_all();
    (
        StatusCode::OK,
        Json(ApiResponse::success_with_message(
            "Unlocked",
            CellLockResult { success: true },
        )),
    )
}

/// GET /api/network/interfaces
pub async fn get_network_interfaces_info(
    State(dbus_conn): State<Arc<Connection>>,
) -> impl IntoResponse {
    match read_network_interfaces(Some(&dbus_conn)).await {
        Ok(interfaces) => {
            let total_count = interfaces.len();
            (
                StatusCode::OK,
                Json(ApiResponse::success_with_message(
                    "Success",
                    NetworkInterfacesResponse {
                        interfaces,
                        total_count,
                    },
                )),
            )
        }
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<NetworkInterfacesResponse>::error(format!(
                "Failed: {}",
                e
            ))),
        ),
    }
}

/// GET /api/network/connection-addresses
pub async fn get_network_connection_addresses(
    State(dbus_conn): State<Arc<Connection>>,
) -> impl IntoResponse {
    match read_network_interfaces(Some(&dbus_conn)).await {
        Ok(interfaces) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "Success",
                connection_addresses_from_interfaces(&interfaces),
            )),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<ConnectionAddressesResponse>::error(format!(
                "Failed: {}",
                e
            ))),
        ),
    }
}

/// GET /api/device-network/ddns/config
pub async fn get_device_ddns_config_handler(State(app): State<AppState>) -> impl IntoResponse {
    let config = app.config_manager.get_ddns_config();
    let access_secret_set = !config.access_secret.trim().is_empty();
    (
        StatusCode::OK,
        Json(ApiResponse::success_with_message(
            "Success",
            ddns_config_response(config, access_secret_set),
        )),
    )
}

/// POST /api/device-network/ddns/config
pub async fn set_device_ddns_config_handler(
    State(app): State<AppState>,
    Json(mut payload): Json<crate::config::DdnsConfig>,
) -> impl IntoResponse {
    let current = app.config_manager.get_ddns_config();
    if is_masked_secret(&payload.access_id) {
        payload.access_id = current.access_id;
    }
    if payload.access_secret.trim().is_empty() {
        payload.access_secret = current.access_secret;
    } else if is_masked_secret(&payload.access_secret) {
        payload.access_secret = current.access_secret;
    }
    if payload.interval_seconds == 0 {
        payload.interval_seconds = 300;
    }
    if payload.ttl == 0 {
        payload.ttl = 600;
    }

    match app.config_manager.set_ddns_config(payload.clone()) {
        Ok(()) => {
            let access_secret_set = !payload.access_secret.trim().is_empty();
            (
                StatusCode::OK,
                Json(ApiResponse::success_with_message(
                    "DDNS config updated",
                    ddns_config_response(payload, access_secret_set),
                )),
            )
        }
        Err(err) => (
            StatusCode::OK,
            Json(ApiResponse::<serde_json::Value>::error(format!(
                "Failed: {}",
                err
            ))),
        ),
    }
}

fn ddns_config_response(
    mut config: crate::config::DdnsConfig,
    access_secret_set: bool,
) -> serde_json::Value {
    config.access_id = mask_secret(&config.access_id);
    config.access_secret = mask_secret(&config.access_secret);
    let mut value = serde_json::to_value(config).unwrap_or_else(|_| json!({}));
    if let Some(object) = value.as_object_mut() {
        object.insert("access_secret_set".to_string(), json!(access_secret_set));
    }
    value
}

fn mask_secret(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let prefix: String = trimmed.chars().take(3).collect();
    format!("{prefix}******")
}

fn is_masked_secret(value: &str) -> bool {
    value.contains('*')
}

/// GET /api/device-network/ddns/status
pub async fn get_device_ddns_status_handler(State(app): State<AppState>) -> impl IntoResponse {
    let config = app.config_manager.get_ddns_config();
    let status = app.ddns_manager.status(&config).await;
    (
        StatusCode::OK,
        Json(ApiResponse::success_with_message("Success", status)),
    )
}

/// POST /api/device-network/ddns/sync
pub async fn sync_device_ddns_handler(State(app): State<AppState>) -> impl IntoResponse {
    match app
        .ddns_manager
        .sync_now(app.config_manager.clone(), app.notification_sender.clone())
        .await
    {
        Ok(data) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "DDNS sync completed",
                data,
            )),
        ),
        Err(err) => (
            StatusCode::OK,
            Json(ApiResponse::<DdnsSyncResponse>::error(format!(
                "Failed: {}",
                err
            ))),
        ),
    }
}

/// GET /api/device-network/ddns/logs
pub async fn get_device_ddns_logs_handler(State(app): State<AppState>) -> impl IntoResponse {
    let logs = app.ddns_manager.logs().await;
    (
        StatusCode::OK,
        Json(ApiResponse::success_with_message("Success", logs)),
    )
}

/// POST /api/device-network/ddns/logs/clear
pub async fn clear_device_ddns_logs_handler(State(app): State<AppState>) -> impl IntoResponse {
    app.ddns_manager.clear_logs().await;
    (
        StatusCode::OK,
        Json(ApiResponse::success_with_message(
            "DDNS logs cleared",
            json!({}),
        )),
    )
}

/// GET /api/device-network/wlan/status
pub async fn get_device_wlan_status_handler() -> impl IntoResponse {
    match crate::device_network::wlan_status().await {
        Ok(data) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message("Success", data)),
        ),
        Err(err) => (
            StatusCode::OK,
            Json(ApiResponse::<WlanStatusResponse>::error(format!(
                "Failed: {}",
                err
            ))),
        ),
    }
}

/// POST /api/device-network/wlan/enabled
pub async fn set_device_wlan_enabled_handler(
    Json(payload): Json<WlanEnabledRequest>,
) -> impl IntoResponse {
    match crate::device_network::wlan_set_enabled(payload).await {
        Ok(data) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "WLAN state updated",
                data,
            )),
        ),
        Err(err) => (
            StatusCode::OK,
            Json(ApiResponse::<WlanStatusResponse>::error(format!(
                "Failed: {}",
                err
            ))),
        ),
    }
}

/// POST /api/device-network/wlan/scan
pub async fn scan_device_wlan_handler() -> impl IntoResponse {
    match crate::device_network::wlan_scan().await {
        Ok(data) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message("Success", data)),
        ),
        Err(err) => (
            StatusCode::OK,
            Json(ApiResponse::<WlanScanResponse>::error(format!(
                "Failed: {}",
                err
            ))),
        ),
    }
}

/// GET /api/device-network/wlan/profiles
pub async fn get_device_wlan_profiles_handler() -> impl IntoResponse {
    match crate::device_network::wlan_profiles().await {
        Ok(data) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message("Success", data)),
        ),
        Err(err) => (
            StatusCode::OK,
            Json(ApiResponse::<WlanProfilesResponse>::error(format!(
                "Failed: {}",
                err
            ))),
        ),
    }
}

/// POST /api/device-network/wlan/forget
pub async fn forget_device_wlan_handler(
    Json(payload): Json<WlanForgetRequest>,
) -> impl IntoResponse {
    match crate::device_network::wlan_forget(payload).await {
        Ok(data) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "WLAN profile forgotten",
                data,
            )),
        ),
        Err(err) => (
            StatusCode::OK,
            Json(ApiResponse::<WlanProfilesResponse>::error(format!(
                "Failed: {}",
                err
            ))),
        ),
    }
}

/// POST /api/device-network/wlan/connect
pub async fn connect_device_wlan_handler(
    State(app): State<AppState>,
    Json(payload): Json<WlanConnectRequest>,
) -> impl IntoResponse {
    let target_ssid = payload.ssid.clone();
    let previous = crate::device_network::wlan_status().await.ok();
    match crate::device_network::wlan_connect(payload).await {
        Ok(data) => {
            if data.connected {
                app.system_event_emitter
                    .emit_code(
                        system_event_codes::DEVICE_NETWORK_WLAN_CONNECTED,
                        system_event_severity::INFO,
                        system_event_status::SUCCEEDED,
                        data.ssid.clone().unwrap_or_else(|| target_ssid.clone()),
                        "WLAN 已连接",
                    )
                    .await;
                let previous_ssid = previous.and_then(|status| status.ssid);
                if previous_ssid.is_some() && previous_ssid != data.ssid && data.ssid.is_some() {
                    app.system_event_emitter
                        .emit_code(
                            system_event_codes::DEVICE_NETWORK_WLAN_SSID_CHANGED,
                            system_event_severity::INFO,
                            system_event_status::CHANGED,
                            data.ssid.clone().unwrap_or_default(),
                            "WLAN SSID 已变化",
                        )
                        .await;
                }
            }
            (
                StatusCode::OK,
                Json(ApiResponse::success_with_message("WLAN connected", data)),
            )
        }
        Err(err) => {
            app.system_event_emitter
                .emit_code(
                    system_event_codes::DEVICE_NETWORK_WLAN_CONNECT_FAILED,
                    system_event_severity::WARNING,
                    system_event_status::FAILED,
                    target_ssid,
                    format!("WLAN 连接失败: {err}"),
                )
                .await;
            (
                StatusCode::OK,
                Json(ApiResponse::<WlanStatusResponse>::error(format!(
                    "Failed: {}",
                    err
                ))),
            )
        }
    }
}

/// POST /api/device-network/wlan/disconnect
pub async fn disconnect_device_wlan_handler(State(app): State<AppState>) -> impl IntoResponse {
    let previous = crate::device_network::wlan_status().await.ok();
    match crate::device_network::wlan_disconnect().await {
        Ok(data) => {
            if previous
                .as_ref()
                .map(|status| status.connected)
                .unwrap_or(false)
                && !data.connected
            {
                app.system_event_emitter
                    .emit_code(
                        system_event_codes::DEVICE_NETWORK_WLAN_DISCONNECTED,
                        system_event_severity::INFO,
                        system_event_status::CHANGED,
                        previous.and_then(|status| status.ssid).unwrap_or_default(),
                        "WLAN 已断开",
                    )
                    .await;
            }
            (
                StatusCode::OK,
                Json(ApiResponse::success_with_message("WLAN disconnected", data)),
            )
        }
        Err(err) => (
            StatusCode::OK,
            Json(ApiResponse::<WlanStatusResponse>::error(format!(
                "Failed: {}",
                err
            ))),
        ),
    }
}

/// POST /api/device-network/wlan/profile
pub async fn save_device_wlan_profile_handler(
    Json(payload): Json<WlanProfileRequest>,
) -> impl IntoResponse {
    match crate::device_network::wlan_save_profile(payload).await {
        Ok(data) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "WLAN profile updated",
                data,
            )),
        ),
        Err(err) => (
            StatusCode::OK,
            Json(ApiResponse::<WlanStatusResponse>::error(format!(
                "Failed: {}",
                err
            ))),
        ),
    }
}

/// GET /api/network/signal-strength
pub async fn get_signal_strength_handler(State(conn): State<Arc<Connection>>) -> impl IntoResponse {
    match get_signal_strength(&conn).await {
        Ok(data) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message("Success", data)),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<SignalStrengthResponse>::error(format!(
                "Failed: {}",
                e
            ))),
        ),
    }
}

// ============ 数据连接 ============

/// GET /api/data
pub async fn get_data_status(State(app): State<AppState>) -> impl IntoResponse {
    if app.data_user_disabled.load(Ordering::SeqCst) {
        return (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "Success",
                DataConnectionResponse { active: false },
            )),
        );
    }

    match get_data_connection_status(&app.dbus_conn).await {
        Ok(active) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "Success",
                DataConnectionResponse { active },
            )),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<DataConnectionResponse>::error(format!(
                "Failed: {}",
                e
            ))),
        ),
    }
}

/// POST /api/data
pub async fn set_data_status(
    State(app): State<AppState>,
    Json(payload): Json<DataConnectionRequest>,
) -> impl IntoResponse {
    let previous_active = !app.data_user_disabled.load(Ordering::SeqCst);
    let allow_roaming = app.config_manager.get_roaming_allowed();
    let apn_config = app.config_manager.get_apn_config();
    match set_data_connection_with_apn(
        &app.dbus_conn,
        payload.active,
        allow_roaming,
        Some(&apn_config),
    )
    .await
    {
        Ok(_) => {
            if let Err(err) = app.config_manager.set_data_enabled(payload.active) {
                return (
                    StatusCode::OK,
                    Json(ApiResponse::<DataConnectionResponse>::error(format!(
                        "Failed to save data switch state: {}",
                        err
                    ))),
                );
            }
            app.data_user_disabled
                .store(!payload.active, Ordering::SeqCst);
            if previous_active != payload.active {
                app.system_event_emitter
                    .emit_code(
                        system_event_codes::CELLULAR_DATA_ENABLED_CHANGED,
                        system_event_severity::INFO,
                        system_event_status::CHANGED,
                        "cellular_data",
                        if payload.active {
                            "蜂窝数据开关已开启"
                        } else {
                            "蜂窝数据开关已关闭"
                        },
                    )
                    .await;
            }
            // 同步 NM autoconnect 状态，防止用户关闭数据后 NM 自动重连
            tokio::spawn(async move {
                if let Ok(profile) = find_nm_modem_connection_pub().await {
                    let _ = nm_set_autoconnect_pub(&profile, payload.active).await;
                }
            });
            (
                StatusCode::OK,
                Json(ApiResponse::success_with_message(
                    "Data connection updated",
                    DataConnectionResponse {
                        active: payload.active,
                    },
                )),
            )
        }
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<DataConnectionResponse>::error(format!(
                "Failed: {}",
                e
            ))),
        ),
    }
}

pub async fn restart_baseband_handler(State(app): State<AppState>) -> impl IntoResponse {
    let auto_connect_data = !app.data_user_disabled.load(Ordering::SeqCst);
    let allow_roaming = app.config_manager.get_roaming_allowed();
    let apn_config = app.config_manager.get_apn_config();
    match restart_baseband(
        &app.dbus_conn,
        auto_connect_data,
        allow_roaming,
        Some(apn_config),
    )
    .await
    {
        Ok(data) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "Baseband restarted",
                data,
            )),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<BasebandRestartResponse>::error(format!(
                "重启基带失败：{e}",
            ))),
        ),
    }
}

pub async fn get_baseband_restart_status_handler() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse::success_with_message(
            "Success",
            get_baseband_restart_progress(),
        )),
    )
}

/// GET /api/roaming
pub async fn get_roaming_status_handler(State(app): State<AppState>) -> impl IntoResponse {
    let roaming_allowed = app.config_manager.get_roaming_allowed();
    match get_is_roaming_mm(&app.dbus_conn).await {
        Ok(is_roaming) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "Success",
                RoamingResponse {
                    roaming_allowed,
                    is_roaming,
                },
            )),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<RoamingResponse>::error(format!(
                "Failed: {}",
                e
            ))),
        ),
    }
}

/// POST /api/roaming
pub async fn set_roaming_status_handler(
    State(app): State<AppState>,
    Json(payload): Json<RoamingRequest>,
) -> impl IntoResponse {
    let previous_allowed = app.config_manager.get_roaming_allowed();
    match apply_roaming_policy(&app.dbus_conn, &app.config_manager, payload.allowed).await {
        Ok(_) => {
            let roaming_allowed = app.config_manager.get_roaming_allowed();
            if previous_allowed != roaming_allowed {
                app.system_event_emitter
                    .emit_code(
                        system_event_codes::CELLULAR_ROAMING_ALLOWED_CHANGED,
                        system_event_severity::INFO,
                        system_event_status::CHANGED,
                        "roaming",
                        if roaming_allowed {
                            "允许漫游已开启"
                        } else {
                            "允许漫游已关闭"
                        },
                    )
                    .await;
            }
            match get_is_roaming_mm(&app.dbus_conn).await {
                Ok(is_roaming) => (
                    StatusCode::OK,
                    Json(ApiResponse::success_with_message(
                        "Success",
                        RoamingResponse {
                            roaming_allowed,
                            is_roaming,
                        },
                    )),
                ),
                Err(e) => (
                    StatusCode::OK,
                    Json(ApiResponse::<RoamingResponse>::error(format!(
                        "Failed: {}",
                        e
                    ))),
                ),
            }
        }
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<RoamingResponse>::error(format!(
                "Failed: {}",
                e
            ))),
        ),
    }
}

/// POST /api/airplane-mode
pub async fn set_airplane_mode_handler(
    State(app): State<AppState>,
    Json(payload): Json<AirplaneModeRequest>,
) -> impl IntoResponse {
    let previous_enabled = get_airplane_mode(&app.dbus_conn)
        .await
        .ok()
        .map(|status| status.enabled);
    if payload.enabled {
        app.airplane_mode_requested.store(true, Ordering::SeqCst);
    }

    match set_airplane_mode(&app.dbus_conn, payload.enabled).await {
        Ok(_) => {
            app.airplane_mode_requested
                .store(payload.enabled, Ordering::SeqCst);
            match get_airplane_mode(&app.dbus_conn).await {
                Ok(status) => {
                    if previous_enabled != Some(status.enabled) {
                        app.system_event_emitter
                            .emit_code(
                                system_event_codes::CELLULAR_AIRPLANE_MODE_CHANGED,
                                system_event_severity::INFO,
                                system_event_status::CHANGED,
                                "airplane_mode",
                                if status.enabled {
                                    "飞行模式已开启"
                                } else {
                                    "飞行模式已关闭"
                                },
                            )
                            .await;
                    }
                    (
                        StatusCode::OK,
                        Json(ApiResponse::success_with_message(
                            if payload.enabled {
                                "Airplane mode enabled"
                            } else {
                                "Airplane mode disabled"
                            },
                            status,
                        )),
                    )
                }
                Err(e) => (
                    StatusCode::OK,
                    Json(ApiResponse::<AirplaneModeResponse>::error(format!(
                        "Failed: {}",
                        e
                    ))),
                ),
            }
        }
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<AirplaneModeResponse>::error(format!(
                "Failed: {}",
                e
            ))),
        ),
    }
}

/// GET /api/airplane-mode
pub async fn get_airplane_mode_handler(State(conn): State<Arc<Connection>>) -> impl IntoResponse {
    match get_airplane_mode(&conn).await {
        Ok(status) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message("Success", status)),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<AirplaneModeResponse>::error(format!(
                "Failed: {}",
                e
            ))),
        ),
    }
}

// ============ 短信功能 ============

use crate::db::{Database, EsimEuiccCacheEntry, EsimProfileCacheEntry};

fn schedule_sms_db_maintenance(app: &AppState, deleted: usize) {
    if deleted < SMS_DB_MAINTENANCE_DELETE_THRESHOLD {
        return;
    }

    if app.sms_db_maintenance_pending.swap(true, Ordering::SeqCst) {
        info!(
            deleted,
            threshold = SMS_DB_MAINTENANCE_DELETE_THRESHOLD,
            "SMS database maintenance already scheduled"
        );
        return;
    }

    let db = Arc::clone(&app.database);
    let pending = Arc::clone(&app.sms_db_maintenance_pending);
    tokio::spawn(async move {
        info!(
            deleted,
            delay_secs = SMS_DB_MAINTENANCE_DELAY_SECS,
            "SMS database maintenance scheduled"
        );
        tokio::time::sleep(tokio::time::Duration::from_secs(
            SMS_DB_MAINTENANCE_DELAY_SECS,
        ))
        .await;

        let result = tokio::task::spawn_blocking(move || db.vacuum()).await;
        match result {
            Ok(Ok(())) => info!("SMS database maintenance completed"),
            Ok(Err(err)) => warn!(error = %err, "SMS database maintenance failed"),
            Err(err) => warn!(error = %err, "SMS database maintenance task failed"),
        }
        pending.store(false, Ordering::SeqCst);
    });
}

/// POST /api/sms/send
pub async fn send_sms_handler(
    State(conn): State<Arc<Connection>>,
    State(db): State<Arc<Database>>,
    Json(payload): Json<SendSmsRequest>,
) -> impl IntoResponse {
    match send_sms(&conn, &payload.phone_number, &payload.content).await {
        Ok(path) => {
            // 存入数据库
            let _ = db.insert_sms(
                "outgoing",
                &payload.phone_number,
                &payload.content,
                "sent",
                None,
            );
            (
                StatusCode::OK,
                Json(ApiResponse::success_with_message(
                    "SMS sent",
                    json!({ "path": path }),
                )),
            )
        }
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<serde_json::Value>::error(format!(
                "Failed to send SMS: {}",
                e
            ))),
        ),
    }
}

/// GET /api/sms/list
pub async fn get_sms_list_handler(
    State(db): State<Arc<Database>>,
    Query(params): Query<SmsListRequest>,
) -> (StatusCode, Json<ApiResponse<SmsListResponse>>) {
    let limit = if params.limit > 0 { params.limit } else { 50 };
    let offset = if params.offset >= 0 { params.offset } else { 0 };
    let direction = params
        .direction
        .as_deref()
        .filter(|value| matches!(*value, "incoming" | "outgoing"));

    match db.get_sms_messages(limit, offset, direction) {
        Ok(messages) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "Success",
                SmsListResponse { messages },
            )),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<SmsListResponse>::error(format!(
                "Failed: {}",
                e
            ))),
        ),
    }
}

/// GET /api/sms/conversation
pub async fn get_sms_conversation_handler(
    State(db): State<Arc<Database>>,
    Query(params): Query<SmsConversationRequest>,
) -> (StatusCode, Json<ApiResponse<SmsListResponse>>) {
    let limit = if params.limit > 0 { params.limit } else { 50 };
    match db.get_sms_conversation(&params.phone_number, limit) {
        Ok(messages) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "Success",
                SmsListResponse { messages },
            )),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<SmsListResponse>::error(format!(
                "Failed: {}",
                e
            ))),
        ),
    }
}

/// GET /api/sms/stats
pub async fn get_sms_stats_handler(
    State(db): State<Arc<Database>>,
) -> (StatusCode, Json<ApiResponse<SmsStatsResponse>>) {
    match db.get_sms_stats() {
        Ok(stats) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message("Success", stats)),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<SmsStatsResponse>::error(format!(
                "Failed: {}",
                e
            ))),
        ),
    }
}

/// POST /api/sms/clear
pub async fn clear_sms_handler(
    State(app): State<AppState>,
) -> (StatusCode, Json<ApiResponse<serde_json::Value>>) {
    let deleted = app
        .database
        .get_sms_stats()
        .map(|stats| stats.total.max(0) as usize)
        .unwrap_or(SMS_DB_MAINTENANCE_DELETE_THRESHOLD);

    match app.database.clear_all_sms() {
        Ok(_) => {
            schedule_sms_db_maintenance(&app, deleted);
            (
                StatusCode::OK,
                Json(ApiResponse::success_with_message(
                    "All SMS cleared",
                    json!({ "deleted": deleted }),
                )),
            )
        }
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::error(format!("Failed: {}", e))),
        ),
    }
}

/// DELETE /api/sms/message/{id}
pub async fn delete_sms_message_handler(
    State(db): State<Arc<Database>>,
    Path(id): Path<i64>,
) -> (StatusCode, Json<ApiResponse<serde_json::Value>>) {
    match db.delete_sms(id) {
        Ok(deleted) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "SMS deleted",
                json!({ "deleted": deleted }),
            )),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::error(format!("Failed: {}", e))),
        ),
    }
}

/// DELETE /api/sms/conversation/{phone_number}
pub async fn delete_sms_conversation_handler(
    State(app): State<AppState>,
    Path(phone_number): Path<String>,
) -> (StatusCode, Json<ApiResponse<serde_json::Value>>) {
    match app.database.delete_sms_conversation(&phone_number) {
        Ok(deleted) => {
            schedule_sms_db_maintenance(&app, deleted);
            (
                StatusCode::OK,
                Json(ApiResponse::success_with_message(
                    "SMS conversation deleted",
                    json!({ "deleted": deleted }),
                )),
            )
        }
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::error(format!("Failed: {}", e))),
        ),
    }
}

/// POST /api/sms/batch-delete
pub async fn delete_sms_batch_handler(
    State(app): State<AppState>,
    Json(payload): Json<SmsBatchDeleteRequest>,
) -> (StatusCode, Json<ApiResponse<serde_json::Value>>) {
    if payload.ids.is_empty() && payload.phone_numbers.is_empty() {
        return (StatusCode::OK, Json(ApiResponse::error("No SMS selected")));
    }

    match app
        .database
        .delete_sms_batch(&payload.ids, &payload.phone_numbers)
    {
        Ok(deleted) => {
            schedule_sms_db_maintenance(&app, deleted);
            (
                StatusCode::OK,
                Json(ApiResponse::success_with_message(
                    "SMS batch deleted",
                    json!({ "deleted": deleted }),
                )),
            )
        }
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::error(format!("Failed: {}", e))),
        ),
    }
}

// ============ 系统信息 ============

/// 读取温度传感器数据
// ============ 电话功能 ============

async fn track_call_start(
    app: &AppState,
    path: &str,
    direction: &str,
    phone_number: &str,
    answered: bool,
) {
    if let Ok(id) = app.database.insert_call(direction, phone_number, answered) {
        let mut active = app.active_calls.lock().await;
        active.insert(
            path.to_string(),
            crate::state::ActiveCallRecord {
                id,
                answered_at: answered.then(std::time::Instant::now),
                answered,
            },
        );
    }
}

async fn mark_tracked_call_answered(app: &AppState, path: &str) {
    let mut active = app.active_calls.lock().await;
    if let Some(record) = active.get_mut(path) {
        record.answered = true;
        if record.answered_at.is_none() {
            record.answered_at = Some(std::time::Instant::now());
        }
    }
}

async fn finish_tracked_call(app: &AppState, path: &str, answered_now: bool) {
    let mut record = {
        let mut active = app.active_calls.lock().await;
        active.remove(path)
    };
    if let Some(ref mut record) = record {
        if answered_now && record.answered_at.is_none() {
            record.answered_at = Some(std::time::Instant::now());
        }
        let duration = record
            .answered_at
            .map(|at| at.elapsed().as_secs() as i64)
            .unwrap_or(0);
        let _ = app
            .database
            .update_call_end(record.id, duration, record.answered || answered_now);
    }
}

pub async fn get_calls_handler(State(app): State<AppState>) -> impl IntoResponse {
    match list_current_calls(&app.dbus_conn).await {
        Ok(data) => {
            for call in &data.calls {
                if matches!(call.state.as_str(), "active" | "held") {
                    mark_tracked_call_answered(&app, &call.path).await;
                }
            }
            (
                StatusCode::OK,
                Json(ApiResponse::success_with_message("Success", data)),
            )
        }
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<CallListResponse>::error(format!(
                "Failed: {}",
                e
            ))),
        ),
    }
}

pub async fn dial_call_handler(
    State(app): State<AppState>,
    Json(payload): Json<MakeCallRequest>,
) -> impl IntoResponse {
    let phone_number = payload.phone_number.trim().to_string();
    if phone_number.is_empty() {
        return (
            StatusCode::OK,
            Json(ApiResponse::<serde_json::Value>::error(
                "Phone number is required",
            )),
        );
    }
    match make_call(&app.dbus_conn, &phone_number).await {
        Ok(path) => {
            track_call_start(&app, &path, "outgoing", &phone_number, false).await;
            (
                StatusCode::OK,
                Json(ApiResponse::success_with_message(
                    "Call started",
                    json!({ "path": path }),
                )),
            )
        }
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<serde_json::Value>::error(format!(
                "Failed to dial: {}",
                e
            ))),
        ),
    }
}

pub async fn hangup_call_handler(
    State(app): State<AppState>,
    Json(payload): Json<HangupCallRequest>,
) -> impl IntoResponse {
    let before = get_call_by_path(&app.dbus_conn, &payload.path).await.ok();
    match hangup_call(&app.dbus_conn, &payload.path).await {
        Ok(()) => {
            let answered = before
                .as_ref()
                .map(|call| call.state == "active" || call.state == "held")
                .unwrap_or(false);
            finish_tracked_call(&app, &payload.path, answered).await;
            if let Some(call) = before {
                if call.direction == "incoming"
                    && matches!(call.state.as_str(), "incoming" | "waiting")
                {
                    let _ = app
                        .database
                        .insert_call("missed", &call.phone_number, false);
                }
            }
            (
                StatusCode::OK,
                Json(ApiResponse::success_with_message("Call hung up", json!({}))),
            )
        }
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<serde_json::Value>::error(format!(
                "Failed to hang up: {}",
                e
            ))),
        ),
    }
}

pub async fn hangup_all_calls_handler(State(app): State<AppState>) -> impl IntoResponse {
    let before = list_current_calls(&app.dbus_conn).await.ok();
    match hangup_all_calls(&app.dbus_conn).await {
        Ok(()) => {
            if let Some(list) = before {
                for call in list.calls {
                    let answered = call.state == "active" || call.state == "held";
                    finish_tracked_call(&app, &call.path, answered).await;
                    if call.direction == "incoming"
                        && matches!(call.state.as_str(), "incoming" | "waiting")
                    {
                        let _ = app
                            .database
                            .insert_call("missed", &call.phone_number, false);
                    }
                }
            }
            (
                StatusCode::OK,
                Json(ApiResponse::success_with_message(
                    "All calls hung up",
                    json!({}),
                )),
            )
        }
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<serde_json::Value>::error(format!(
                "Failed to hang up calls: {}",
                e
            ))),
        ),
    }
}

pub async fn answer_call_handler(
    State(app): State<AppState>,
    Json(payload): Json<HangupCallRequest>,
) -> impl IntoResponse {
    let before = get_call_by_path(&app.dbus_conn, &payload.path).await.ok();
    match answer_call(&app.dbus_conn, &payload.path).await {
        Ok(()) => {
            if let Some(call) = before {
                track_call_start(&app, &payload.path, "incoming", &call.phone_number, true).await;
                mark_tracked_call_answered(&app, &payload.path).await;
            }
            (
                StatusCode::OK,
                Json(ApiResponse::success_with_message(
                    "Call answered",
                    json!({}),
                )),
            )
        }
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<serde_json::Value>::error(format!(
                "Failed to answer call: {}",
                e
            ))),
        ),
    }
}

pub async fn get_call_history_handler(
    State(db): State<Arc<Database>>,
    Query(params): Query<CallHistoryRequest>,
) -> (StatusCode, Json<ApiResponse<CallHistoryResponse>>) {
    let limit = if params.limit > 0 { params.limit } else { 50 };
    let offset = if params.offset >= 0 { params.offset } else { 0 };
    match (db.get_call_history(limit, offset), db.get_call_stats()) {
        (Ok(records), Ok(stats)) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "Success",
                CallHistoryResponse { records, stats },
            )),
        ),
        (Err(e), _) | (_, Err(e)) => (
            StatusCode::OK,
            Json(ApiResponse::<CallHistoryResponse>::error(format!(
                "Failed: {}",
                e
            ))),
        ),
    }
}

pub async fn delete_call_history_handler(
    State(db): State<Arc<Database>>,
    Path(id): Path<i64>,
) -> (StatusCode, Json<ApiResponse<serde_json::Value>>) {
    match db.delete_call(id) {
        Ok(()) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "Call record deleted",
                json!({}),
            )),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::error(format!("Failed: {}", e))),
        ),
    }
}

pub async fn clear_call_history_handler(
    State(db): State<Arc<Database>>,
) -> (StatusCode, Json<ApiResponse<serde_json::Value>>) {
    match db.clear_all_calls() {
        Ok(()) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "Call history cleared",
                json!({}),
            )),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::error(format!("Failed: {}", e))),
        ),
    }
}

pub async fn get_call_settings_handler(State(conn): State<Arc<Connection>>) -> impl IntoResponse {
    match get_call_settings(&conn).await {
        Ok(data) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message("Success", data)),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<CallSettingsResponse>::error(format!(
                "Failed: {}",
                e
            ))),
        ),
    }
}

pub async fn set_call_settings_handler(
    State(conn): State<Arc<Connection>>,
    Json(payload): Json<SetCallSettingRequest>,
) -> impl IntoResponse {
    if payload.property != "VoiceCallWaiting" {
        return (
            StatusCode::OK,
            Json(ApiResponse::<serde_json::Value>::error(
                "Only VoiceCallWaiting is supported by ModemManager",
            )),
        );
    }
    let enabled = matches!(payload.value.as_str(), "enabled" | "on" | "true" | "1");
    match set_call_waiting(&conn, enabled).await {
        Ok(()) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "Call setting updated",
                json!({}),
            )),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<serde_json::Value>::error(format!(
                "Failed to update call setting: {}",
                e
            ))),
        ),
    }
}

pub async fn get_call_volume_handler() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse::<CallVolumeResponse>::error(
            "Call volume control is not exposed by ModemManager on this backend",
        )),
    )
}

pub async fn set_call_volume_handler(
    Json(payload): Json<SetCallVolumeRequest>,
) -> impl IntoResponse {
    let _ = (
        payload.speaker_volume,
        payload.microphone_volume,
        payload.muted,
    );
    (
        StatusCode::OK,
        Json(ApiResponse::<CallVolumeResponse>::error(
            "Call volume control is not exposed by ModemManager on this backend",
        )),
    )
}

pub async fn get_call_forwarding_handler() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse::<CallForwardingResponse>::error(
            "Call forwarding is not exposed by ModemManager on this backend",
        )),
    )
}

pub async fn set_call_forwarding_handler(
    Json(payload): Json<SetCallForwardingRequest>,
) -> impl IntoResponse {
    let _ = (payload.forward_type, payload.number, payload.timeout);
    (
        StatusCode::OK,
        Json(ApiResponse::<CallForwardingResponse>::error(
            "Call forwarding is not exposed by ModemManager on this backend",
        )),
    )
}

pub async fn get_ims_status_handler() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse::<ImsStatusResponse>::error(
            "IMS status is not exposed by ModemManager on this backend",
        )),
    )
}

pub async fn get_voicemail_status_handler() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse::<VoicemailStatusResponse>::error(
            "Voicemail status is not exposed by ModemManager on this backend",
        )),
    )
}

pub(crate) fn temperature_sensor_label(sensor_type: &str, zone: &str) -> String {
    let source = if sensor_type.trim().is_empty() {
        if zone.trim().is_empty() {
            "unknown"
        } else {
            zone.trim()
        }
    } else {
        sensor_type.trim()
    };
    let normalized = source.to_ascii_lowercase().replace('_', "-");

    if ["modem", "baseband", "wwan", "qmi", "mhi"]
        .iter()
        .any(|pattern| normalized.contains(pattern))
    {
        return "基带".to_string();
    }
    if ["gpu", "adreno"]
        .iter()
        .any(|pattern| normalized.contains(pattern))
    {
        return "GPU".to_string();
    }
    if ["camera", "cam", "isp"]
        .iter()
        .any(|pattern| normalized.contains(pattern))
    {
        return "摄像头".to_string();
    }
    if ["wifi", "wlan"]
        .iter()
        .any(|pattern| normalized.contains(pattern))
    {
        return "Wi-Fi".to_string();
    }
    if ["battery", "batt"]
        .iter()
        .any(|pattern| normalized.contains(pattern))
    {
        return "电池".to_string();
    }
    if ["charger", "charge"]
        .iter()
        .any(|pattern| normalized.contains(pattern))
    {
        return "充电".to_string();
    }
    if ["pmic", "power"]
        .iter()
        .any(|pattern| normalized.contains(pattern))
    {
        return "电源管理".to_string();
    }
    if ["soc", "tsens"]
        .iter()
        .any(|pattern| normalized.contains(pattern))
    {
        return "SoC".to_string();
    }
    if ["skin", "shell", "case"]
        .iter()
        .any(|pattern| normalized.contains(pattern))
    {
        return "外壳".to_string();
    }
    if ["ambient", "board"]
        .iter()
        .any(|pattern| normalized.contains(pattern))
    {
        return "环境".to_string();
    }

    if let Some((first, second)) = extract_number_range_after(&normalized, "cpu") {
        return second
            .map(|second| format!("CPU {first}-{second}"))
            .unwrap_or_else(|| format!("CPU {first}"));
    }
    if normalized.contains("cpu") {
        return "CPU".to_string();
    }

    if let Some((first, second)) = extract_number_range_after(&normalized, "core") {
        return second
            .map(|second| format!("核心 {first}-{second}"))
            .unwrap_or_else(|| format!("核心 {first}"));
    }
    if normalized.contains("core") {
        return "核心".to_string();
    }

    let cleaned = source
        .replace(|ch: char| matches!(ch, '-' | '_' | ' '), " ")
        .split_whitespace()
        .filter(|part| {
            !matches!(
                part.to_ascii_lowercase().as_str(),
                "thermal" | "therm" | "temperature" | "temp" | "sensor" | "zone"
            )
        })
        .collect::<Vec<_>>()
        .join(" ");

    if cleaned.is_empty() {
        source.to_string()
    } else {
        cleaned
    }
}

fn extract_number_range_after(value: &str, prefix: &str) -> Option<(String, Option<String>)> {
    let start = value.find(prefix)? + prefix.len();
    let chars = value[start..].char_indices();
    let mut first_start = None;
    for (index, ch) in chars {
        if ch.is_ascii_digit() {
            first_start = Some(start + index);
            break;
        }
    }
    let first_start = first_start?;
    let first_end = value[first_start..]
        .char_indices()
        .find_map(|(index, ch)| (!ch.is_ascii_digit()).then_some(first_start + index))
        .unwrap_or(value.len());
    let first = value[first_start..first_end].to_string();

    let after_first = &value[first_end..];
    let mut second_start = None;
    for (index, ch) in after_first.char_indices() {
        if ch.is_ascii_digit() {
            second_start = Some(first_end + index);
            break;
        }
        if ch.is_ascii_alphabetic() {
            break;
        }
    }
    let Some(second_start) = second_start else {
        return Some((first, None));
    };
    let second_end = value[second_start..]
        .char_indices()
        .find_map(|(index, ch)| (!ch.is_ascii_digit()).then_some(second_start + index))
        .unwrap_or(value.len());
    Some((first, Some(value[second_start..second_end].to_string())))
}

pub(crate) fn read_temperature_sensors() -> Vec<ThermalZone> {
    use std::fs;
    use std::path::Path;

    let thermal_path = Path::new("/sys/class/thermal");
    let mut sensors = Vec::new();

    if let Ok(entries) = fs::read_dir(thermal_path) {
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();

            if name.starts_with("thermal_zone") {
                let zone_path = entry.path();
                let sensor_type = fs::read_to_string(zone_path.join("type"))
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
                let temperature = fs::read_to_string(zone_path.join("temp"))
                    .ok()
                    .and_then(|s| s.trim().parse::<i32>().ok())
                    .map(|t| t as f64 / 1000.0)
                    .unwrap_or(0.0);

                let label = temperature_sensor_label(&sensor_type, &name);
                sensors.push(ThermalZone {
                    zone: name.to_string(),
                    sensor_type,
                    label,
                    temperature,
                });
            }
        }
    }
    sensors.sort_by(|a, b| a.zone.cmp(&b.zone));
    sensors
}

static SYSTEM_STATS_SNAPSHOT: OnceLock<Arc<RwLock<Option<SystemStatsResponse>>>> = OnceLock::new();
const SYSTEM_STATS_LOW_FREQUENCY_REFRESH_SECS: u64 = 10;

#[derive(Default)]
struct SystemStatsSamplerState {
    previous_network: HashMap<String, (u64, u64)>,
    last_low_frequency_refresh: Option<Instant>,
    memory: MemoryInfo,
    disk: Vec<DiskInfo>,
    uptime: UptimeInfo,
    system_info: SystemInfo,
    temperature: Vec<ThermalZone>,
}

fn system_stats_snapshot() -> Arc<RwLock<Option<SystemStatsResponse>>> {
    Arc::clone(SYSTEM_STATS_SNAPSHOT.get_or_init(|| Arc::new(RwLock::new(None))))
}

async fn collect_system_stats_snapshot(
    dbus_conn: &Connection,
    state: &mut SystemStatsSamplerState,
    interval_seconds: f64,
) -> Result<SystemStatsResponse, String> {
    let interfaces =
        get_active_interfaces().map_err(|e| format!("Failed to get interfaces: {}", e))?;

    let mut speed_data = Vec::new();
    let elapsed = interval_seconds.max(0.001);
    for iface in &interfaces {
        if let Ok((rx, tx)) = read_interface_stats(iface, Some(dbus_conn)).await {
            let (rx_speed, tx_speed) = state
                .previous_network
                .get(iface)
                .map(|(prev_rx, prev_tx)| {
                    (
                        (rx.saturating_sub(*prev_rx) as f64 / elapsed) as u64,
                        (tx.saturating_sub(*prev_tx) as f64 / elapsed) as u64,
                    )
                })
                .unwrap_or((0, 0));
            speed_data.push(NetworkSpeed {
                interface: iface.clone(),
                rx_bytes_per_sec: rx_speed,
                tx_bytes_per_sec: tx_speed,
                total_rx_bytes: rx,
                total_tx_bytes: tx,
            });
            state.previous_network.insert(iface.clone(), (rx, tx));
        }
    }
    state
        .previous_network
        .retain(|iface, _| interfaces.iter().any(|current| current == iface));

    let cpu_usage = sample_cpu_usage().await.unwrap_or(0.0);
    let mut cpu_load = read_cpu_load_sync().unwrap_or_default();
    cpu_load.load_percent = cpu_usage;

    let should_refresh_low_frequency = state
        .last_low_frequency_refresh
        .map(|last| last.elapsed() >= Duration::from_secs(SYSTEM_STATS_LOW_FREQUENCY_REFRESH_SECS))
        .unwrap_or(true);
    if should_refresh_low_frequency {
        let (total, available, cached, buffers) = read_memory_info()?;
        let used = total.saturating_sub(available);
        let used_percent = if total > 0 {
            (used as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        let (uptime, idle) = read_uptime()?;
        state.memory = MemoryInfo {
            total_bytes: total,
            available_bytes: available,
            used_bytes: used,
            used_percent,
            cached_bytes: cached,
            buffers_bytes: buffers,
        };
        state.disk = read_disk_info();
        state.uptime = UptimeInfo {
            uptime_seconds: uptime,
            idle_seconds: idle,
            uptime_formatted: format_uptime(uptime),
        };
        state.system_info = read_system_info()?;
        state.temperature = read_temperature_sensors();
        state.last_low_frequency_refresh = Some(Instant::now());
    }

    Ok(SystemStatsResponse {
        network_speed: NetworkSpeedResponse {
            interfaces: speed_data,
            interval_seconds: elapsed,
        },
        memory: state.memory.clone(),
        disk: state.disk.clone(),
        cpu_load,
        uptime: state.uptime.clone(),
        system_info: state.system_info.clone(),
        temperature: state.temperature.clone(),
    })
}

pub fn spawn_system_stats_sampler(dbus_conn: Arc<Connection>) {
    let snapshot = system_stats_snapshot();
    tokio::spawn(async move {
        let mut sampler_state = SystemStatsSamplerState::default();
        let mut last_sample = Instant::now();
        loop {
            let elapsed = last_sample.elapsed().as_secs_f64().max(1.0);
            last_sample = Instant::now();
            match collect_system_stats_snapshot(&dbus_conn, &mut sampler_state, elapsed).await {
                Ok(stats) => {
                    *snapshot.write().await = Some(stats);
                }
                Err(err) => warn!(error = %err, "Failed to sample system stats"),
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
}

/// GET /api/stats
pub async fn get_system_stats(State(_dbus_conn): State<Arc<Connection>>) -> impl IntoResponse {
    let snapshot = system_stats_snapshot();
    if let Some(data) = snapshot.read().await.clone() {
        return (
            StatusCode::OK,
            Json(ApiResponse::success_with_message("Success", data)),
        );
    }

    (
        StatusCode::OK,
        Json(ApiResponse::success_with_message(
            "No system stats sample yet",
            SystemStatsResponse::default(),
        )),
    )
}

/// GET /api/stats/cpu
pub async fn get_cpu_info() -> impl IntoResponse {
    match read_cpu_info() {
        Ok(info) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message("Success", info)),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<CpuInfo>::error(format!("Failed: {}", e))),
        ),
    }
}

/// GET /api/connectivity
pub async fn get_connectivity_check() -> (StatusCode, Json<ApiResponse<ConnectivityCheckResponse>>)
{
    // 两个 ping 并行执行，超时从 2s 缩短到 1s
    let (ipv4_result, ipv6_result) = tokio::join!(
        async_ping_host("223.5.5.5", false),
        async_ping_host("2400:3200::1", true),
    );
    (
        StatusCode::OK,
        Json(ApiResponse::success_with_message(
            "Connectivity check completed",
            ConnectivityCheckResponse {
                ipv4: ipv4_result,
                ipv6: ipv6_result,
            },
        )),
    )
}

pub(crate) async fn async_ping_host(target: &str, is_ipv6: bool) -> PingResult {
    let cmd = if is_ipv6 { "ping6" } else { "ping" };
    let output = tokio::process::Command::new(cmd)
        .args(["-c", "1", "-W", "1", target])
        .output()
        .await;
    match output {
        Ok(result) => {
            if result.status.success() {
                let stdout = String::from_utf8_lossy(&result.stdout);
                let latency = parse_ping_latency(&stdout);
                PingResult {
                    success: true,
                    latency_ms: latency,
                    target: target.to_string(),
                    error: None,
                }
            } else {
                let stderr = String::from_utf8_lossy(&result.stderr);
                PingResult {
                    success: false,
                    latency_ms: None,
                    target: target.to_string(),
                    error: Some(if stderr.is_empty() {
                        "Unreachable".to_string()
                    } else {
                        stderr.trim().to_string()
                    }),
                }
            }
        }
        Err(e) => PingResult {
            success: false,
            latency_ms: None,
            target: target.to_string(),
            error: Some(format!("Failed: {}", e)),
        },
    }
}

fn parse_ping_latency(output: &str) -> Option<f64> {
    for line in output.lines() {
        if let Some(time_pos) = line.find("time=") {
            let after_time = &line[time_pos + 5..];
            let num_str: String = after_time
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            if let Ok(latency) = num_str.parse::<f64>() {
                return Some(latency);
            }
        }
    }
    None
}

/// POST /api/system/reboot
pub async fn system_reboot(
    State(app): State<AppState>,
    Json(payload): Json<SystemRebootRequest>,
) -> impl IntoResponse {
    let delay = payload.delay_seconds;
    app.system_event_emitter
        .emit_code(
            system_event_codes::SYSTEM_SERVICE_REBOOT_REQUESTED,
            system_event_severity::WARNING,
            system_event_status::TRIGGERED,
            "system",
            format!("用户触发系统重启，延迟 {} 秒执行", delay),
        )
        .await;
    let system_events = Arc::clone(&app.system_event_emitter);
    tokio::spawn(async move {
        run_safe_os_reboot_sequence(delay, system_events).await;
    });
    (
        StatusCode::OK,
        Json(ApiResponse::success_with_message(
            format!("System will perform safe OS reboot in {} seconds", delay),
            json!({ "delay_seconds": delay }),
        )),
    )
}

pub async fn run_safe_os_reboot_sequence(
    delay_seconds: u32,
    system_events: Arc<crate::system_event::SystemEventEmitter>,
) {
    if delay_seconds > 0 {
        tokio::time::sleep(tokio::time::Duration::from_secs(delay_seconds as u64)).await;
    }

    info!("Starting safe OS reboot sequence");

    if let Some(message) =
        run_reboot_prep_command("disable modem radio", "mmcli", &["-m", "0", "-d"], false)
    {
        system_events
            .emit_code(
                system_event_codes::SYSTEM_SERVICE_REBOOT_PREP_FAILED,
                system_event_severity::WARNING,
                system_event_status::FAILED,
                "disable modem radio",
                message,
            )
            .await;
    }
    if let Some(message) = run_reboot_prep_command(
        "stop ModemManager IPC service",
        "systemctl",
        &["stop", "ModemManager"],
        false,
    ) {
        system_events
            .emit_code(
                system_event_codes::SYSTEM_SERVICE_REBOOT_PREP_FAILED,
                system_event_severity::WARNING,
                system_event_status::FAILED,
                "stop ModemManager IPC service",
                message,
            )
            .await;
    }
    let _ = run_reboot_prep_command("stop qmi-proxy", "killall", &["qmi-proxy"], true);
    cleanup_modemmanager_runtime_cache();
    if let Some(message) = run_reboot_prep_command("flush filesystem cache", "sync", &[], false) {
        system_events
            .emit_code(
                system_event_codes::SYSTEM_SERVICE_REBOOT_PREP_FAILED,
                system_event_severity::WARNING,
                system_event_status::FAILED,
                "flush filesystem cache",
                message,
            )
            .await;
    }

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    info!("Safe OS reboot preparation complete, executing reboot");
    if let Err(err) = Command::new("reboot").output() {
        error!(error = %err, "Failed to execute reboot command");
    }
}

fn run_reboot_prep_command(
    label: &str,
    program: &str,
    args: &[&str],
    allow_failure: bool,
) -> Option<String> {
    match Command::new(program).args(args).output() {
        Ok(output) if output.status.success() => {
            info!(step = label, "Safe OS reboot step completed");
            None
        }
        Ok(output) => {
            let severity = if allow_failure {
                "optional"
            } else {
                "required"
            };
            warn_reboot_prep_failure(label, program, severity, &output);
            if allow_failure {
                None
            } else {
                Some(format!(
                    "重启预处理步骤失败: {label}; command={program}; status={}",
                    output.status
                ))
            }
        }
        Err(err) if allow_failure => {
            warn!(step = label, command = program, error = %err, "Optional safe OS reboot step failed");
            None
        }
        Err(err) => {
            warn!(step = label, command = program, error = %err, "Safe OS reboot step failed");
            Some(format!(
                "重启预处理步骤失败: {label}; command={program}; error={err}"
            ))
        }
    }
}

fn cleanup_modemmanager_runtime_cache() {
    const CACHE_DIR: &str = "/var/lib/ModemManager";

    match fs::read_dir(CACHE_DIR) {
        Ok(entries) => {
            let mut removed = 0usize;
            for entry in entries {
                match entry {
                    Ok(entry) => {
                        let path = entry.path();
                        let result = if path.is_dir() {
                            fs::remove_dir_all(&path)
                        } else {
                            fs::remove_file(&path)
                        };

                        match result {
                            Ok(()) => removed += 1,
                            Err(err) => warn!(
                                path = %path.display(),
                                error = %err,
                                "Failed to remove ModemManager runtime cache entry"
                            ),
                        }
                    }
                    Err(err) => warn!(
                        directory = CACHE_DIR,
                        error = %err,
                        "Failed to read ModemManager runtime cache entry"
                    ),
                }
            }
            info!(
                directory = CACHE_DIR,
                removed, "ModemManager runtime cache cleanup completed"
            );
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            info!(
                directory = CACHE_DIR,
                "ModemManager runtime cache directory does not exist"
            );
        }
        Err(err) => {
            warn!(
                directory = CACHE_DIR,
                error = %err,
                "Failed to open ModemManager runtime cache directory"
            );
        }
    }
}

fn warn_reboot_prep_failure(label: &str, program: &str, severity: &str, output: &Output) {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    warn!(
        step = label,
        command = program,
        severity = severity,
        status = %output.status,
        stderr = %stderr,
        stdout = %stdout,
        "Safe OS reboot step returned non-zero status"
    );
}

// ============ 通知配置 ============

pub async fn restart_service_handler(State(app): State<AppState>) -> impl IntoResponse {
    app.system_event_emitter
        .emit_code(
            system_event_codes::SYSTEM_SERVICE_SIMADMIN_RESTART_REQUESTED,
            system_event_severity::WARNING,
            system_event_status::TRIGGERED,
            "simadmin",
            "用户触发 SimAdmin 服务重启",
        )
        .await;
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        let _ = Command::new("systemctl")
            .args(["restart", "simadmin"])
            .output();
    });
    (
        StatusCode::OK,
        Json(ApiResponse::success_with_message(
            "SimAdmin service will restart",
            json!({}),
        )),
    )
}

use crate::config::ConfigManager;
use crate::notification::NotificationSender;

#[derive(Debug, Default, Deserialize)]
pub struct NotificationLogQuery {
    #[serde(default, rename = "type")]
    pub event_type: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub start_date: String,
    #[serde(default)]
    pub end_date: String,
    #[serde(default = "default_notification_log_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

#[derive(Debug, Default, Deserialize)]
pub struct NotificationLogClearRequest {
    #[serde(default, rename = "type")]
    pub event_type: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub start_date: String,
    #[serde(default)]
    pub end_date: String,
}

fn default_notification_log_limit() -> i64 {
    50
}

/// GET /api/notifications/config
pub async fn get_notification_config_handler(
    State(config_manager): State<Arc<ConfigManager>>,
) -> (
    StatusCode,
    Json<ApiResponse<crate::config::NotificationConfig>>,
) {
    let config = config_manager.get_notifications();
    (
        StatusCode::OK,
        Json(ApiResponse::success_with_message("Success", config)),
    )
}

/// POST /api/notifications/config
pub async fn set_notification_config_handler(
    State(config_manager): State<Arc<ConfigManager>>,
    Json(notification_config): Json<crate::config::NotificationConfig>,
) -> (StatusCode, Json<ApiResponse<serde_json::Value>>) {
    match config_manager.set_notifications(notification_config) {
        Ok(_) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "Notification config updated",
                json!({}),
            )),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::error(format!("Failed: {}", e))),
        ),
    }
}

/// POST /api/notifications/test/{channel}
pub async fn test_notification_channel_handler(
    Path(channel): Path<String>,
    State(notification_sender): State<Arc<NotificationSender>>,
) -> (
    StatusCode,
    Json<ApiResponse<crate::models::WebhookTestResponse>>,
) {
    match notification_sender.test_channel(&channel).await {
        Ok(message) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "Notification test successful",
                WebhookTestResponse {
                    success: true,
                    message,
                },
            )),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "Notification test failed",
                WebhookTestResponse {
                    success: false,
                    message: e,
                },
            )),
        ),
    }
}

// ============ OTA 更新 ============

/// GET /api/notifications/logs
pub async fn get_notification_logs_handler(
    Query(query): Query<NotificationLogQuery>,
    State(database): State<Arc<Database>>,
) -> (
    StatusCode,
    Json<ApiResponse<crate::db::NotificationLogsResponse>>,
) {
    match database.get_notification_logs(
        &query.event_type,
        &query.status,
        &query.q,
        &query.start_date,
        &query.end_date,
        query.limit,
        query.offset,
    ) {
        Ok(logs) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message("Success", logs)),
        ),
        Err(err) => (
            StatusCode::OK,
            Json(ApiResponse::error(format!("Failed: {}", err))),
        ),
    }
}

/// POST /api/notifications/logs/clear
pub async fn clear_notification_logs_handler(
    State(database): State<Arc<Database>>,
    payload: Option<Json<NotificationLogClearRequest>>,
) -> (StatusCode, Json<ApiResponse<serde_json::Value>>) {
    let filters = payload.map(|Json(value)| value).unwrap_or_default();
    match database.clear_notification_logs(
        &filters.event_type,
        &filters.status,
        &filters.start_date,
        &filters.end_date,
    ) {
        Ok(deleted) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "Notification logs cleared",
                json!({ "deleted": deleted }),
            )),
        ),
        Err(err) => (
            StatusCode::OK,
            Json(ApiResponse::error(format!("Failed: {}", err))),
        ),
    }
}

/// GET /api/ota/status
pub async fn get_ota_status_handler() -> impl IntoResponse {
    let status = crate::ota::get_ota_status();
    (
        StatusCode::OK,
        Json(ApiResponse::success_with_message("Success", status)),
    )
}

/// POST /api/ota/upload
pub async fn upload_ota_handler(body: axum::body::Bytes) -> impl IntoResponse {
    match crate::ota::handle_ota_upload(&body) {
        Ok(response) => {
            let message = if response.validation.valid {
                "OTA uploaded and validated"
            } else {
                "OTA uploaded but validation failed"
            };
            (
                StatusCode::OK,
                Json(ApiResponse::success_with_message(message, response)),
            )
        }
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<crate::models::OtaUploadResponse>::error(
                format!("Failed: {}", e),
            )),
        ),
    }
}

/// POST /api/ota/latest-release
pub async fn get_latest_ota_release_handler(
    Json(req): Json<crate::models::OtaOnlinePrepareRequest>,
) -> impl IntoResponse {
    let result: Result<crate::models::OtaLatestReleaseResponse, String> = async {
        let include_builtin_proxies = req
            .proxy_prefix
            .as_ref()
            .map(|prefix| !prefix.trim().is_empty())
            .unwrap_or(false);
        let proxy_prefix = crate::ota::normalize_proxy_prefix(req.proxy_prefix);
        let client = crate::ota::build_ota_http_client()?;

        crate::ota::fetch_latest_github_release(&client, &proxy_prefix, include_builtin_proxies)
            .await
    }
    .await;

    match result {
        Ok(release) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message("Success", release)),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<crate::models::OtaLatestReleaseResponse>::error(format!(
                "Failed: {}. GitHub may have rate-limited this request; try again later or enable a proxy.",
                e
            ))),
        ),
    }
}

/// POST /api/ota/online-prepare
pub async fn prepare_online_ota_handler(
    Json(req): Json<crate::models::OtaOnlinePrepareRequest>,
) -> impl IntoResponse {
    let result: Result<crate::models::OtaUploadResponse, String> = async {
        let include_builtin_proxies = req
            .proxy_prefix
            .as_ref()
            .map(|prefix| !prefix.trim().is_empty())
            .unwrap_or(false);
        let proxy_prefix = crate::ota::normalize_proxy_prefix(req.proxy_prefix);
        let client = crate::ota::build_ota_http_client()?;

        let release = crate::ota::fetch_latest_github_release(
            &client,
            &proxy_prefix,
            include_builtin_proxies,
        )
        .await?;

        let asset = crate::ota::supported_release_asset(&release)
            .ok_or_else(|| "No supported OTA asset found in latest release".to_string())?;

        if asset.size > crate::ota::MAX_OTA_BYTES {
            return Err(format!(
                "OTA asset is too large: {} bytes exceeds {} bytes",
                asset.size,
                crate::ota::MAX_OTA_BYTES
            ));
        }

        let bytes = crate::ota::download_ota_asset_bytes(
            &client,
            &proxy_prefix,
            include_builtin_proxies,
            asset,
        )
        .await?;

        crate::ota::handle_ota_upload(&bytes)
    }
    .await;

    match result {
        Ok(response) => {
            let message = if response.validation.valid {
                "Online OTA downloaded and validated"
            } else {
                "Online OTA downloaded but validation failed"
            };
            (
                StatusCode::OK,
                Json(ApiResponse::success_with_message(message, response)),
            )
        }
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<crate::models::OtaUploadResponse>::error(
                format!("Failed: {}", e),
            )),
        ),
    }
}

/// POST /api/ota/apply
pub async fn apply_ota_handler(
    Json(req): Json<crate::models::OtaApplyRequest>,
) -> impl IntoResponse {
    match crate::ota::apply_ota_update(req.restart_now) {
        Ok(message) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                &message,
                json!({ "applied": true }),
            )),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<serde_json::Value>::error(format!(
                "Failed: {}",
                e
            ))),
        ),
    }
}

/// POST /api/ota/cancel
pub async fn cancel_ota_handler() -> impl IntoResponse {
    match crate::ota::cancel_pending_update() {
        Ok(()) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "Update cancelled",
                json!({}),
            )),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::<serde_json::Value>::error(format!(
                "Failed: {}",
                e
            ))),
        ),
    }
}

fn default_log_limit() -> i64 {
    100
}

#[derive(Debug, Deserialize)]
pub struct AutomationLogQuery {
    #[serde(default, rename = "type")]
    pub task_type: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub start_date: String,
    #[serde(default)]
    pub end_date: String,
    #[serde(default = "default_log_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

#[derive(Debug, Deserialize, Default)]
pub struct AutomationLogClearRequest {
    #[serde(default, rename = "type")]
    pub task_type: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub start_date: String,
    #[serde(default)]
    pub end_date: String,
}

/// GET /api/automation/config
pub async fn get_automation_config_handler(
    State(config_manager): State<Arc<ConfigManager>>,
) -> (
    StatusCode,
    Json<ApiResponse<crate::config::AutomationConfig>>,
) {
    let config = config_manager.get_automation_config();
    (
        StatusCode::OK,
        Json(ApiResponse::success_with_message("Success", config)),
    )
}

/// POST /api/automation/config
pub async fn set_automation_config_handler(
    State(config_manager): State<Arc<ConfigManager>>,
    Json(config): Json<crate::config::AutomationConfig>,
) -> (StatusCode, Json<ApiResponse<serde_json::Value>>) {
    match config_manager.set_automation_config(config) {
        Ok(_) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "Automation config updated",
                json!({}),
            )),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ApiResponse::error(format!("Failed: {}", e))),
        ),
    }
}

/// GET /api/automation/logs
pub async fn get_automation_logs_handler(
    Query(query): Query<AutomationLogQuery>,
    State(database): State<Arc<Database>>,
) -> (
    StatusCode,
    Json<ApiResponse<crate::db::AutomationLogsResponse>>,
) {
    match database.get_automation_logs(
        &query.task_type,
        &query.status,
        &query.q,
        &query.start_date,
        &query.end_date,
        query.limit,
        query.offset,
    ) {
        Ok(logs) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message("Success", logs)),
        ),
        Err(err) => (
            StatusCode::OK,
            Json(ApiResponse::error(format!("Failed: {}", err))),
        ),
    }
}

/// POST /api/automation/logs/clear
pub async fn clear_automation_logs_handler(
    State(database): State<Arc<Database>>,
    payload: Option<Json<AutomationLogClearRequest>>,
) -> (StatusCode, Json<ApiResponse<serde_json::Value>>) {
    let filters = payload.map(|Json(value)| value).unwrap_or_default();
    match database.clear_automation_logs(
        &filters.task_type,
        &filters.status,
        &filters.start_date,
        &filters.end_date,
    ) {
        Ok(deleted) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "Automation logs cleared",
                json!({ "deleted": deleted }),
            )),
        ),
        Err(err) => (
            StatusCode::OK,
            Json(ApiResponse::error(format!("Failed: {}", err))),
        ),
    }
}

/// POST /api/automation/test/{task_id}
pub async fn test_automation_task_handler(
    Path(task_id): Path<String>,
    State(app_state): State<AppState>,
) -> (StatusCode, Json<ApiResponse<serde_json::Value>>) {
    let config = app_state.config_manager.get_automation_config();
    let task = config.tasks.iter().find(|t| t.id == task_id).cloned();

    let Some(task) = task else {
        return (StatusCode::OK, Json(ApiResponse::error("自动化任务不存在")));
    };

    tokio::spawn(async move {
        let registry = crate::automation::tasks::TaskRegistry::new();
        let task_type = match &task.action {
            crate::config::AutomationAction::RestartBaseband => "restart_baseband",
            crate::config::AutomationAction::RebootDevice { .. } => "reboot_device",
            crate::config::AutomationAction::BackupData { .. } => "backup_data",
            crate::config::AutomationAction::SendSms { .. } => "send_sms",
        };

        let handler = match registry.get(task_type) {
            Some(h) => h,
            None => {
                let err_msg = format!("未找到该任务类型的处理器: {}", task_type);
                let _ = app_state
                    .database
                    .insert_automation_log(&task.id, &task.name, task_type, "failed", &err_msg);
                return;
            }
        };

        let mut delay_secs = 0u64;
        let params = match &task.action {
            crate::config::AutomationAction::RestartBaseband => serde_json::Value::Null,
            crate::config::AutomationAction::RebootDevice { delay_seconds } => {
                serde_json::json!({ "delay_seconds": delay_seconds })
            }
            crate::config::AutomationAction::BackupData {
                components,
                storage,
            } => {
                serde_json::json!({
                    "components": components,
                    "storage": storage,
                })
            }
            crate::config::AutomationAction::SendSms {
                phone_number,
                content,
                random_delay_seconds,
                retry_limit,
            } => {
                delay_secs = u64::from(random_delay_seconds.unwrap_or(0));
                serde_json::json!({
                    "phone_number": phone_number,
                    "content": content,
                    "random_delay_seconds": random_delay_seconds,
                    "retry_limit": retry_limit
                })
            }
        };

        let result = tokio::time::timeout(
            tokio::time::Duration::from_secs(60 + delay_secs),
            handler.execute(&app_state, &params),
        )
        .await;

        let (status, detail) = match result {
            Ok(Ok(_)) => ("success", "执行成功".to_string()),
            Ok(Err(e)) => ("failed", format!("执行失败: {}", e)),
            Err(_) => ("failed", "执行超时 (超过60秒限制)".to_string()),
        };

        let _ = app_state
            .database
            .insert_automation_log(&task.id, &task.name, task_type, status, &detail);

        let event = crate::notification::AutomationEvent {
            task_id: task.id.clone(),
            task_name: task.name.clone(),
            task_type: task_type.to_string(),
            status: status.to_string(),
            message: detail.clone(),
            timestamp: crate::db::beijing_sms_now_string(),
        };

        let _ = app_state
            .notification_sender
            .forward_automation_event(&event)
            .await;
    });

    (
        StatusCode::OK,
        Json(ApiResponse::success_with_message(
            "任务已在后台下发立即执行",
            json!({}),
        )),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modem_manager::SimIdentity;

    #[test]
    fn enriches_enabled_esim_profile_from_current_sim_identity() {
        let mut profiles = vec![
            EsimProfile {
                iccid: "profile-a".to_string(),
                state: "disabled".to_string(),
                ..Default::default()
            },
            EsimProfile {
                iccid: "profile-b".to_string(),
                state: "disabled".to_string(),
                ..Default::default()
            },
        ];
        let identity = SimIdentity {
            iccid: "profile-b".to_string(),
            imsi: "234336".to_string(),
            operator_id: "234336".to_string(),
        };

        enrich_profiles_with_current_identity(&mut profiles, &identity);

        assert_eq!(profiles[1].state, "enabled");
        assert_eq!(profiles[1].imsi.as_deref(), Some("234336"));
        assert_eq!(profiles[1].mcc.as_deref(), Some("234"));
        assert_eq!(profiles[1].mnc.as_deref(), Some("336"));
        assert!(profiles[0].mcc.is_none());
    }

    #[test]
    fn enriches_unknown_cached_profile_states_from_current_sim_identity() {
        let mut profiles = vec![
            EsimProfile {
                iccid: "profile-a".to_string(),
                state: "unknown".to_string(),
                ..Default::default()
            },
            EsimProfile {
                iccid: "profile-b".to_string(),
                state: "unknown".to_string(),
                ..Default::default()
            },
        ];
        let identity = SimIdentity {
            iccid: "profile-b".to_string(),
            imsi: String::new(),
            operator_id: String::new(),
        };

        enrich_profiles_with_current_identity(&mut profiles, &identity);

        assert_eq!(profiles[0].state, "disabled");
        assert_eq!(profiles[1].state, "enabled");
    }

    #[test]
    fn sorts_esim_profiles_with_enabled_first_and_stable_fallback() {
        let mut profiles = vec![
            EsimProfile {
                iccid: "300".to_string(),
                name: "Charlie".to_string(),
                state: "disabled".to_string(),
                ..Default::default()
            },
            EsimProfile {
                iccid: "100".to_string(),
                name: "Alpha".to_string(),
                state: "disabled".to_string(),
                ..Default::default()
            },
            EsimProfile {
                iccid: "200".to_string(),
                name: "Bravo".to_string(),
                state: "enabled".to_string(),
                ..Default::default()
            },
        ];

        sort_esim_profiles_for_display(&mut profiles);

        let order: Vec<&str> = profiles
            .iter()
            .map(|profile| profile.iccid.as_str())
            .collect();
        assert_eq!(order, vec!["200", "100", "300"]);
    }

    #[test]
    fn splits_five_digit_operator_codes_for_profile_enrichment() {
        assert_eq!(
            split_profile_operator_code("46002"),
            ("460".to_string(), "02".to_string())
        );
    }

    #[test]
    fn labels_temperature_sensors_with_dashboard_names() {
        assert_eq!(temperature_sensor_label("modem-thermal", ""), "基带");
        assert_eq!(temperature_sensor_label("cpu0-1-thermal", ""), "CPU 0-1");
        assert_eq!(temperature_sensor_label("core2_3_temp", ""), "核心 2-3");
        assert_eq!(temperature_sensor_label("wifi_sensor", ""), "Wi-Fi");
    }
}
