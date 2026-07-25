use axum::{
    body::{Body, Bytes},
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use base64::{engine::general_purpose, Engine as _};
use chrono::{Local, TimeZone, Utc};
use ring::digest;
use rusqlite::{params, types::ValueRef, Row, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path as FsPath, PathBuf};
use zip::{write::FileOptions, ZipArchive, ZipWriter};

use crate::{
    config::{
        ApnConfig, AppConfig, AutomationConfig, BackupCleanupConfig, BackupConfig,
        BackupStorageConfig, DeviceNetworkConfig, EsimConfig, NotificationConfig, SecurityConfig,
        VersionUpdateNotificationConfig,
    },
    db::Database,
    models::{ApiResponse, WorkMode},
    state::AppState,
};

const FORMAT_VERSION: u32 = 1;
const APP_ID: &str = "simadmin";
const MANIFEST_PATH: &str = "manifest.json";
const DEFAULT_BACKUP_DIR: &str = "/opt/simadmin/backups";
const PRE_RESTORE_DIR_NAME: &str = "pre-restore";
const MAX_IMPORT_BYTES: usize = 50 * 1024 * 1024;
const DEFAULT_COMPONENTS: &[BackupComponent] = &[
    BackupComponent::Config,
    BackupComponent::Sms,
    BackupComponent::NotificationConfig,
    BackupComponent::AutomationConfig,
    BackupComponent::SimCache,
    BackupComponent::EsimCache,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupComponent {
    Config,
    Sms,
    NotificationConfig,
    NotificationLogs,
    NotificationQueue,
    AutomationConfig,
    AutomationLogs,
    SimCache,
    EsimCache,
    Auth,
}

impl BackupComponent {
    fn as_str(self) -> &'static str {
        match self {
            BackupComponent::Config => "config",
            BackupComponent::Sms => "sms",
            BackupComponent::NotificationConfig => "notification_config",
            BackupComponent::NotificationLogs => "notification_logs",
            BackupComponent::NotificationQueue => "notification_queue",
            BackupComponent::AutomationConfig => "automation_config",
            BackupComponent::AutomationLogs => "automation_logs",
            BackupComponent::SimCache => "sim_cache",
            BackupComponent::EsimCache => "esim_cache",
            BackupComponent::Auth => "auth",
        }
    }

    fn file_path(self) -> &'static str {
        match self {
            BackupComponent::Config => "components/config.json",
            BackupComponent::Sms => "components/sms.json",
            BackupComponent::NotificationConfig => "components/notification_config.json",
            BackupComponent::NotificationLogs => "components/notification_logs.json",
            BackupComponent::NotificationQueue => "components/notification_queue.json",
            BackupComponent::AutomationConfig => "components/automation_config.json",
            BackupComponent::AutomationLogs => "components/automation_logs.json",
            BackupComponent::SimCache => "components/sim_cache.json",
            BackupComponent::EsimCache => "components/esim_cache.json",
            BackupComponent::Auth => "components/auth.json",
        }
    }

    fn label(self) -> &'static str {
        match self {
            BackupComponent::Config => "系统配置",
            BackupComponent::Sms => "短信记录",
            BackupComponent::NotificationConfig => "通知配置",
            BackupComponent::NotificationLogs => "通知日志",
            BackupComponent::NotificationQueue => "通知队列",
            BackupComponent::AutomationConfig => "自动化配置",
            BackupComponent::AutomationLogs => "自动化日志",
            BackupComponent::SimCache => "SIM 缓存",
            BackupComponent::EsimCache => "eSIM 缓存",
            BackupComponent::Auth => "登录凭据",
        }
    }

    fn description(self) -> &'static str {
        match self {
            BackupComponent::Config => "APN、漫游、数据连接、DDNS、工作模式、备份设置等基础配置",
            BackupComponent::Sms => "短信收发历史记录",
            BackupComponent::NotificationConfig => "通知渠道、规则、模板和日志保留策略",
            BackupComponent::NotificationLogs => "通知发送历史日志",
            BackupComponent::NotificationQueue => "通知待重试和失败队列",
            BackupComponent::AutomationConfig => "自动化任务配置",
            BackupComponent::AutomationLogs => "自动化任务执行日志",
            BackupComponent::SimCache => "SMSC、本机号码和短信容量缓存",
            BackupComponent::EsimCache => {
                "eSIM Profile 缓存和 eUICC 缓存，不包含运营商 Profile 内容"
            }
            BackupComponent::Auth => "管理员密码哈希和安全策略，不包含会话",
        }
    }

    fn sensitive(self) -> bool {
        matches!(
            self,
            BackupComponent::Config
                | BackupComponent::Sms
                | BackupComponent::SimCache
                | BackupComponent::EsimCache
                | BackupComponent::Auth
        )
    }

    fn default_selected(self) -> bool {
        DEFAULT_COMPONENTS.contains(&self)
    }
}

fn all_components() -> Vec<BackupComponent> {
    vec![
        BackupComponent::Config,
        BackupComponent::Sms,
        BackupComponent::NotificationConfig,
        BackupComponent::NotificationLogs,
        BackupComponent::NotificationQueue,
        BackupComponent::AutomationConfig,
        BackupComponent::AutomationLogs,
        BackupComponent::SimCache,
        BackupComponent::EsimCache,
        BackupComponent::Auth,
    ]
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackupKind {
    #[default]
    Full,
    Slim,
}

impl BackupKind {
    fn as_str(self) -> &'static str {
        match self {
            BackupKind::Full => "full",
            BackupKind::Slim => "slim",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImportMode {
    Merge,
    Replace,
}

impl Default for ImportMode {
    fn default() -> Self {
        Self::Merge
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupComponentOption {
    pub key: String,
    pub label: String,
    pub description: String,
    pub default_selected: bool,
    pub sensitive: bool,
    pub records: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupOptionsResponse {
    pub format_version: u32,
    pub default_components: Vec<String>,
    pub components: Vec<BackupComponentOption>,
    pub local_dir: String,
    pub pre_restore_dir: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BackupExportRequest {
    pub components: Vec<BackupComponent>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BackupConfigRequest {
    pub config: BackupConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BackupImportApplyQuery {
    #[serde(default)]
    pub mode: ImportMode,
    #[serde(default)]
    pub components: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupFileManifest {
    pub sha256: String,
    pub records: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    pub format_version: u32,
    pub app: String,
    pub backup_kind: BackupKind,
    pub components: Vec<BackupComponent>,
    pub counts: BTreeMap<String, usize>,
    pub files: BTreeMap<String, BackupFileManifest>,
    pub created_at: String,
    pub simadmin_version: String,
    pub contains_sensitive_data: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct BackupExportLocalResponse {
    pub file: BackupLocalFile,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupImportComponentPreview {
    pub key: String,
    pub label: String,
    pub records: usize,
    pub sensitive: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct BackupImportPreview {
    pub filename: Option<String>,
    pub backup_kind: BackupKind,
    pub format_version: u32,
    pub simadmin_version: String,
    pub created_at: String,
    pub contains_sensitive_data: bool,
    pub components: Vec<BackupImportComponentPreview>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct BackupImportApplyResponse {
    pub imported_components: Vec<String>,
    pub backup_kind: BackupKind,
    pub mode: String,
    pub pre_restore_file: Option<BackupLocalFile>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct BackupLocalFile {
    pub name: String,
    pub size: u64,
    pub modified_at: String,
    pub backup_kind: Option<BackupKind>,
    pub components: Vec<String>,
    pub counts: BTreeMap<String, usize>,
    pub pre_restore: bool,
    pub valid: bool,
    pub error: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct BackupLocalFilesResponse {
    pub backups: Vec<BackupLocalFile>,
    pub pre_restore: Vec<BackupLocalFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackupBaseConfig {
    pub device_network: DeviceNetworkConfig,
    pub version_update_notifications: VersionUpdateNotificationConfig,
    pub roaming_allowed: bool,
    pub data_enabled: bool,
    pub apn: ApnConfig,
    pub work_mode: WorkMode,
    pub esim: EsimConfig,
    pub backup: BackupConfig,
}

impl From<AppConfig> for BackupBaseConfig {
    fn from(config: AppConfig) -> Self {
        Self {
            device_network: config.device_network,
            version_update_notifications: config.version_update_notifications,
            roaming_allowed: config.roaming_allowed,
            data_enabled: config.data_enabled,
            apn: config.apn,
            work_mode: config.work_mode,
            esim: config.esim,
            backup: config.backup,
        }
    }
}

impl BackupBaseConfig {
    fn apply_to(self, config: &mut AppConfig) {
        config.device_network = self.device_network;
        config.version_update_notifications = self.version_update_notifications;
        config.roaming_allowed = self.roaming_allowed;
        config.data_enabled = self.data_enabled;
        config.apn = self.apn;
        config.work_mode = self.work_mode;
        config.esim = self.esim;
        config.backup = normalize_backup_config(self.backup);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuthBackup {
    pub security: SecurityConfig,
    pub auth_config: Vec<Value>,
}

struct ComponentPayload {
    component: BackupComponent,
    value: Value,
    count: usize,
}

struct ParsedBackup {
    manifest: BackupManifest,
    files: BTreeMap<String, Vec<u8>>,
}

pub async fn get_backup_options_handler(
    State(app): State<AppState>,
) -> (StatusCode, Json<ApiResponse<BackupOptionsResponse>>) {
    let local_dir = normalized_backup_dir(&app.config_manager.get_backup_config());
    let response = BackupOptionsResponse {
        format_version: FORMAT_VERSION,
        default_components: DEFAULT_COMPONENTS
            .iter()
            .map(|component| component.as_str().to_string())
            .collect(),
        components: all_components()
            .into_iter()
            .map(|component| BackupComponentOption {
                key: component.as_str().to_string(),
                label: component.label().to_string(),
                description: component.description().to_string(),
                default_selected: component.default_selected(),
                sensitive: component.sensitive(),
                records: backup_component_records(&app, component).ok(),
            })
            .collect(),
        pre_restore_dir: pre_restore_dir(&local_dir).to_string_lossy().to_string(),
        local_dir: local_dir.to_string_lossy().to_string(),
    };

    (
        StatusCode::OK,
        Json(ApiResponse::success_with_message("Success", response)),
    )
}

pub async fn get_backup_config_handler(
    State(app): State<AppState>,
) -> (StatusCode, Json<ApiResponse<BackupConfig>>) {
    (
        StatusCode::OK,
        Json(ApiResponse::success_with_message(
            "Success",
            normalize_backup_config(app.config_manager.get_backup_config()),
        )),
    )
}

pub async fn set_backup_config_handler(
    State(app): State<AppState>,
    Json(payload): Json<BackupConfigRequest>,
) -> (StatusCode, Json<ApiResponse<BackupConfig>>) {
    let config = normalize_backup_config(payload.config);
    match app.config_manager.set_backup_config(config.clone()) {
        Ok(()) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "Backup config updated",
                config,
            )),
        ),
        Err(err) => (
            StatusCode::OK,
            Json(ApiResponse::<BackupConfig>::error(format!("Failed: {err}"))),
        ),
    }
}

pub async fn export_backup_handler(
    State(app): State<AppState>,
    Json(payload): Json<BackupExportRequest>,
) -> Response {
    match build_backup_archive(&app, &payload.components) {
        Ok((filename, bytes, _manifest)) => zip_download_response(filename, bytes),
        Err(err) => json_error_response(err),
    }
}

pub async fn export_backup_local_handler(
    State(app): State<AppState>,
    Json(payload): Json<BackupExportRequest>,
) -> (StatusCode, Json<ApiResponse<BackupExportLocalResponse>>) {
    match write_local_backup(&app, &payload.components, false) {
        Ok(file) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "Backup written",
                BackupExportLocalResponse { file },
            )),
        ),
        Err(err) => (
            StatusCode::OK,
            Json(ApiResponse::<BackupExportLocalResponse>::error(format!(
                "Failed: {err}"
            ))),
        ),
    }
}

pub async fn preview_backup_import_handler(
    body: Bytes,
) -> (StatusCode, Json<ApiResponse<BackupImportPreview>>) {
    if body.len() > MAX_IMPORT_BYTES {
        return (
            StatusCode::OK,
            Json(ApiResponse::<BackupImportPreview>::error(
                "Backup package is too large",
            )),
        );
    }

    match parse_backup_archive(&body) {
        Ok(parsed) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "Backup validated",
                preview_from_manifest(&parsed.manifest, None),
            )),
        ),
        Err(err) => (
            StatusCode::OK,
            Json(ApiResponse::<BackupImportPreview>::error(format!(
                "Failed: {err}"
            ))),
        ),
    }
}

pub async fn apply_backup_import_handler(
    State(app): State<AppState>,
    Query(query): Query<BackupImportApplyQuery>,
    body: Bytes,
) -> (StatusCode, Json<ApiResponse<BackupImportApplyResponse>>) {
    if body.len() > MAX_IMPORT_BYTES {
        return (
            StatusCode::OK,
            Json(ApiResponse::<BackupImportApplyResponse>::error(
                "Backup package is too large",
            )),
        );
    }

    let result = apply_backup_import_bytes(&app, &body, query);

    match result {
        Ok(response) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "Backup restored",
                response,
            )),
        ),
        Err(err) => (
            StatusCode::OK,
            Json(ApiResponse::<BackupImportApplyResponse>::error(format!(
                "Failed: {err}"
            ))),
        ),
    }
}

pub async fn preview_backup_file_handler(
    State(app): State<AppState>,
    Path(filename): Path<String>,
) -> (StatusCode, Json<ApiResponse<BackupImportPreview>>) {
    let result: Result<BackupImportPreview, String> = (|| {
        let path = resolve_backup_file(&app.config_manager.get_backup_config(), &filename)?;
        let body = fs::read(&path).map_err(|err| format!("Failed to read backup file: {err}"))?;
        let parsed = parse_backup_archive(&body)?;
        Ok(preview_from_manifest(&parsed.manifest, Some(filename)))
    })();

    match result {
        Ok(response) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "Backup validated",
                response,
            )),
        ),
        Err(err) => (
            StatusCode::OK,
            Json(ApiResponse::<BackupImportPreview>::error(format!(
                "Failed: {err}"
            ))),
        ),
    }
}

pub async fn apply_backup_file_handler(
    State(app): State<AppState>,
    Path(filename): Path<String>,
    Query(query): Query<BackupImportApplyQuery>,
) -> (StatusCode, Json<ApiResponse<BackupImportApplyResponse>>) {
    let result: Result<BackupImportApplyResponse, String> = (|| {
        let path = resolve_backup_file(&app.config_manager.get_backup_config(), &filename)?;
        let body = fs::read(&path).map_err(|err| format!("Failed to read backup file: {err}"))?;
        apply_backup_import_bytes(&app, &body, query)
    })();

    match result {
        Ok(response) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message(
                "Backup restored",
                response,
            )),
        ),
        Err(err) => (
            StatusCode::OK,
            Json(ApiResponse::<BackupImportApplyResponse>::error(format!(
                "Failed: {err}"
            ))),
        ),
    }
}

fn apply_backup_import_bytes(
    app: &AppState,
    body: &[u8],
    query: BackupImportApplyQuery,
) -> Result<BackupImportApplyResponse, String> {
    let parsed = parse_backup_archive(body)?;
    let requested_components = parse_component_csv(&query.components)?;
    let components = if requested_components.is_empty() {
        parsed
            .manifest
            .components
            .iter()
            .copied()
            .filter(|component| *component != BackupComponent::Auth)
            .collect::<Vec<_>>()
    } else {
        requested_components
    };
    validate_requested_components(&parsed.manifest, &components)?;

    let pre_restore_components = if components.contains(&BackupComponent::Auth) {
        DEFAULT_COMPONENTS
            .iter()
            .copied()
            .chain(std::iter::once(BackupComponent::Auth))
            .collect::<Vec<_>>()
    } else {
        DEFAULT_COMPONENTS.to_vec()
    };
    let pre_restore = write_local_backup(app, &pre_restore_components, true)?;
    apply_backup(app, &parsed, &components, query.mode)?;

    Ok(BackupImportApplyResponse {
        imported_components: components
            .iter()
            .map(|component| component.as_str().to_string())
            .collect(),
        backup_kind: parsed.manifest.backup_kind,
        mode: match query.mode {
            ImportMode::Merge => "merge".to_string(),
            ImportMode::Replace => "replace".to_string(),
        },
        pre_restore_file: Some(pre_restore),
    })
}

pub async fn get_backup_files_handler(
    State(app): State<AppState>,
) -> (StatusCode, Json<ApiResponse<BackupLocalFilesResponse>>) {
    match list_backup_files(&app.config_manager.get_backup_config()) {
        Ok(response) => (
            StatusCode::OK,
            Json(ApiResponse::success_with_message("Success", response)),
        ),
        Err(err) => (
            StatusCode::OK,
            Json(ApiResponse::<BackupLocalFilesResponse>::error(format!(
                "Failed: {err}"
            ))),
        ),
    }
}

pub async fn download_backup_file_handler(
    State(app): State<AppState>,
    Path(filename): Path<String>,
) -> Response {
    match resolve_backup_file(&app.config_manager.get_backup_config(), &filename) {
        Ok(path) => match fs::read(&path) {
            Ok(bytes) => zip_download_response(filename, bytes),
            Err(err) => json_error_response(format!("Failed to read backup file: {err}")),
        },
        Err(err) => json_error_response(err),
    }
}

pub async fn delete_backup_file_handler(
    State(app): State<AppState>,
    Path(filename): Path<String>,
) -> (StatusCode, Json<ApiResponse<Value>>) {
    match resolve_backup_file(&app.config_manager.get_backup_config(), &filename) {
        Ok(path) => match fs::remove_file(&path) {
            Ok(()) => (
                StatusCode::OK,
                Json(ApiResponse::success_with_message(
                    "Backup deleted",
                    json!({ "deleted": true }),
                )),
            ),
            Err(err) => (
                StatusCode::OK,
                Json(ApiResponse::error(format!("Failed: {err}"))),
            ),
        },
        Err(err) => (
            StatusCode::OK,
            Json(ApiResponse::error(format!("Failed: {err}"))),
        ),
    }
}

fn normalize_backup_config(mut config: BackupConfig) -> BackupConfig {
    let mut seen = BTreeSet::new();
    config.components = config
        .components
        .into_iter()
        .filter_map(|component| parse_component_key(&component).ok())
        .filter(|component| seen.insert(*component))
        .map(|component| component.as_str().to_string())
        .collect();
    if config.components.is_empty() {
        config.components = DEFAULT_COMPONENTS
            .iter()
            .map(|component| component.as_str().to_string())
            .collect();
    }

    config.schedule.mode = match config.schedule.mode.as_str() {
        "fixed" | "interval" | "manual" => config.schedule.mode,
        _ => "manual".to_string(),
    };
    config.schedule.weekdays = config
        .schedule
        .weekdays
        .into_iter()
        .filter(|day| (1..=7).contains(day))
        .collect();
    if config.schedule.weekdays.is_empty() {
        config.schedule.weekdays = vec![1, 2, 3, 4, 5, 6, 7];
    }
    config.schedule.times = config
        .schedule
        .times
        .into_iter()
        .filter_map(|value| normalize_hhmm(&value))
        .collect();
    if config.schedule.times.is_empty() {
        config.schedule.times = vec!["04:00".to_string()];
    }
    config.schedule.interval_value = config.schedule.interval_value.clamp(1, 365);
    config.schedule.interval_unit = match config.schedule.interval_unit.as_str() {
        "mins" | "hours" | "days" => config.schedule.interval_unit,
        _ => "days".to_string(),
    };

    config.cleanup.retention_days = config.cleanup.retention_days.clamp(1, 3650);
    config.cleanup.max_files = config.cleanup.max_files.clamp(1, 1000);
    if config.storage.local_dir.trim().is_empty() {
        config.storage = BackupStorageConfig::default();
    }

    config
}

fn normalize_hhmm(value: &str) -> Option<String> {
    let value = value.trim().replace('：', ":");
    let (hour, minute) = value.split_once(':')?;
    let hour = hour.parse::<u8>().ok()?;
    let minute = minute.parse::<u8>().ok()?;
    if hour <= 23 && minute <= 59 {
        Some(format!("{hour:02}:{minute:02}"))
    } else {
        None
    }
}

fn parse_component_csv(value: &str) -> Result<Vec<BackupComponent>, String> {
    if value.trim().is_empty() {
        return Ok(Vec::new());
    }

    value
        .split(',')
        .map(|item| parse_component_key(item.trim()))
        .collect()
}

fn parse_component_key(value: &str) -> Result<BackupComponent, String> {
    match value {
        "config" => Ok(BackupComponent::Config),
        "sms" => Ok(BackupComponent::Sms),
        "notification_config" => Ok(BackupComponent::NotificationConfig),
        "notification_logs" => Ok(BackupComponent::NotificationLogs),
        "notification_queue" => Ok(BackupComponent::NotificationQueue),
        "automation_config" => Ok(BackupComponent::AutomationConfig),
        "automation_logs" => Ok(BackupComponent::AutomationLogs),
        "sim_cache" => Ok(BackupComponent::SimCache),
        "esim_cache" => Ok(BackupComponent::EsimCache),
        "auth" => Ok(BackupComponent::Auth),
        _ => Err(format!("Unsupported backup component: {value}")),
    }
}

fn selected_components(components: &[BackupComponent]) -> Result<Vec<BackupComponent>, String> {
    let mut seen = BTreeSet::new();
    let result = components
        .iter()
        .copied()
        .filter(|component| seen.insert(*component))
        .collect::<Vec<_>>();
    if result.is_empty() {
        Err("At least one backup component must be selected".to_string())
    } else {
        Ok(result)
    }
}

fn backup_kind(components: &[BackupComponent]) -> BackupKind {
    let selected = components.iter().copied().collect::<HashSet<_>>();
    if all_components()
        .into_iter()
        .filter(|component| *component != BackupComponent::Auth)
        .all(|component| selected.contains(&component))
    {
        BackupKind::Full
    } else {
        BackupKind::Slim
    }
}

fn backup_filename(kind: BackupKind) -> String {
    format!(
        "simadmin-backup-{}-{}.zip",
        kind.as_str(),
        Local::now().format("%Y%m%d-%H%M%S")
    )
}

fn build_backup_archive(
    app: &AppState,
    components: &[BackupComponent],
) -> Result<(String, Vec<u8>, BackupManifest), String> {
    let components = selected_components(components)?;
    let kind = backup_kind(&components);
    let mut payloads = Vec::new();
    for component in &components {
        payloads.push(export_component(app, *component)?);
    }

    let mut files = BTreeMap::new();
    let mut counts = BTreeMap::new();
    let mut component_bytes = Vec::new();
    for payload in payloads {
        let path = payload.component.file_path().to_string();
        let bytes = serde_json::to_vec_pretty(&payload.value).map_err(|err| {
            format!(
                "Failed to serialize component {}: {err}",
                payload.component.as_str()
            )
        })?;
        files.insert(
            path.clone(),
            BackupFileManifest {
                sha256: sha256_hex(&bytes),
                records: payload.count,
            },
        );
        counts.insert(payload.component.as_str().to_string(), payload.count);
        component_bytes.push((path, bytes));
    }

    let manifest = BackupManifest {
        format_version: FORMAT_VERSION,
        app: APP_ID.to_string(),
        backup_kind: kind,
        components: components.clone(),
        counts,
        files,
        created_at: Local::now().to_rfc3339(),
        simadmin_version: env!("CARGO_PKG_VERSION").to_string(),
        contains_sensitive_data: components.iter().any(|component| component.sensitive()),
    };

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    writer
        .start_file(MANIFEST_PATH, options)
        .map_err(|err| format!("Failed to create manifest: {err}"))?;
    writer
        .write_all(
            &serde_json::to_vec_pretty(&manifest)
                .map_err(|err| format!("Failed to serialize manifest: {err}"))?,
        )
        .map_err(|err| format!("Failed to write manifest: {err}"))?;

    for (path, bytes) in component_bytes {
        writer
            .start_file(path, options)
            .map_err(|err| format!("Failed to create component file: {err}"))?;
        writer
            .write_all(&bytes)
            .map_err(|err| format!("Failed to write component file: {err}"))?;
    }

    let bytes = writer
        .finish()
        .map_err(|err| format!("Failed to finalize backup zip: {err}"))?
        .into_inner();

    Ok((backup_filename(kind), bytes, manifest))
}

fn backup_component_records(app: &AppState, component: BackupComponent) -> Result<usize, String> {
    export_component(app, component).map(|payload| payload.count)
}

fn export_component(
    app: &AppState,
    component: BackupComponent,
) -> Result<ComponentPayload, String> {
    let value = match component {
        BackupComponent::Config => {
            serde_json::to_value(BackupBaseConfig::from(app.config_manager.get_config()))
                .map_err(|err| format!("Failed to export base config: {err}"))?
        }
        BackupComponent::NotificationConfig => {
            serde_json::to_value(app.config_manager.get_notifications())
                .map_err(|err| format!("Failed to export notification config: {err}"))?
        }
        BackupComponent::AutomationConfig => {
            serde_json::to_value(app.config_manager.get_automation_config())
                .map_err(|err| format!("Failed to export automation config: {err}"))?
        }
        BackupComponent::Auth => serde_json::to_value(AuthBackup {
            security: app.config_manager.get_security(),
            auth_config: export_table_rows(
                &app.database,
                "auth_config",
                &["key", "value", "updated_at"],
            )?,
        })
        .map_err(|err| format!("Failed to export auth config: {err}"))?,
        BackupComponent::Sms => json!({
            "rows": export_table_rows(
                &app.database,
                "sms_messages",
                &[
                    "direction",
                    "phone_number",
                    "content",
                    "timestamp",
                    "status",
                    "notification_status",
                    "pdu",
                    "created_at",
                ],
            )?
        }),
        BackupComponent::NotificationLogs => json!({
            "rows": export_table_rows(
                &app.database,
                "notification_logs",
                &[
                    "event_type",
                    "status",
                    "summary",
                    "rule_id",
                    "rule_name",
                    "channel_id",
                    "channel_name",
                    "message",
                    "created_at",
                ],
            )?
        }),
        BackupComponent::NotificationQueue => json!({
            "rows": export_table_rows(
                &app.database,
                "notification_queue",
                &[
                    "status",
                    "event_type",
                    "event_label",
                    "summary",
                    "reason",
                    "rule_id",
                    "rule_name",
                    "channel_id",
                    "channel_name",
                    "channel_type",
                    "title",
                    "body",
                    "next_attempt_at",
                    "attempt_count",
                    "max_attempts",
                    "last_error",
                    "created_at",
                    "updated_at",
                    "expires_at",
                ],
            )?
        }),
        BackupComponent::AutomationLogs => json!({
            "rows": export_table_rows(
                &app.database,
                "automation_logs",
                &[
                    "task_id",
                    "task_name",
                    "task_type",
                    "status",
                    "detail",
                    "created_at",
                ],
            )?
        }),
        BackupComponent::SimCache => json!({
            "smsc_cache": export_table_rows(
                &app.database,
                "smsc_cache",
                &["identity_key", "iccid", "imsi", "operator_id", "sms_center", "source", "updated_at"],
            )?,
            "own_number_cache": export_table_rows(
                &app.database,
                "own_number_cache",
                &["identity_key", "iccid", "imsi", "operator_id", "phone_numbers", "source", "updated_at"],
            )?,
            "sms_storage_cache": export_table_rows(
                &app.database,
                "sms_storage_cache",
                &["identity_key", "iccid", "imsi", "operator_id", "sms_used", "sms_total", "source", "updated_at"],
            )?,
        }),
        BackupComponent::EsimCache => json!({
            "esim_profile_cache": export_table_rows(
                &app.database,
                "esim_profile_cache",
                &[
                    "iccid",
                    "name",
                    "provider",
                    "state",
                    "profile_class",
                    "imsi",
                    "msisdn",
                    "smsc",
                    "smdp",
                    "matching_id",
                    "isdp_aid",
                    "mcc",
                    "mnc",
                    "disable_allowed",
                    "delete_allowed",
                    "updated_at",
                ],
            )?,
            "esim_euicc_cache": export_table_rows(
                &app.database,
                "esim_euicc_cache",
                &[
                    "cache_key",
                    "eid",
                    "status",
                    "manufacturer",
                    "memory_total_kb",
                    "memory_available_kb",
                    "memory_total_customizable",
                    "raw",
                    "updated_at",
                ],
            )?,
        }),
    };

    let count = component_record_count(component, &value);
    Ok(ComponentPayload {
        component,
        value,
        count,
    })
}

fn export_table_rows(
    database: &Database,
    table: &str,
    columns: &[&str],
) -> Result<Vec<Value>, String> {
    let sql = format!("SELECT {} FROM {table}", columns.join(", "));
    database
        .with_connection(|conn| {
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map([], |row| row_to_object(row, columns))?;
            let mut result = Vec::new();
            for row in rows {
                result.push(Value::Object(row?));
            }
            Ok(result)
        })
        .map_err(|err| format!("Failed to export table {table}: {err}"))
}

fn row_to_object(row: &Row<'_>, columns: &[&str]) -> rusqlite::Result<Map<String, Value>> {
    let mut object = Map::new();
    for (index, column) in columns.iter().enumerate() {
        object.insert(
            (*column).to_string(),
            value_ref_to_json(row.get_ref(index)?),
        );
    }
    Ok(object)
}

fn value_ref_to_json(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(value) => json!(value),
        ValueRef::Real(value) => json!(value),
        ValueRef::Text(value) => Value::String(String::from_utf8_lossy(value).to_string()),
        ValueRef::Blob(value) => Value::String(general_purpose::STANDARD.encode(value)),
    }
}

fn component_record_count(component: BackupComponent, value: &Value) -> usize {
    match component {
        BackupComponent::Config
        | BackupComponent::NotificationConfig
        | BackupComponent::AutomationConfig
        | BackupComponent::Auth => 1,
        BackupComponent::Sms
        | BackupComponent::NotificationLogs
        | BackupComponent::NotificationQueue
        | BackupComponent::AutomationLogs => value
            .get("rows")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0),
        BackupComponent::SimCache => ["smsc_cache", "own_number_cache", "sms_storage_cache"]
            .iter()
            .filter_map(|key| value.get(*key).and_then(Value::as_array).map(Vec::len))
            .sum(),
        BackupComponent::EsimCache => ["esim_profile_cache", "esim_euicc_cache"]
            .iter()
            .filter_map(|key| value.get(*key).and_then(Value::as_array).map(Vec::len))
            .sum(),
    }
}

fn parse_backup_archive(bytes: &[u8]) -> Result<ParsedBackup, String> {
    let cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).map_err(|err| format!("Invalid zip: {err}"))?;
    let mut files = BTreeMap::new();

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|err| format!("Failed to read zip entry: {err}"))?;
        if entry.is_dir() {
            continue;
        }
        let Some(name) = entry
            .enclosed_name()
            .map(|path| path.to_string_lossy().to_string())
        else {
            continue;
        };
        let mut content = Vec::new();
        entry
            .read_to_end(&mut content)
            .map_err(|err| format!("Failed to read zip entry {name}: {err}"))?;
        files.insert(name, content);
    }

    let manifest_bytes = files
        .get(MANIFEST_PATH)
        .ok_or_else(|| "manifest.json not found".to_string())?;
    let manifest: BackupManifest = serde_json::from_slice(manifest_bytes)
        .map_err(|err| format!("Invalid manifest.json: {err}"))?;
    validate_manifest(&manifest, &files)?;
    Ok(ParsedBackup { manifest, files })
}

fn validate_manifest(
    manifest: &BackupManifest,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<(), String> {
    if manifest.app != APP_ID {
        return Err("Backup package does not belong to SimAdmin".to_string());
    }
    if manifest.format_version != FORMAT_VERSION {
        return Err(format!(
            "Unsupported backup format version {}, current supported version is {}",
            manifest.format_version, FORMAT_VERSION
        ));
    }
    if manifest.components.is_empty() {
        return Err("Backup contains no components".to_string());
    }

    for component in &manifest.components {
        let path = component.file_path();
        let bytes = files
            .get(path)
            .ok_or_else(|| format!("Component file missing: {path}"))?;
        let expected = manifest
            .files
            .get(path)
            .ok_or_else(|| format!("Manifest file metadata missing: {path}"))?;
        let actual = sha256_hex(bytes);
        if actual != expected.sha256 {
            return Err(format!("Component checksum mismatch: {path}"));
        }

        let value: Value = serde_json::from_slice(bytes)
            .map_err(|err| format!("Invalid component JSON {path}: {err}"))?;
        let count = component_record_count(*component, &value);
        if count != expected.records {
            return Err(format!("Component record count mismatch: {path}"));
        }
    }

    Ok(())
}

fn preview_from_manifest(
    manifest: &BackupManifest,
    filename: Option<String>,
) -> BackupImportPreview {
    let components = manifest
        .components
        .iter()
        .map(|component| BackupImportComponentPreview {
            key: component.as_str().to_string(),
            label: component.label().to_string(),
            records: manifest
                .counts
                .get(component.as_str())
                .copied()
                .unwrap_or_default(),
            sensitive: component.sensitive(),
        })
        .collect();
    let mut warnings = Vec::new();
    if manifest.components.contains(&BackupComponent::Auth) {
        warnings.push(
            "Backup contains auth component. It is ignored unless explicitly selected.".to_string(),
        );
    }
    if manifest.components.contains(&BackupComponent::EsimCache) {
        warnings.push("eSIM cache does not contain carrier eSIM profile contents.".to_string());
    }

    BackupImportPreview {
        filename,
        backup_kind: manifest.backup_kind,
        format_version: manifest.format_version,
        simadmin_version: manifest.simadmin_version.clone(),
        created_at: manifest.created_at.clone(),
        contains_sensitive_data: manifest.contains_sensitive_data,
        components,
        warnings,
    }
}

fn validate_requested_components(
    manifest: &BackupManifest,
    components: &[BackupComponent],
) -> Result<(), String> {
    if components.is_empty() {
        return Err("No components selected for import".to_string());
    }
    for component in components {
        if !manifest.components.contains(component) {
            return Err(format!(
                "Backup package does not contain component {}",
                component.as_str()
            ));
        }
    }
    Ok(())
}

fn apply_backup(
    app: &AppState,
    parsed: &ParsedBackup,
    components: &[BackupComponent],
    mode: ImportMode,
) -> Result<(), String> {
    validate_requested_components(&parsed.manifest, components)?;

    let mut next_config = app.config_manager.get_config();
    let mut config_changed = false;

    for component in components {
        match component {
            BackupComponent::Config => {
                let value = component_value(parsed, *component)?;
                let base: BackupBaseConfig = serde_json::from_value(value)
                    .map_err(|err| format!("Invalid config component: {err}"))?;
                base.apply_to(&mut next_config);
                config_changed = true;
            }
            BackupComponent::NotificationConfig => {
                let value = component_value(parsed, *component)?;
                let notifications: NotificationConfig = serde_json::from_value(value)
                    .map_err(|err| format!("Invalid notification config: {err}"))?;
                next_config.webhook = notifications
                    .first_webhook_config()
                    .unwrap_or_else(Default::default);
                next_config.notifications = notifications;
                config_changed = true;
            }
            BackupComponent::AutomationConfig => {
                let value = component_value(parsed, *component)?;
                next_config.automation = serde_json::from_value::<AutomationConfig>(value)
                    .map_err(|err| format!("Invalid automation config: {err}"))?;
                config_changed = true;
            }
            BackupComponent::Auth => {
                let value = component_value(parsed, *component)?;
                let auth: AuthBackup = serde_json::from_value(value)
                    .map_err(|err| format!("Invalid auth component: {err}"))?;
                next_config.security = auth.security;
                config_changed = true;
            }
            _ => {}
        }
    }

    app.database
        .with_transaction(|tx| {
            for component in components {
                match component {
                    BackupComponent::Sms => import_rows_component(
                        tx,
                        parsed,
                        *component,
                        mode,
                        "sms_messages",
                        &[
                            "direction",
                            "phone_number",
                            "content",
                            "timestamp",
                            "status",
                            "notification_status",
                            "pdu",
                            "created_at",
                        ],
                        import_sms_row,
                    )?,
                    BackupComponent::NotificationLogs => import_rows_component(
                        tx,
                        parsed,
                        *component,
                        mode,
                        "notification_logs",
                        &[
                            "event_type",
                            "status",
                            "summary",
                            "rule_id",
                            "rule_name",
                            "channel_id",
                            "channel_name",
                            "message",
                            "created_at",
                        ],
                        import_notification_log_row,
                    )?,
                    BackupComponent::NotificationQueue => import_rows_component(
                        tx,
                        parsed,
                        *component,
                        mode,
                        "notification_queue",
                        &[
                            "status",
                            "event_type",
                            "event_label",
                            "summary",
                            "reason",
                            "rule_id",
                            "rule_name",
                            "channel_id",
                            "channel_name",
                            "channel_type",
                            "title",
                            "body",
                            "next_attempt_at",
                            "attempt_count",
                            "max_attempts",
                            "last_error",
                            "created_at",
                            "updated_at",
                            "expires_at",
                        ],
                        import_notification_queue_row,
                    )?,
                    BackupComponent::AutomationLogs => import_rows_component(
                        tx,
                        parsed,
                        *component,
                        mode,
                        "automation_logs",
                        &[
                            "task_id",
                            "task_name",
                            "task_type",
                            "status",
                            "detail",
                            "created_at",
                        ],
                        import_automation_log_row,
                    )?,
                    BackupComponent::SimCache => import_sim_cache(
                        tx,
                        component_value(parsed, *component).map_err(to_sql_error)?,
                        mode,
                    )?,
                    BackupComponent::EsimCache => import_esim_cache(
                        tx,
                        component_value(parsed, *component).map_err(to_sql_error)?,
                        mode,
                    )?,
                    BackupComponent::Auth => import_auth_config(
                        tx,
                        component_value(parsed, *component).map_err(to_sql_error)?,
                    )?,
                    _ => {}
                }
            }
            Ok(())
        })
        .map_err(|err| format!("Failed to import database components: {err}"))?;

    if config_changed {
        app.config_manager
            .replace_config(next_config)
            .map_err(|err| format!("Failed to save restored config: {err}"))?;
    }

    if components.contains(&BackupComponent::Auth) {
        app.database
            .clear_auth_sessions()
            .map_err(|err| format!("Failed to clear auth sessions: {err}"))?;
    }

    Ok(())
}

fn component_value(parsed: &ParsedBackup, component: BackupComponent) -> Result<Value, String> {
    let bytes = parsed
        .files
        .get(component.file_path())
        .ok_or_else(|| format!("Missing component {}", component.as_str()))?;
    serde_json::from_slice(bytes)
        .map_err(|err| format!("Invalid component {}: {err}", component.as_str()))
}

fn import_rows_component<F>(
    tx: &Transaction<'_>,
    parsed: &ParsedBackup,
    component: BackupComponent,
    mode: ImportMode,
    table: &str,
    columns: &[&str],
    import_row: F,
) -> rusqlite::Result<()>
where
    F: Fn(&Transaction<'_>, &Map<String, Value>, &[&str]) -> rusqlite::Result<()>,
{
    let value = component_value(parsed, component).map_err(to_sql_error)?;
    let rows = value.get("rows").and_then(Value::as_array).ok_or_else(|| {
        to_sql_error(format!(
            "Component {} missing rows array",
            component.as_str()
        ))
    })?;

    if mode == ImportMode::Replace {
        tx.execute(&format!("DELETE FROM {table}"), [])?;
    }

    for row in rows {
        let object = row
            .as_object()
            .ok_or_else(|| to_sql_error(format!("Invalid row in {}", component.as_str())))?;
        import_row(tx, object, columns)?;
    }

    Ok(())
}

fn import_sms_row(
    tx: &Transaction<'_>,
    row: &Map<String, Value>,
    _columns: &[&str],
) -> rusqlite::Result<()> {
    let direction = string_value(row, "direction");
    let phone_number = string_value(row, "phone_number");
    let content = string_value(row, "content");
    let timestamp = string_value(row, "timestamp");
    let status = non_empty_string(row, "status", "received");
    let notification_status = non_empty_string(row, "notification_status", "pending");
    let pdu = optional_string_value(row, "pdu");
    let created_at = optional_string_value(row, "created_at");

    let exists: i64 = tx.query_row(
        "SELECT COUNT(*) FROM sms_messages
         WHERE direction = ?1
           AND phone_number = ?2
           AND content = ?3
           AND timestamp = ?4
           AND COALESCE(pdu, '') = COALESCE(?5, '')",
        params![direction, phone_number, content, timestamp, pdu],
        |row| row.get(0),
    )?;
    if exists > 0 {
        return Ok(());
    }

    tx.execute(
        "INSERT INTO sms_messages (
            direction, phone_number, content, timestamp, status,
            notification_status, pdu, created_at
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, COALESCE(?8, CURRENT_TIMESTAMP))",
        params![
            direction,
            phone_number,
            content,
            timestamp,
            status,
            notification_status,
            pdu,
            created_at
        ],
    )?;
    Ok(())
}

fn import_notification_log_row(
    tx: &Transaction<'_>,
    row: &Map<String, Value>,
    _columns: &[&str],
) -> rusqlite::Result<()> {
    let fields = [
        "event_type",
        "status",
        "summary",
        "rule_id",
        "rule_name",
        "channel_id",
        "channel_name",
        "message",
        "created_at",
    ];
    if row_exists_by_fields(tx, "notification_logs", row, &fields)? {
        return Ok(());
    }
    insert_row(tx, "notification_logs", row, &fields)
}

fn import_notification_queue_row(
    tx: &Transaction<'_>,
    row: &Map<String, Value>,
    _columns: &[&str],
) -> rusqlite::Result<()> {
    let dedupe_fields = [
        "status",
        "event_type",
        "summary",
        "channel_id",
        "rule_id",
        "title",
        "body",
        "next_attempt_at",
        "created_at",
    ];
    if row_exists_by_fields(tx, "notification_queue", row, &dedupe_fields)? {
        return Ok(());
    }
    insert_row(
        tx,
        "notification_queue",
        row,
        &[
            "status",
            "event_type",
            "event_label",
            "summary",
            "reason",
            "rule_id",
            "rule_name",
            "channel_id",
            "channel_name",
            "channel_type",
            "title",
            "body",
            "next_attempt_at",
            "attempt_count",
            "max_attempts",
            "last_error",
            "created_at",
            "updated_at",
            "expires_at",
        ],
    )
}

fn import_automation_log_row(
    tx: &Transaction<'_>,
    row: &Map<String, Value>,
    _columns: &[&str],
) -> rusqlite::Result<()> {
    let fields = [
        "task_id",
        "task_name",
        "task_type",
        "status",
        "detail",
        "created_at",
    ];
    if row_exists_by_fields(tx, "automation_logs", row, &fields)? {
        return Ok(());
    }
    insert_row(tx, "automation_logs", row, &fields)
}

fn import_sim_cache(tx: &Transaction<'_>, value: Value, mode: ImportMode) -> rusqlite::Result<()> {
    if mode == ImportMode::Replace {
        tx.execute("DELETE FROM smsc_cache", [])?;
        tx.execute("DELETE FROM own_number_cache", [])?;
        tx.execute("DELETE FROM sms_storage_cache", [])?;
    }

    for row in array_rows(&value, "smsc_cache")? {
        tx.execute(
            "INSERT INTO smsc_cache (
                identity_key, iccid, imsi, operator_id, sms_center, source, updated_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(identity_key) DO UPDATE SET
                iccid = excluded.iccid,
                imsi = excluded.imsi,
                operator_id = excluded.operator_id,
                sms_center = excluded.sms_center,
                source = excluded.source,
                updated_at = excluded.updated_at",
            params![
                string_value(row, "identity_key"),
                string_value(row, "iccid"),
                string_value(row, "imsi"),
                string_value(row, "operator_id"),
                string_value(row, "sms_center"),
                non_empty_string(row, "source", "backup"),
                string_value(row, "updated_at"),
            ],
        )?;
    }
    for row in array_rows(&value, "own_number_cache")? {
        tx.execute(
            "INSERT INTO own_number_cache (
                identity_key, iccid, imsi, operator_id, phone_numbers, source, updated_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(identity_key) DO UPDATE SET
                iccid = excluded.iccid,
                imsi = excluded.imsi,
                operator_id = excluded.operator_id,
                phone_numbers = excluded.phone_numbers,
                source = excluded.source,
                updated_at = excluded.updated_at",
            params![
                string_value(row, "identity_key"),
                string_value(row, "iccid"),
                string_value(row, "imsi"),
                string_value(row, "operator_id"),
                string_value(row, "phone_numbers"),
                non_empty_string(row, "source", "backup"),
                string_value(row, "updated_at"),
            ],
        )?;
    }
    for row in array_rows(&value, "sms_storage_cache")? {
        tx.execute(
            "INSERT INTO sms_storage_cache (
                identity_key, iccid, imsi, operator_id, sms_used, sms_total, source, updated_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(identity_key) DO UPDATE SET
                iccid = excluded.iccid,
                imsi = excluded.imsi,
                operator_id = excluded.operator_id,
                sms_used = excluded.sms_used,
                sms_total = COALESCE(excluded.sms_total, sms_storage_cache.sms_total),
                source = excluded.source,
                updated_at = excluded.updated_at",
            params![
                string_value(row, "identity_key"),
                string_value(row, "iccid"),
                string_value(row, "imsi"),
                string_value(row, "operator_id"),
                optional_i64_value(row, "sms_used"),
                optional_i64_value(row, "sms_total"),
                non_empty_string(row, "source", "backup"),
                string_value(row, "updated_at"),
            ],
        )?;
    }
    Ok(())
}

fn import_esim_cache(tx: &Transaction<'_>, value: Value, mode: ImportMode) -> rusqlite::Result<()> {
    if mode == ImportMode::Replace {
        tx.execute("DELETE FROM esim_profile_cache", [])?;
        tx.execute("DELETE FROM esim_euicc_cache", [])?;
    }

    for row in array_rows(&value, "esim_profile_cache")? {
        let iccid = crate::utils::normalize_iccid(&string_value(row, "iccid"));
        if iccid.is_empty() {
            continue;
        }
        tx.execute(
            "INSERT INTO esim_profile_cache (
                iccid, name, provider, state, profile_class, imsi, msisdn, smsc, smdp,
                matching_id, isdp_aid, mcc, mnc, disable_allowed, delete_allowed, updated_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
             ON CONFLICT(iccid) DO UPDATE SET
                name = COALESCE(excluded.name, esim_profile_cache.name),
                provider = COALESCE(excluded.provider, esim_profile_cache.provider),
                state = COALESCE(excluded.state, esim_profile_cache.state),
                profile_class = COALESCE(excluded.profile_class, esim_profile_cache.profile_class),
                imsi = COALESCE(excluded.imsi, esim_profile_cache.imsi),
                msisdn = COALESCE(excluded.msisdn, esim_profile_cache.msisdn),
                smsc = COALESCE(excluded.smsc, esim_profile_cache.smsc),
                smdp = COALESCE(excluded.smdp, esim_profile_cache.smdp),
                matching_id = COALESCE(excluded.matching_id, esim_profile_cache.matching_id),
                isdp_aid = COALESCE(excluded.isdp_aid, esim_profile_cache.isdp_aid),
                mcc = COALESCE(excluded.mcc, esim_profile_cache.mcc),
                mnc = COALESCE(excluded.mnc, esim_profile_cache.mnc),
                disable_allowed = COALESCE(excluded.disable_allowed, esim_profile_cache.disable_allowed),
                delete_allowed = COALESCE(excluded.delete_allowed, esim_profile_cache.delete_allowed),
                updated_at = excluded.updated_at",
            params![
                iccid,
                non_empty_optional_string(row, "name"),
                non_empty_optional_string(row, "provider"),
                non_empty_optional_string(row, "state"),
                non_empty_optional_string(row, "profile_class"),
                non_empty_optional_string(row, "imsi"),
                non_empty_optional_string(row, "msisdn"),
                non_empty_optional_string(row, "smsc"),
                non_empty_optional_string(row, "smdp"),
                non_empty_optional_string(row, "matching_id"),
                non_empty_optional_string(row, "isdp_aid"),
                non_empty_optional_string(row, "mcc"),
                non_empty_optional_string(row, "mnc"),
                optional_bool_i64_value(row, "disable_allowed"),
                optional_bool_i64_value(row, "delete_allowed"),
                non_empty_string(row, "updated_at", &Utc::now().to_rfc3339()),
            ],
        )?;
    }
    for row in array_rows(&value, "esim_euicc_cache")? {
        let cache_key = non_empty_string(row, "cache_key", "default");
        tx.execute(
            "INSERT INTO esim_euicc_cache (
                cache_key, eid, status, manufacturer, memory_total_kb,
                memory_available_kb, memory_total_customizable, raw, updated_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(cache_key) DO UPDATE SET
                eid = excluded.eid,
                status = excluded.status,
                manufacturer = excluded.manufacturer,
                memory_total_kb = excluded.memory_total_kb,
                memory_available_kb = excluded.memory_available_kb,
                memory_total_customizable = excluded.memory_total_customizable,
                raw = excluded.raw,
                updated_at = excluded.updated_at",
            params![
                cache_key,
                string_value(row, "eid"),
                string_value(row, "status"),
                string_value(row, "manufacturer"),
                optional_f64_value(row, "memory_total_kb"),
                optional_f64_value(row, "memory_available_kb"),
                optional_bool_i64_value(row, "memory_total_customizable"),
                string_value(row, "raw"),
                non_empty_string(row, "updated_at", &Utc::now().to_rfc3339()),
            ],
        )?;
    }
    Ok(())
}

fn import_auth_config(tx: &Transaction<'_>, value: Value) -> rusqlite::Result<()> {
    let auth: AuthBackup = serde_json::from_value(value).map_err(to_sql_error)?;
    tx.execute("DELETE FROM auth_config", [])?;
    for row in auth.auth_config {
        let object = row
            .as_object()
            .ok_or_else(|| to_sql_error("Invalid auth_config row".to_string()))?;
        tx.execute(
            "INSERT INTO auth_config (key, value, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                updated_at = excluded.updated_at",
            params![
                string_value(object, "key"),
                string_value(object, "value"),
                optional_i64_value(object, "updated_at").unwrap_or_else(|| Utc::now().timestamp()),
            ],
        )?;
    }
    tx.execute("DELETE FROM auth_sessions", [])?;
    Ok(())
}

fn array_rows<'a>(value: &'a Value, key: &str) -> rusqlite::Result<Vec<&'a Map<String, Value>>> {
    let Some(rows) = value.get(key).and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    rows.iter()
        .map(|row| {
            row.as_object()
                .ok_or_else(|| to_sql_error(format!("Invalid row in {key}")))
        })
        .collect()
}

fn insert_row(
    tx: &Transaction<'_>,
    table: &str,
    row: &Map<String, Value>,
    columns: &[&str],
) -> rusqlite::Result<()> {
    let placeholders = (1..=columns.len())
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "INSERT INTO {table} ({}) VALUES ({})",
        columns.join(", "),
        placeholders
    );
    let values = columns
        .iter()
        .map(|column| json_to_sql_value(row.get(*column)))
        .collect::<Vec<_>>();
    let params = values.iter().map(|value| value as &dyn rusqlite::ToSql);
    tx.execute(&sql, rusqlite::params_from_iter(params))?;
    Ok(())
}

fn row_exists_by_fields(
    tx: &Transaction<'_>,
    table: &str,
    row: &Map<String, Value>,
    fields: &[&str],
) -> rusqlite::Result<bool> {
    let where_clause = fields
        .iter()
        .map(|field| format!("COALESCE(CAST({field} AS TEXT), '') = COALESCE(CAST(? AS TEXT), '')"))
        .collect::<Vec<_>>()
        .join(" AND ");
    let sql = format!("SELECT COUNT(*) FROM {table} WHERE {where_clause}");
    let values = fields
        .iter()
        .map(|field| json_to_sql_value(row.get(*field)))
        .collect::<Vec<_>>();
    let params = values.iter().map(|value| value as &dyn rusqlite::ToSql);
    let count: i64 = tx.query_row(&sql, rusqlite::params_from_iter(params), |row| row.get(0))?;
    Ok(count > 0)
}

fn json_to_sql_value(value: Option<&Value>) -> rusqlite::types::Value {
    match value {
        Some(Value::Null) | None => rusqlite::types::Value::Null,
        Some(Value::Bool(value)) => rusqlite::types::Value::Integer(if *value { 1 } else { 0 }),
        Some(Value::Number(value)) => {
            if let Some(value) = value.as_i64() {
                rusqlite::types::Value::Integer(value)
            } else if let Some(value) = value.as_f64() {
                rusqlite::types::Value::Real(value)
            } else {
                rusqlite::types::Value::Null
            }
        }
        Some(Value::String(value)) => rusqlite::types::Value::Text(value.clone()),
        Some(value) => rusqlite::types::Value::Text(value.to_string()),
    }
}

fn string_value(row: &Map<String, Value>, key: &str) -> String {
    row.get(key)
        .and_then(|value| match value {
            Value::String(value) => Some(value.clone()),
            Value::Number(value) => Some(value.to_string()),
            Value::Bool(value) => Some(value.to_string()),
            _ => None,
        })
        .unwrap_or_default()
}

fn non_empty_string(row: &Map<String, Value>, key: &str, fallback: &str) -> String {
    let value = string_value(row, key);
    if value.trim().is_empty() {
        fallback.to_string()
    } else {
        value
    }
}

fn optional_string_value(row: &Map<String, Value>, key: &str) -> Option<String> {
    let value = string_value(row, key);
    (!value.is_empty()).then_some(value)
}

fn non_empty_optional_string(row: &Map<String, Value>, key: &str) -> Option<String> {
    optional_string_value(row, key).filter(|value| !value.trim().is_empty())
}

fn optional_i64_value(row: &Map<String, Value>, key: &str) -> Option<i64> {
    row.get(key).and_then(|value| match value {
        Value::Number(value) => value.as_i64(),
        Value::String(value) => value.parse::<i64>().ok(),
        _ => None,
    })
}

fn optional_f64_value(row: &Map<String, Value>, key: &str) -> Option<f64> {
    row.get(key).and_then(|value| match value {
        Value::Number(value) => value.as_f64(),
        Value::String(value) => value.parse::<f64>().ok(),
        _ => None,
    })
}

fn optional_bool_i64_value(row: &Map<String, Value>, key: &str) -> Option<i64> {
    row.get(key).and_then(|value| match value {
        Value::Bool(value) => Some(if *value { 1 } else { 0 }),
        Value::Number(value) => value.as_i64(),
        Value::String(value) => value.parse::<i64>().ok(),
        _ => None,
    })
}

fn to_sql_error(error: impl std::fmt::Display) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        error.to_string(),
    )))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = digest::digest(&digest::SHA256, bytes);
    digest
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn zip_download_response(filename: String, bytes: Vec<u8>) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/zip")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )
        .body(Body::from(bytes))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn json_error_response(message: String) -> Response {
    (
        StatusCode::OK,
        Json(ApiResponse::<Value>::error(format!("Failed: {message}"))),
    )
        .into_response()
}

fn normalized_backup_dir(config: &BackupConfig) -> PathBuf {
    let dir = config.storage.local_dir.trim();
    if dir.is_empty() {
        PathBuf::from(DEFAULT_BACKUP_DIR)
    } else {
        PathBuf::from(dir)
    }
}

fn pre_restore_dir(local_dir: &FsPath) -> PathBuf {
    local_dir.join(PRE_RESTORE_DIR_NAME)
}

fn write_local_backup(
    app: &AppState,
    components: &[BackupComponent],
    pre_restore: bool,
) -> Result<BackupLocalFile, String> {
    let config = normalize_backup_config(app.config_manager.get_backup_config());
    write_local_backup_with_config(app, components, pre_restore, &config)
}

pub(crate) fn write_automation_backup(
    app: &AppState,
    component_keys: &[String],
    local_dir: &str,
) -> Result<BackupLocalFile, String> {
    let components = if component_keys.is_empty() {
        DEFAULT_COMPONENTS.to_vec()
    } else {
        component_keys
            .iter()
            .map(|component| parse_component_key(component))
            .collect::<Result<Vec<_>, _>>()?
    };
    let mut config = normalize_backup_config(app.config_manager.get_backup_config());
    if !local_dir.trim().is_empty() {
        config.storage.local_dir = local_dir.trim().to_string();
    }
    write_local_backup_with_config(app, &components, false, &config)
}

fn write_local_backup_with_config(
    app: &AppState,
    components: &[BackupComponent],
    pre_restore: bool,
    config: &BackupConfig,
) -> Result<BackupLocalFile, String> {
    let (filename, bytes, _manifest) = build_backup_archive(app, components)?;
    let local_dir = normalized_backup_dir(config);
    let target_dir = if pre_restore {
        pre_restore_dir(&local_dir)
    } else {
        local_dir
    };
    fs::create_dir_all(&target_dir)
        .map_err(|err| format!("Failed to create backup directory: {err}"))?;
    let target = target_dir.join(&filename);
    fs::write(&target, bytes).map_err(|err| format!("Failed to write backup file: {err}"))?;
    cleanup_backup_dir(&target_dir, &config.cleanup)?;
    backup_file_info(&target, pre_restore)
}

fn cleanup_backup_dir(dir: &FsPath, cleanup: &BackupCleanupConfig) -> Result<(), String> {
    let mut files = backup_zip_paths(dir)?;
    files.sort_by_key(|path| {
        fs::metadata(path)
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .map(|elapsed| elapsed.as_secs())
            .unwrap_or(u64::MAX)
    });

    if cleanup.retention_days_enabled {
        let cutoff = std::time::SystemTime::now()
            .checked_sub(std::time::Duration::from_secs(
                u64::from(cleanup.retention_days) * 24 * 60 * 60,
            ))
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        for path in &files {
            if fs::metadata(path)
                .and_then(|meta| meta.modified())
                .map(|modified| modified < cutoff)
                .unwrap_or(false)
            {
                let _ = fs::remove_file(path);
            }
        }
    }

    if cleanup.max_files_enabled {
        let mut remaining = backup_zip_paths(dir)?;
        remaining.sort_by_key(|path| {
            std::cmp::Reverse(
                fs::metadata(path)
                    .and_then(|meta| meta.modified())
                    .ok()
                    .and_then(|modified| {
                        modified
                            .duration_since(std::time::SystemTime::UNIX_EPOCH)
                            .ok()
                    })
                    .map(|duration| duration.as_secs())
                    .unwrap_or(0),
            )
        });
        for path in remaining.into_iter().skip(cleanup.max_files as usize) {
            let _ = fs::remove_file(path);
        }
    }

    Ok(())
}

fn list_backup_files(config: &BackupConfig) -> Result<BackupLocalFilesResponse, String> {
    let config = normalize_backup_config(config.clone());
    let local_dir = normalized_backup_dir(&config);
    let pre_dir = pre_restore_dir(&local_dir);
    Ok(BackupLocalFilesResponse {
        backups: list_backup_dir(&local_dir, false)?,
        pre_restore: list_backup_dir(&pre_dir, true)?,
    })
}

fn list_backup_dir(dir: &FsPath, pre_restore: bool) -> Result<Vec<BackupLocalFile>, String> {
    let mut files = Vec::new();
    for path in backup_zip_paths(dir)? {
        if let Ok(file) = backup_file_info(&path, pre_restore) {
            files.push(file);
        }
    }
    files.sort_by(|left, right| right.modified_at.cmp(&left.modified_at));
    Ok(files)
}

fn backup_zip_paths(dir: &FsPath) -> Result<Vec<PathBuf>, String> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for entry in
        fs::read_dir(dir).map_err(|err| format!("Failed to read backup directory: {err}"))?
    {
        let path = entry
            .map_err(|err| format!("Failed to read backup directory entry: {err}"))?
            .path();
        if path.is_file()
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .map(is_backup_filename)
                .unwrap_or(false)
        {
            paths.push(path);
        }
    }
    Ok(paths)
}

fn backup_file_info(path: &FsPath, pre_restore: bool) -> Result<BackupLocalFile, String> {
    let metadata =
        fs::metadata(path).map_err(|err| format!("Failed to read backup metadata: {err}"))?;
    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|modified| {
            modified
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .ok()
                .and_then(|duration| Local.timestamp_opt(duration.as_secs() as i64, 0).single())
        })
        .map(|time| time.to_rfc3339())
        .unwrap_or_default();
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Invalid backup filename".to_string())?
        .to_string();

    let mut backup_kind = None;
    let mut components = Vec::new();
    let mut counts = BTreeMap::new();
    let mut valid = false;
    let mut error = String::new();
    if let Ok(bytes) = fs::read(path) {
        match parse_backup_archive(&bytes) {
            Ok(parsed) => {
                valid = true;
                backup_kind = Some(parsed.manifest.backup_kind);
                components = parsed
                    .manifest
                    .components
                    .iter()
                    .map(|component| component.as_str().to_string())
                    .collect();
                counts = parsed.manifest.counts;
            }
            Err(err) => {
                error = err;
            }
        }
    } else {
        error = "Failed to read backup file".to_string();
    }

    Ok(BackupLocalFile {
        name,
        size: metadata.len(),
        modified_at,
        backup_kind,
        components,
        counts,
        pre_restore,
        valid,
        error,
    })
}

fn resolve_backup_file(config: &BackupConfig, filename: &str) -> Result<PathBuf, String> {
    if !is_backup_filename(filename) {
        return Err("Invalid backup filename".to_string());
    }
    let local_dir = normalized_backup_dir(&normalize_backup_config(config.clone()));
    let direct = local_dir.join(filename);
    if direct.is_file() {
        return Ok(direct);
    }
    let pre_restore = pre_restore_dir(&local_dir).join(filename);
    if pre_restore.is_file() {
        return Ok(pre_restore);
    }
    Err("Backup file not found".to_string())
}

fn is_backup_filename(filename: &str) -> bool {
    filename.starts_with("simadmin-backup-")
        && filename.ends_with(".zip")
        && filename
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}
