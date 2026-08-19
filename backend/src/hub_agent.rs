use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::Duration,
};

use async_trait::async_trait;
use axum::{
    body::{to_bytes, Body},
    extract::State,
    http::{Method, Request, StatusCode},
    Json, Router,
};
use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone, Utc};
use mdns_sd::{ServiceDaemon, ServiceInfo};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use simadmin_agent::{
    AgentConfig, AgentError, AgentExecutor, AgentResult, AgentRuntime, AgentStore, ExecutionResult,
};
use simadmin_protocol::{
    AgentType, CommandPayload, CommandResultStatus, ConfigSyncPayload, ConnectionScope,
    DeviceAction, DeviceActionCommandPayload, DeviceFeatureSnapshot, DeviceProvisionRequest,
    DeviceStatus, DeviceStatusItem, Envelope, EventItem, EventType, HardwareFingerprintPayload,
    LayerStatus, MessageAckPayload, OtaUpdateCommandPayload, SendSmsCommandPayload,
    SessionReadyPayload, SmsDirection, SmsItem, SmsStatus,
};
use tokio::sync::{Mutex as AsyncMutex, Notify, RwLock};
use tower::ServiceExt;
use uuid::Uuid;

use crate::{
    automation::tasks::TaskRegistry,
    config::HubConfig,
    db::HubEventRecord,
    handlers::{read_temperature_sensors, run_safe_os_reboot_sequence},
    models::{RadioMode, WorkMode},
    modem_manager::{
        apply_roaming_policy, get_airplane_mode, get_data_connection_status, get_device_info_data,
        get_network_info_data, get_radio_mode, get_sim_info_data_with_cache, hangup_all_calls,
        make_call, restart_baseband, send_sms, set_airplane_mode, set_data_connection_with_apn,
        set_radio_mode,
    },
    state::AppState,
    utils::{read_cpu_load_sync, read_disk_info, read_memory_info, read_system_info, read_uptime},
};

#[derive(Debug, Clone, serde::Serialize)]
pub struct HubRuntimeStatus {
    pub enabled: bool,
    pub online: bool,
    pub connection_state: String,
    pub hub_url: Option<String>,
    pub hub_instance_id: Option<String>,
    pub hub_version: Option<String>,
    pub last_connected_at: Option<DateTime<Utc>>,
    pub agent_id: Option<String>,
    pub device_ids: Vec<String>,
    pub local_fallback_state: String,
    pub last_error: Option<String>,
    #[serde(skip)]
    offline_since: Option<DateTime<Utc>>,
}

impl Default for HubRuntimeStatus {
    fn default() -> Self {
        Self {
            enabled: false,
            online: false,
            connection_state: "disabled".into(),
            hub_url: None,
            hub_instance_id: None,
            hub_version: None,
            last_connected_at: None,
            agent_id: None,
            device_ids: Vec::new(),
            local_fallback_state: "inactive".into(),
            last_error: None,
            offline_since: None,
        }
    }
}

#[derive(Default)]
struct Credentials {
    agent_id: String,
    token: String,
}

pub struct SimAdminExecutor {
    app: AppState,
    online: AtomicBool,
    credentials: RwLock<Credentials>,
    status: Arc<RwLock<HubRuntimeStatus>>,
    generation: u64,
    active_generation: Arc<AtomicU64>,
}

impl SimAdminExecutor {
    fn new(
        app: AppState,
        status: Arc<RwLock<HubRuntimeStatus>>,
        generation: u64,
        active_generation: Arc<AtomicU64>,
    ) -> Self {
        Self {
            app,
            online: AtomicBool::new(false),
            credentials: RwLock::new(Credentials::default()),
            status,
            generation,
            active_generation,
        }
    }

    fn is_current_generation(&self) -> bool {
        self.active_generation.load(Ordering::SeqCst) == self.generation
    }

    fn device_id<'a>(&self, configured: &'a [String]) -> AgentResult<&'a str> {
        configured
            .first()
            .map(String::as_str)
            .ok_or(AgentError::MissingCredentials)
    }

    async fn snapshot(&self, device_id: &str) -> DeviceStatusItem {
        let observed_at = Utc::now();
        let (device, sim, network, data_active, airplane, radio) = tokio::join!(
            get_device_info_data(&self.app.dbus_conn),
            get_sim_info_data_with_cache(&self.app.dbus_conn, Some(&self.app.database)),
            get_network_info_data(&self.app.dbus_conn),
            get_data_connection_status(&self.app.dbus_conn),
            get_airplane_mode(&self.app.dbus_conn),
            get_radio_mode(&self.app.dbus_conn),
        );
        let device = device.ok();
        let sim = sim.ok();
        let network = network.ok();
        let data_active = data_active.ok();
        let airplane = airplane.ok();
        let radio = radio.ok();
        let (uptime, _) = read_uptime().unwrap_or_default();
        let memory_percent = read_memory_info()
            .ok()
            .and_then(|(total, available, _, _)| {
                (total > 0).then_some(((total - available) as f64 / total as f64 * 100.0) as f32)
            });
        let cpu_percent = read_cpu_load_sync()
            .ok()
            .map(|value| value.load_percent as f32);
        let temperatures = read_temperature_sensors();
        let min_temperature_c = temperatures
            .iter()
            .map(|value| value.temperature as f32)
            .min_by(f32::total_cmp);
        let max_temperature_c = temperatures
            .iter()
            .map(|value| value.temperature as f32)
            .max_by(f32::total_cmp);
        let esim = self.app.esim_supervisor.get_profiles().await.ok();
        let hub_config = self.app.config_manager.get_hub_config();
        let hardware_present = device.as_ref().is_some_and(|value| value.powered);
        let registration_ok = network.as_ref().is_some_and(|value| {
            matches!(
                value.registration_status.as_str(),
                "home" | "roaming" | "registered"
            )
        });
        let mut capabilities = vec![
            "sim",
            "sms",
            "sms_send",
            "sms_receive",
            "network",
            "data_control",
            "radio_mode",
            "device_network",
            "wlan",
            "phone",
            "call",
            "system",
            "notifications",
            "automation",
            "ota",
        ];
        if self.app.config_manager.get_work_mode() == WorkMode::Esim {
            capabilities.push("esim");
        }

        DeviceStatusItem {
            item_id: format!("status-{}", Uuid::new_v4()),
            device_id: device_id.to_owned(),
            observed_at,
            // Reaching this snapshot means the Agent connection is online. Modem health is
            // represented by the hardware/control/SIM/registration/data layer statuses below.
            status: DeviceStatus::Online,
            hardware_present,
            control_channel_status: if device.is_some() {
                LayerStatus::Ok
            } else {
                LayerStatus::Error
            },
            sim_status: match sim.as_ref() {
                Some(value) if value.present => LayerStatus::Ok,
                Some(_) => LayerStatus::Warning,
                None => LayerStatus::Unknown,
            },
            cellular_registration_status: if registration_ok {
                LayerStatus::Ok
            } else {
                LayerStatus::Warning
            },
            data_connection_status: match data_active {
                Some(true) => LayerStatus::Ok,
                Some(false) => LayerStatus::Warning,
                None => LayerStatus::Unknown,
            },
            capabilities: capabilities.into_iter().map(str::to_owned).collect(),
            simadmin_version: Some(env!("CARGO_PKG_VERSION").to_owned()),
            architecture: Some(std::env::consts::ARCH.to_owned()),
            phone_number: sim
                .as_ref()
                .and_then(|value| value.phone_numbers.first().cloned()),
            carrier: network
                .as_ref()
                .map(|value| value.operator_name.clone())
                .filter(|value| !value.is_empty()),
            network_type: network
                .as_ref()
                .map(|value| value.technology_preference.clone())
                .filter(|value| !value.is_empty()),
            signal_percent: network.as_ref().map(|value| value.signal_strength),
            signal_dbm: None,
            min_temperature_c,
            max_temperature_c,
            uptime_seconds: Some(uptime),
            imei: device
                .as_ref()
                .map(|value| value.imei.clone())
                .filter(|value| !value.is_empty()),
            iccid: sim
                .as_ref()
                .map(|value| value.iccid.clone())
                .filter(|value| !value.is_empty()),
            imsi: sim
                .as_ref()
                .map(|value| value.imsi.clone())
                .filter(|value| !value.is_empty()),
            model: device.as_ref().map(|value| {
                format!("{} {}", value.manufacturer, value.model)
                    .trim()
                    .to_owned()
            }),
            cpu_percent,
            memory_percent,
            feature_snapshot: Some(DeviceFeatureSnapshot {
                sim: sim
                    .as_ref()
                    .and_then(|value| serde_json::to_value(value).ok())
                    .unwrap_or_default(),
                esim: esim
                    .and_then(|value| serde_json::to_value(value).ok())
                    .unwrap_or_default(),
                cellular: json!({
                    "network": network,
                    "data_enabled": self.app.config_manager.get_data_enabled(),
                    "data_active": data_active,
                    "roaming_enabled": self.app.config_manager.get_roaming_allowed(),
                    "airplane_mode": airplane.as_ref().map(|value| value.enabled),
                    "radio_mode": radio.as_ref().map(|value| value.mode.clone()),
                    "apn": self.app.config_manager.get_apn_config(),
                }),
                device_network: serde_json::to_value(self.app.config_manager.get_device_network())
                    .unwrap_or_default(),
                vowifi: Value::Null,
                volte: Value::Null,
                phone: json!({}),
                system: json!({
                    "system_info": read_system_info().ok(),
                    "disk": read_disk_info(),
                    "baseband_revision": device.as_ref().map(|value| value.revision.clone()),
                    "uptime_seconds": uptime,
                    "temperatures": temperatures,
                }),
                local_notifications: json!({
                    "configured": !self.app.config_manager.get_notifications().rules.is_empty(),
                    "hub_online": self.online.load(Ordering::SeqCst),
                }),
                local_automation: json!({
                    "configured_tasks": self.app.config_manager.get_automation_config().tasks.len(),
                    "hub_online": self.online.load(Ordering::SeqCst),
                }),
                policy_status: json!({
                    "hub_enabled": hub_config.enabled,
                    "hub_online": self.online.load(Ordering::SeqCst),
                }),
            }),
        }
    }

    async fn execute_send_sms(&self, command: &CommandPayload) -> AgentResult<Value> {
        let payload: SendSmsCommandPayload = serde_json::from_value(command.payload.clone())?;
        let path = send_sms(&self.app.dbus_conn, &payload.phone_number, &payload.content)
            .await
            .map_err(|error| AgentError::Execution(error.to_string()))?;
        let sms_id = self.app.database.insert_sms(
            "outgoing",
            &payload.phone_number,
            &payload.content,
            "sent",
            None,
        )?;
        Ok(json!({"path": path, "local_sms_id": sms_id, "hub_command_id": command.command_id}))
    }

    async fn execute_device_action(&self, command: &CommandPayload) -> AgentResult<Value> {
        let payload: DeviceActionCommandPayload = serde_json::from_value(command.payload.clone())?;
        let parameters = &payload.parameters;
        match payload.action {
            DeviceAction::RestartBaseband => {
                let result = restart_baseband(
                    &self.app.dbus_conn,
                    self.app.config_manager.get_data_enabled(),
                    self.app.config_manager.get_roaming_allowed(),
                    Some(self.app.config_manager.get_apn_config()),
                )
                .await
                .map_err(AgentError::Execution)?;
                serde_json::to_value(result).map_err(Into::into)
            }
            DeviceAction::RebootDevice => {
                let delay = parameters
                    .get("delay_seconds")
                    .and_then(Value::as_u64)
                    .unwrap_or(5) as u32;
                let events = self.app.system_event_emitter.clone();
                tokio::spawn(async move { run_safe_os_reboot_sequence(delay, events).await });
                Ok(json!({"scheduled": true, "delay_seconds": delay}))
            }
            DeviceAction::SetDataEnabled => {
                let enabled = required_bool(parameters, "enabled")?;
                set_data_connection_with_apn(
                    &self.app.dbus_conn,
                    enabled,
                    self.app.config_manager.get_roaming_allowed(),
                    Some(&self.app.config_manager.get_apn_config()),
                )
                .await
                .map_err(|error| AgentError::Execution(error.to_string()))?;
                self.app
                    .config_manager
                    .set_data_enabled(enabled)
                    .map_err(AgentError::Execution)?;
                self.app
                    .data_user_disabled
                    .store(!enabled, Ordering::SeqCst);
                Ok(json!({"enabled": enabled}))
            }
            DeviceAction::SetRoamingEnabled => {
                let enabled = required_bool(parameters, "enabled")?;
                apply_roaming_policy(&self.app.dbus_conn, &self.app.config_manager, enabled)
                    .await
                    .map_err(|error| AgentError::Execution(error.to_string()))?;
                Ok(json!({"enabled": enabled}))
            }
            DeviceAction::SetAirplaneMode => {
                let enabled = required_bool(parameters, "enabled")?;
                set_airplane_mode(&self.app.dbus_conn, enabled)
                    .await
                    .map_err(AgentError::Execution)?;
                Ok(json!({"enabled": enabled}))
            }
            DeviceAction::SetRadioMode => {
                let mode: RadioMode = serde_json::from_value(
                    parameters
                        .get("mode")
                        .cloned()
                        .ok_or_else(|| AgentError::Execution("mode is required".into()))?,
                )?;
                set_radio_mode(&self.app.dbus_conn, mode)
                    .await
                    .map_err(|error| AgentError::Execution(error.to_string()))?;
                Ok(json!({"applied": true}))
            }
            DeviceAction::SetApn => {
                let mut apn = self.app.config_manager.get_apn_config();
                apn.apn = required_str(parameters, "apn")?.to_owned();
                self.app
                    .config_manager
                    .set_apn_config(apn.clone())
                    .map_err(AgentError::Execution)?;
                set_data_connection_with_apn(
                    &self.app.dbus_conn,
                    self.app.config_manager.get_data_enabled(),
                    self.app.config_manager.get_roaming_allowed(),
                    Some(&apn),
                )
                .await
                .map_err(|error| AgentError::Execution(error.to_string()))?;
                Ok(json!({"apn": apn.apn}))
            }
            DeviceAction::EsimEnableProfile => {
                let result = self
                    .app
                    .esim_supervisor
                    .enable_profile(required_str(parameters, "iccid")?.to_owned())
                    .await
                    .map_err(|error| AgentError::Execution(error.message()))?;
                serde_json::to_value(result).map_err(Into::into)
            }
            DeviceAction::EsimDisableProfile => Err(AgentError::Execution(
                "disable profile is not supported by the current SimAdmin eSIM service".into(),
            )),
            DeviceAction::EsimDeleteProfile => {
                let result = self
                    .app
                    .esim_supervisor
                    .delete_profile(required_str(parameters, "iccid")?.to_owned())
                    .await
                    .map_err(|error| AgentError::Execution(error.message()))?;
                serde_json::to_value(result).map_err(Into::into)
            }
            DeviceAction::CallDial => {
                let path = make_call(
                    &self.app.dbus_conn,
                    required_str(parameters, "phone_number")?,
                )
                .await
                .map_err(|error| AgentError::Execution(error.to_string()))?;
                Ok(json!({"path": path}))
            }
            DeviceAction::CallHangupAll => {
                hangup_all_calls(&self.app.dbus_conn)
                    .await
                    .map_err(|error| AgentError::Execution(error.to_string()))?;
                Ok(json!({"hung_up": true}))
            }
        }
    }

    async fn execute_automation(&self, command: &CommandPayload) -> AgentResult<Value> {
        let action = command.command_type.trim_start_matches("automation_");
        let config = command.payload.get("config").cloned().unwrap_or_default();
        let registry = TaskRegistry::new();
        let handler = registry.get(action).ok_or_else(|| {
            AgentError::Execution(format!("unsupported automation action {action}"))
        })?;
        handler
            .execute(&self.app, &config)
            .await
            .map_err(|error| AgentError::Execution(error.to_string()))?;
        Ok(json!({"execution_id": command.payload.get("execution_id"), "action": action}))
    }

    async fn execute_ota(&self, command: &CommandPayload, apply: bool) -> AgentResult<Value> {
        let payload: OtaUpdateCommandPayload = serde_json::from_value(command.payload.clone())?;
        let credentials = self.credentials.read().await;
        let download_url = if payload.download_url.starts_with('/') {
            let hub_url = self.app.config_manager.get_hub_config().url;
            if hub_url.trim().is_empty() {
                return Err(AgentError::Execution(
                    "Hub 地址为空，无法下载 OTA 更新包".into(),
                ));
            }
            format!("{}{}", hub_url.trim_end_matches('/'), payload.download_url)
        } else {
            payload.download_url.clone()
        };
        let mut request = reqwest::Client::new().get(download_url);
        if payload.source == "upload" {
            request = request
                .query(&[("agent_id", credentials.agent_id.as_str())])
                .bearer_auth(&credentials.token);
        }
        let bytes = request.send().await?.error_for_status()?.bytes().await?;
        if bytes.len() as u64 > crate::ota::MAX_OTA_BYTES {
            return Err(AgentError::Execution("OTA package exceeds 50 MiB".into()));
        }
        if let Some(expected) = payload.sha256.as_deref() {
            let actual = format!("{:x}", Sha256::digest(&bytes));
            if !actual.eq_ignore_ascii_case(expected) {
                return Err(AgentError::Execution(format!(
                    "OTA SHA-256 mismatch: expected {expected}, got {actual}"
                )));
            }
        }
        let uploaded = crate::ota::handle_ota_upload(&bytes).map_err(AgentError::Execution)?;
        if !apply {
            return serde_json::to_value(uploaded).map_err(Into::into);
        }
        if !uploaded.validation.valid {
            return Err(AgentError::Execution(
                uploaded
                    .validation
                    .error
                    .unwrap_or_else(|| "OTA package validation failed".into()),
            ));
        }
        let message = crate::ota::apply_ota_update(true).map_err(AgentError::Execution)?;
        Ok(json!({"version": payload.target_version, "message": message}))
    }

    async fn execute_device_api(&self, command: &CommandPayload) -> AgentResult<Value> {
        let payload: simadmin_protocol::DeviceApiRequestPayload =
            serde_json::from_value(command.payload.clone())?;
        let router = device_api_router()
            .ok_or_else(|| AgentError::Execution("device API router is unavailable".into()))?;
        let method = Method::from_bytes(payload.method.as_bytes())
            .map_err(|error| AgentError::Execution(error.to_string()))?;
        let uri = format!("/api{}", payload.path);
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&payload.body)?))
            .map_err(|error| AgentError::Execution(error.to_string()))?;
        let response = router
            .oneshot(request)
            .await
            .map_err(|error| AgentError::Execution(error.to_string()))?;
        let status = response.status().as_u16();
        let bytes = to_bytes(response.into_body(), 2 * 1024 * 1024)
            .await
            .map_err(|error| AgentError::Execution(error.to_string()))?;
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).map_err(|error| {
                AgentError::Execution(format!("device API returned non-JSON response: {error}"))
            })?
        };
        serde_json::to_value(simadmin_protocol::DeviceApiResponsePayload { status, body })
            .map_err(Into::into)
    }
}

#[async_trait]
impl AgentExecutor for SimAdminExecutor {
    async fn registration_fingerprint(&self) -> AgentResult<Option<HardwareFingerprintPayload>> {
        let device = get_device_info_data(&self.app.dbus_conn)
            .await
            .map_err(|error| AgentError::Execution(error.to_string()))?;
        let imei = device.imei.trim();
        Ok((!imei.is_empty()).then(|| HardwareFingerprintPayload {
            imei: Some(imei.to_owned()),
            ..Default::default()
        }))
    }

    async fn status_items(&self, device_ids: &[String]) -> AgentResult<Vec<DeviceStatusItem>> {
        Ok(vec![self.snapshot(self.device_id(device_ids)?).await])
    }

    async fn sms_items(&self, device_ids: &[String]) -> AgentResult<Vec<SmsItem>> {
        let device_id = self.device_id(device_ids)?.to_owned();
        let sim = get_sim_info_data_with_cache(&self.app.dbus_conn, Some(&self.app.database))
            .await
            .ok();
        self.app
            .database
            .get_unsynced_sms_messages(100)?
            .into_iter()
            .map(|message| {
                Ok(SmsItem {
                    item_id: format!("sms-local-{}", message.id),
                    device_id: device_id.clone(),
                    direction: if message.direction == "incoming" {
                        SmsDirection::Incoming
                    } else {
                        SmsDirection::Outgoing
                    },
                    phone_number: message.phone_number,
                    content: message.content,
                    timestamp: parse_local_timestamp(&message.timestamp),
                    status: parse_sms_status(&message.status),
                    pdu: message.pdu,
                    transport: "modem".to_owned(),
                    iccid: sim
                        .as_ref()
                        .map(|value| value.iccid.clone())
                        .filter(|value| !value.is_empty()),
                    imsi: sim
                        .as_ref()
                        .map(|value| value.imsi.clone())
                        .filter(|value| !value.is_empty()),
                    operator_name: sim
                        .as_ref()
                        .map(|value| value.operator_name.clone())
                        .filter(|value| !value.is_empty()),
                    hub_command_id: None,
                })
            })
            .collect()
    }

    async fn event_items(&self, device_ids: &[String]) -> AgentResult<Vec<EventItem>> {
        let device_id = self.device_id(device_ids)?.to_owned();
        self.app
            .database
            .unsynced_hub_events(100)?
            .into_iter()
            .map(|event| {
                Ok(EventItem {
                    item_id: event.item_id,
                    device_id: device_id.clone(),
                    event_type: parse_event_type(&event.event_type)?,
                    event_code: event.event_code,
                    occurred_at: DateTime::parse_from_rfc3339(&event.occurred_at)
                        .map(|value| value.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    summary: event.summary,
                    details: event.details,
                    local_fallback_applied: event.local_fallback_applied,
                })
            })
            .collect()
    }

    async fn prepare_full_sms_sync(&self) -> AgentResult<()> {
        self.app.database.reset_sms_hub_sync()?;
        Ok(())
    }

    async fn handle_message_ack(
        &self,
        source: &Envelope,
        ack: &MessageAckPayload,
    ) -> AgentResult<()> {
        if source.message_type == "sms_batch" {
            for item in ack.items.iter().filter(|item| item.accepted) {
                if let Some(id) = item
                    .item_id
                    .strip_prefix("sms-local-")
                    .and_then(|value| value.parse().ok())
                {
                    self.app.database.mark_sms_hub_synced(id)?;
                }
            }
        } else if source.message_type == "event_batch" {
            for item in ack.items.iter().filter(|item| item.accepted) {
                self.app.database.mark_hub_event_synced(&item.item_id)?;
                if item.hub_handled == Some(false) {
                    self.app
                        .notification_sender
                        .deliver_queued_hub_event(&item.item_id)
                        .await
                        .map_err(AgentError::Execution)?;
                }
            }
        }
        Ok(())
    }

    async fn execute(
        &self,
        _device_id: &str,
        command: &CommandPayload,
    ) -> AgentResult<ExecutionResult> {
        if Utc::now() > command.expires_at {
            return Ok(ExecutionResult {
                status: CommandResultStatus::Failed,
                result: json!({}),
                error_message: Some("command expired before execution".into()),
            });
        }
        let result = match command.command_type.as_str() {
            "send_sms" => self.execute_send_sms(command).await,
            "device_action" => self.execute_device_action(command).await,
            "restart_baseband" => {
                self.execute_device_action(&CommandPayload {
                    command_type: "device_action".into(),
                    payload: serde_json::to_value(DeviceActionCommandPayload {
                        action: DeviceAction::RestartBaseband,
                        parameters: json!({}),
                    })?,
                    ..command.clone()
                })
                .await
            }
            "set_apn" => {
                self.execute_device_action(&network_action(command, DeviceAction::SetApn))
                    .await
            }
            "set_data_enabled" => {
                self.execute_device_action(&network_action(command, DeviceAction::SetDataEnabled))
                    .await
            }
            "set_roaming_enabled" => {
                self.execute_device_action(&network_action(
                    command,
                    DeviceAction::SetRoamingEnabled,
                ))
                .await
            }
            value if value.starts_with("automation_") => self.execute_automation(command).await,
            "ota_prepare" => self.execute_ota(command, false).await,
            "ota_update" => self.execute_ota(command, true).await,
            "hub_unbind" => {
                let app = self.app.clone();
                let mut config = app.config_manager.get_hub_config();
                config.enabled = true;
                config.url.clear();
                app.config_manager
                    .set_hub_config(config)
                    .map_err(AgentError::Execution)?;
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    if let Err(error) = app.hub_agent_manager.unbind(app.clone()).await {
                        tracing::warn!(%error, "Hub requested unbind failed");
                    }
                });
                Ok(json!({"scheduled": true}))
            }
            "device_api_request" => self.execute_device_api(command).await,
            value => Err(AgentError::Execution(format!(
                "unsupported command {value}"
            ))),
        };
        Ok(match result {
            Ok(result) => ExecutionResult {
                status: CommandResultStatus::Succeeded,
                result,
                error_message: None,
            },
            Err(error) => ExecutionResult {
                status: CommandResultStatus::Failed,
                result: json!({}),
                error_message: Some(error.to_string()),
            },
        })
    }

    async fn apply_policy(&self, _device_id: &str, policy: &ConfigSyncPayload) -> AgentResult<()> {
        if policy.schema_version != 1 {
            return Err(AgentError::Execution(format!(
                "unsupported policy schema {}",
                policy.schema_version
            )));
        }
        Ok(())
    }

    async fn session_state_changed(&self, online: bool) {
        if !self.is_current_generation() {
            return;
        }
        self.online.store(online, Ordering::SeqCst);
        let mut status = self.status.write().await;
        status.online = online;
        status.connection_state = if online { "connected" } else { "offline" }.into();
        if online {
            status.offline_since = None;
            status.last_connected_at = Some(Utc::now());
            status.last_error = None;
            status.local_fallback_state = "standby".into();
        } else if status.enabled {
            status.offline_since.get_or_insert_with(Utc::now);
            status.local_fallback_state = if self
                .app
                .config_manager
                .get_hub_config()
                .local_fallback_enabled
            {
                "armed"
            } else {
                "disabled"
            }
            .into();
        }
    }

    async fn session_ready(&self, session: &SessionReadyPayload) {
        if !self.is_current_generation() {
            return;
        }
        let mut status = self.status.write().await;
        status.hub_instance_id = session.hub_instance_id.clone();
        status.hub_version = session.hub_version.clone();
        if let Some(url) = session.canonical_public_url.as_ref() {
            status.hub_url = Some(url.clone());
        }
    }

    async fn session_error(&self, error: &AgentError) {
        if !self.is_current_generation() {
            return;
        }
        let mut status = self.status.write().await;
        status.online = false;
        status.offline_since.get_or_insert_with(Utc::now);
        status.connection_state = if matches!(error, AgentError::PairingPending(_)) {
            "awaiting_approval"
        } else {
            "offline"
        }
        .into();
        status.last_error =
            (!matches!(error, AgentError::PairingPending(_))).then(|| error.to_string());
    }

    async fn agent_config_changed(&self, config: &AgentConfig) {
        if !self.is_current_generation() {
            return;
        }
        let mut status = self.status.write().await;
        status.agent_id = config.agent_id.clone();
        status.device_ids = config.device_ids.clone();
        status.hub_instance_id = config.hub_instance_id.clone();
        status.hub_version = config.hub_version.clone();
        status.last_connected_at = config.last_connected_at;
        if let Some(url) = config.canonical_hub_url.as_ref() {
            status.hub_url = Some(url.clone());
        }
    }

    async fn configure_credentials(&self, agent_id: &str, token: &str) {
        let mut credentials = self.credentials.write().await;
        credentials.agent_id = agent_id.to_owned();
        credentials.token = token.to_owned();
    }
}

fn network_action(command: &CommandPayload, action: DeviceAction) -> CommandPayload {
    CommandPayload {
        command_type: "device_action".into(),
        payload: serde_json::to_value(DeviceActionCommandPayload {
            action,
            parameters: command.payload.clone(),
        })
        .unwrap_or_default(),
        ..command.clone()
    }
}

fn required_bool(value: &Value, key: &str) -> AgentResult<bool> {
    value
        .get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| AgentError::Execution(format!("{key} must be boolean")))
}

fn required_str<'a>(value: &'a Value, key: &str) -> AgentResult<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AgentError::Execution(format!("{key} is required")))
}

fn parse_sms_status(value: &str) -> SmsStatus {
    match value {
        "pending" => SmsStatus::Pending,
        "sent" => SmsStatus::Sent,
        "failed" => SmsStatus::Failed,
        "received" => SmsStatus::Received,
        _ => SmsStatus::Unknown,
    }
}

fn parse_local_timestamp(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .or_else(|_| {
            NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").map(|value| {
                FixedOffset::east_opt(8 * 3600)
                    .unwrap()
                    .from_local_datetime(&value)
                    .single()
                    .unwrap()
                    .with_timezone(&Utc)
            })
        })
        .unwrap_or_else(|_| Utc::now())
}

fn parse_event_type(value: &str) -> AgentResult<EventType> {
    match value {
        "sms" => Ok(EventType::Sms),
        "ddns" => Ok(EventType::Ddns),
        "version_update" => Ok(EventType::VersionUpdate),
        "system_event" => Ok(EventType::SystemEvent),
        "device_status" => Ok(EventType::DeviceStatus),
        "automation" => Ok(EventType::Automation),
        _ => Err(AgentError::Execution(format!(
            "unsupported event type {value}"
        ))),
    }
}

fn agent_store_path() -> PathBuf {
    let legacy = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("hub-agent.db");
    let data_dir = PathBuf::from("/data");
    if !data_dir.is_dir() {
        return legacy;
    }
    let persistent = data_dir.join("hub-agent.db");
    if !persistent.exists() && legacy.exists() {
        if let Err(error) = std::fs::copy(&legacy, &persistent) {
            tracing::warn!(%error, "迁移 Hub Agent 身份数据库失败，继续使用旧路径");
            return legacy;
        }
        let legacy_wal = legacy.with_file_name("hub-agent.db-wal");
        if legacy_wal.exists() {
            let _ = std::fs::copy(legacy_wal, persistent.with_file_name("hub-agent.db-wal"));
        }
    }
    persistent
}

fn new_agent_config(hub: &HubConfig, enrollment_token: Option<String>) -> AgentConfig {
    let mut config = AgentConfig::new(
        hub.url.clone(),
        AgentType::Simadmin,
        ConnectionScope::Remote,
        read_system_info()
            .map(|value| value.nodename)
            .unwrap_or_else(|_| "simadmin".into()),
        env!("CARGO_PKG_VERSION").into(),
    );
    config.enrollment_token = enrollment_token;
    config
}

const SIMADMIN_SERVICE_TYPE: &str = "_simadmin-agent._tcp.local.";

#[derive(Default)]
struct HubAgentLifecycle {
    active_key: Option<String>,
    runtime_task: Option<tokio::task::JoinHandle<()>>,
    fallback_task: Option<tokio::task::JoinHandle<()>>,
    publisher: Option<MdnsPublisher>,
}

struct MdnsPublisher {
    daemon: ServiceDaemon,
    fullname: String,
}

impl MdnsPublisher {
    fn stop(self) {
        let _ = self.daemon.unregister(&self.fullname);
        let _ = self.daemon.shutdown();
    }
}

pub struct HubAgentManager {
    status: Arc<RwLock<HubRuntimeStatus>>,
    lifecycle: AsyncMutex<HubAgentLifecycle>,
    provisioning: AsyncMutex<()>,
    store_path: PathBuf,
    service_port: AtomicU16,
    discovery_id: String,
    active_generation: Arc<AtomicU64>,
}

impl HubAgentManager {
    pub fn new() -> Self {
        Self::with_store_path(agent_store_path())
    }

    fn with_store_path(store_path: PathBuf) -> Self {
        let status = Arc::new(RwLock::new(HubRuntimeStatus::default()));
        set_global_status(status.clone());
        Self {
            status,
            lifecycle: AsyncMutex::new(HubAgentLifecycle::default()),
            provisioning: AsyncMutex::new(()),
            store_path,
            service_port: AtomicU16::new(3000),
            discovery_id: format!("discovery-{}", Uuid::new_v4()),
            active_generation: Arc::new(AtomicU64::new(0)),
        }
    }

    pub async fn initialize(&self, app: AppState, service_port: u16) {
        self.service_port.store(service_port, Ordering::SeqCst);
        let config = app.config_manager.get_hub_config();
        self.apply_config(app, config, None).await;
    }

    pub async fn apply_config(
        &self,
        app: AppState,
        hub: HubConfig,
        enrollment_token: Option<String>,
    ) {
        let key = lifecycle_key(&hub);
        let mut lifecycle = self.lifecycle.lock().await;
        if lifecycle.active_key.as_deref() == Some(&key) && enrollment_token.is_none() {
            return;
        }
        let generation = self.active_generation.fetch_add(1, Ordering::SeqCst) + 1;
        if let Some(task) = lifecycle.runtime_task.take() {
            task.abort();
            let _ = task.await;
        }
        if let Some(task) = lifecycle.fallback_task.take() {
            task.abort();
            let _ = task.await;
        }
        if let Some(publisher) = lifecycle.publisher.take() {
            publisher.stop();
        }
        lifecycle.active_key = Some(key);

        let next_status = HubRuntimeStatus {
            enabled: hub.enabled,
            hub_url: (!hub.url.trim().is_empty()).then(|| hub.url.trim().to_owned()),
            connection_state: if hub.enabled {
                "waiting_for_hub".into()
            } else {
                "disabled".into()
            },
            local_fallback_state: if !hub.enabled {
                "inactive"
            } else if hub.local_fallback_enabled {
                "armed"
            } else {
                "disabled"
            }
            .into(),
            offline_since: hub.enabled.then(Utc::now),
            ..Default::default()
        };
        *self.status.write().await = next_status;
        if !hub.enabled {
            return;
        }

        if hub.local_fallback_enabled {
            let fallback_app = app.clone();
            let fallback_status = self.status.clone();
            let timeout = hub.local_fallback_timeout_seconds.max(30);
            lifecycle.fallback_task = Some(tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(5));
                loop {
                    interval.tick().await;
                    let fallback_active = {
                        let mut status = fallback_status.write().await;
                        if status.online {
                            false
                        } else {
                            let active = local_fallback_timeout_elapsed(
                                status.offline_since,
                                timeout,
                                Utc::now(),
                            );
                            status.local_fallback_state =
                                if active { "active" } else { "armed" }.into();
                            active
                        }
                    };
                    if !fallback_active {
                        continue;
                    }
                    match fallback_app
                        .database
                        .hub_events_due_for_local_fallback(timeout, 20)
                    {
                        Ok(events) => {
                            for item_id in events {
                                if let Err(error) = fallback_app
                                    .notification_sender
                                    .deliver_queued_hub_event(&item_id)
                                    .await
                                {
                                    tracing::warn!(%item_id, %error, "local Hub notification fallback failed");
                                }
                            }
                        }
                        Err(error) => tracing::warn!(%error, "failed to scan Hub fallback queue"),
                    }
                }
            }));
        }

        if hub.url.trim().is_empty() {
            match self.start_mdns() {
                Ok(publisher) => lifecycle.publisher = Some(publisher),
                Err(error) => {
                    self.status.write().await.last_error = Some(format!("mDNS 发布失败: {error}"));
                }
            }
            return;
        }

        let store = match AgentStore::open(&self.store_path) {
            Ok(store) => Arc::new(store),
            Err(error) => {
                self.status.write().await.last_error = Some(error.to_string());
                return;
            }
        };
        let mut config = store
            .load_config()
            .ok()
            .flatten()
            .unwrap_or_else(|| new_agent_config(&hub, enrollment_token.clone()));
        if config.hub_url.trim_end_matches('/') != hub.url.trim_end_matches('/') {
            config = new_agent_config(&hub, enrollment_token.clone());
        } else if enrollment_token.is_some() {
            config.enrollment_token = enrollment_token;
            config.agent_id = None;
            config.device_ids.clear();
            config.pairing_code = None;
            config.hub_instance_id = None;
            config.hub_version = None;
            config.canonical_hub_url = None;
            config.last_connected_at = None;
        }
        config.enabled = true;
        config.hub_url = hub.url.trim_end_matches('/').to_owned();
        if let Err(error) = store.save_config(&config) {
            self.status.write().await.last_error = Some(error.to_string());
            return;
        }
        {
            let mut status = self.status.write().await;
            status.connection_state = if config.agent_id.is_some() {
                "connecting"
            } else {
                "registering"
            }
            .into();
            status.agent_id = config.agent_id.clone();
            status.device_ids = config.device_ids.clone();
            status.hub_instance_id = config.hub_instance_id.clone();
            status.hub_version = config.hub_version.clone();
            status.last_connected_at = config.last_connected_at;
            if let Some(url) = config.canonical_hub_url.as_ref() {
                status.hub_url = Some(url.clone());
            }
        }
        let status = self.status.clone();
        let active_generation = self.active_generation.clone();
        lifecycle.runtime_task = Some(tokio::spawn(async move {
            let business_wakeup = hub_business_wakeup().clone();
            let executor = Arc::new(SimAdminExecutor::new(
                app,
                status.clone(),
                generation,
                active_generation.clone(),
            ));
            let runtime =
                AgentRuntime::new(store, executor, config).with_business_wakeup(business_wakeup);
            if let Err(error) = runtime.run_forever().await {
                if active_generation.load(Ordering::SeqCst) == generation {
                    status.write().await.last_error = Some(error.to_string());
                }
            }
        }));
    }

    fn start_mdns(&self) -> Result<MdnsPublisher, mdns_sd::Error> {
        let daemon = ServiceDaemon::new()?;
        let name = read_system_info()
            .map(|value| value.nodename)
            .unwrap_or_else(|_| "simadmin".into());
        let hostname = format!("{}.local.", mdns_label(&name));
        let instance = format!(
            "{}-{}",
            mdns_label(&name),
            self.discovery_id.chars().rev().take(8).collect::<String>()
        );
        let properties = [
            ("discovery_id", self.discovery_id.as_str()),
            ("name", name.as_str()),
            ("version", env!("CARGO_PKG_VERSION")),
            ("architecture", std::env::consts::ARCH),
        ];
        let service = ServiceInfo::new(
            SIMADMIN_SERVICE_TYPE,
            &instance,
            &hostname,
            (),
            self.service_port.load(Ordering::SeqCst),
            &properties[..],
        )?
        .enable_addr_auto();
        let fullname = service.get_fullname().to_owned();
        daemon.register(service)?;
        tracing::info!(
            port = self.service_port.load(Ordering::SeqCst),
            "SimAdmin Hub discovery enabled"
        );
        Ok(MdnsPublisher { daemon, fullname })
    }

    pub async fn provision(
        &self,
        app: AppState,
        request: &DeviceProvisionRequest,
    ) -> Result<(), String> {
        let _guard = self.provisioning.lock().await;
        let current = app.config_manager.get_hub_config();
        let status = self.status.read().await.clone();
        validate_provision_request(&current, &status, request)?;
        let mut next = current;
        next.enabled = true;
        next.url = normalized_hub_url(&request.hub_url)?;
        app.config_manager.set_hub_config(next.clone())?;
        self.apply_config(app, next, Some(request.enrollment_token.clone()))
            .await;
        Ok(())
    }

    pub async fn unbind(&self, app: AppState) -> Result<(), String> {
        let _guard = self.provisioning.lock().await;
        let mut next = app.config_manager.get_hub_config();
        next.enabled = true;
        next.url.clear();
        app.config_manager.set_hub_config(next.clone())?;
        self.apply_config(app, next, None).await;
        let mut status = self.status.write().await;
        status.agent_id = None;
        status.device_ids.clear();
        Ok(())
    }
}

impl Default for HubAgentManager {
    fn default() -> Self {
        Self::new()
    }
}

fn lifecycle_key(hub: &HubConfig) -> String {
    format!(
        "{}:{}:{}:{}",
        hub.enabled,
        hub.url.trim(),
        hub.local_fallback_enabled,
        hub.local_fallback_timeout_seconds
    )
}

fn mdns_label(value: &str) -> String {
    let label = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let label = label.trim_matches('-').chars().take(40).collect::<String>();
    if label.is_empty() {
        "simadmin".into()
    } else {
        label
    }
}

fn normalized_hub_url(value: &str) -> Result<String, String> {
    let mut url =
        reqwest::Url::parse(value.trim()).map_err(|error| format!("Hub 地址无效: {error}"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/")
    {
        return Err("Hub 地址只能包含 http(s)、主机名和可选端口".into());
    }
    url.set_path("/");
    Ok(url.to_string().trim_end_matches('/').to_owned())
}

fn validate_provision_request(
    config: &HubConfig,
    status: &HubRuntimeStatus,
    request: &DeviceProvisionRequest,
) -> Result<(), String> {
    if !config.enabled {
        return Err("设备当前处于独立运行模式".into());
    }
    if !config.url.trim().is_empty() || status.agent_id.is_some() {
        return Err("设备已经绑定 SimAdminHub".into());
    }
    if request.hub_instance_id.trim().is_empty() || request.enrollment_token.len() < 32 {
        return Err("Hub 接入凭据无效".into());
    }
    normalized_hub_url(&request.hub_url)?;
    Ok(())
}

fn global_status() -> &'static Mutex<Option<Arc<RwLock<HubRuntimeStatus>>>> {
    static STATUS: std::sync::OnceLock<Mutex<Option<Arc<RwLock<HubRuntimeStatus>>>>> =
        std::sync::OnceLock::new();
    STATUS.get_or_init(|| Mutex::new(None))
}

fn router_cell() -> &'static std::sync::OnceLock<Router> {
    static ROUTER: std::sync::OnceLock<Router> = std::sync::OnceLock::new();
    &ROUTER
}

fn hub_business_wakeup() -> &'static Arc<Notify> {
    static WAKEUP: OnceLock<Arc<Notify>> = OnceLock::new();
    WAKEUP.get_or_init(|| Arc::new(Notify::new()))
}

pub fn configure_device_api_router(router: Router) {
    let _ = router_cell().set(router);
}

fn device_api_router() -> Option<Router> {
    router_cell().get().cloned()
}

fn set_global_status(status: Arc<RwLock<HubRuntimeStatus>>) {
    *global_status().lock().unwrap() = Some(status);
}

pub async fn runtime_status(config: &HubConfig) -> HubRuntimeStatus {
    let status = global_status().lock().unwrap().clone();
    match status {
        Some(status) => status.read().await.clone(),
        None => HubRuntimeStatus {
            enabled: config.enabled,
            ..Default::default()
        },
    }
}

pub async fn local_automation_scheduling_enabled(config: &HubConfig) -> bool {
    let status = runtime_status(config).await;
    local_automation_scheduling_enabled_at(config, &status, Utc::now())
}

fn local_automation_scheduling_enabled_at(
    config: &HubConfig,
    status: &HubRuntimeStatus,
    now: DateTime<Utc>,
) -> bool {
    !config.enabled
        || (config.local_fallback_enabled
            && !status.online
            && local_fallback_timeout_elapsed(
                status.offline_since,
                config.local_fallback_timeout_seconds.max(30),
                now,
            ))
}

fn local_fallback_timeout_elapsed(
    offline_since: Option<DateTime<Utc>>,
    timeout_seconds: u64,
    now: DateTime<Utc>,
) -> bool {
    offline_since.is_some_and(|offline_since| {
        now.signed_duration_since(offline_since).num_seconds()
            >= timeout_seconds.min(i64::MAX as u64) as i64
    })
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct HubSettingsResponse {
    pub config: HubConfig,
    pub runtime: HubRuntimeStatus,
}

pub async fn get_hub_settings(
    State(app): State<AppState>,
) -> (
    StatusCode,
    Json<crate::models::ApiResponse<HubSettingsResponse>>,
) {
    let config = app.config_manager.get_hub_config();
    let runtime = runtime_status(&config).await;
    (
        StatusCode::OK,
        Json(crate::models::ApiResponse::success_with_message(
            "Success",
            HubSettingsResponse { config, runtime },
        )),
    )
}

pub async fn save_hub_settings(
    State(app): State<AppState>,
    Json(config): Json<HubConfig>,
) -> (
    StatusCode,
    Json<crate::models::ApiResponse<HubSettingsResponse>>,
) {
    if let Err(error) = app.config_manager.set_hub_config(config.clone()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(crate::models::ApiResponse::error(error)),
        );
    }
    app.hub_agent_manager
        .apply_config(app.clone(), config.clone(), None)
        .await;
    let runtime = runtime_status(&config).await;
    (
        StatusCode::OK,
        Json(crate::models::ApiResponse::success_with_message(
            "Hub settings saved",
            HubSettingsResponse { config, runtime },
        )),
    )
}

pub async fn provision_device(
    State(app): State<AppState>,
    Json(request): Json<DeviceProvisionRequest>,
) -> (
    StatusCode,
    Json<crate::models::ApiResponse<HubSettingsResponse>>,
) {
    match app.hub_agent_manager.provision(app.clone(), &request).await {
        Ok(()) => hub_settings_response(&app, StatusCode::OK, "Hub 接入请求已接受").await,
        Err(error) => (
            if error.contains("已经绑定") {
                StatusCode::CONFLICT
            } else {
                StatusCode::BAD_REQUEST
            },
            Json(crate::models::ApiResponse::error(error)),
        ),
    }
}

pub async fn unbind_hub(
    State(app): State<AppState>,
) -> (
    StatusCode,
    Json<crate::models::ApiResponse<HubSettingsResponse>>,
) {
    match app.hub_agent_manager.unbind(app.clone()).await {
        Ok(()) => hub_settings_response(&app, StatusCode::OK, "已解除 Hub 绑定").await,
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(crate::models::ApiResponse::error(error)),
        ),
    }
}

async fn hub_settings_response(
    app: &AppState,
    status: StatusCode,
    message: &str,
) -> (
    StatusCode,
    Json<crate::models::ApiResponse<HubSettingsResponse>>,
) {
    let config = app.config_manager.get_hub_config();
    let runtime = runtime_status(&config).await;
    (
        status,
        Json(crate::models::ApiResponse::success_with_message(
            message,
            HubSettingsResponse { config, runtime },
        )),
    )
}

pub async fn queue_notification_event(
    config_manager: &crate::config::ConfigManager,
    database: &crate::db::Database,
    event_type: &str,
    event_code: &str,
    summary: String,
    details: Value,
) -> Result<bool, String> {
    let config = config_manager.get_hub_config();
    if !config.enabled {
        return Ok(false);
    }
    let online = runtime_status(&config).await.online;
    let event = HubEventRecord {
        item_id: format!("event-local-{}", Uuid::new_v4()),
        event_type: event_type.to_owned(),
        event_code: event_code.to_owned(),
        occurred_at: Utc::now().to_rfc3339(),
        summary,
        details,
        local_fallback_applied: false,
    };
    database
        .enqueue_hub_event(&event)
        .map_err(|error| error.to_string())?;
    hub_business_wakeup().notify_one();
    Ok(online || config.enabled)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provision_request() -> DeviceProvisionRequest {
        DeviceProvisionRequest {
            hub_url: "https://hub.example.com".into(),
            hub_instance_id: "hub-test".into(),
            hub_version: "0.1.0".into(),
            enrollment_token: "x".repeat(64),
        }
    }

    #[test]
    fn independent_manager_does_not_create_agent_database() {
        let directory = tempfile::tempdir().unwrap();
        let store_path = directory.path().join("hub-agent.db");
        let _manager = HubAgentManager::with_store_path(store_path.clone());
        assert!(!store_path.exists());
    }

    #[test]
    fn lifecycle_changes_for_mode_url_and_fallback_updates() {
        let independent = HubConfig::default();
        let managed = HubConfig {
            enabled: true,
            ..HubConfig::default()
        };
        let connected = HubConfig {
            url: "https://hub.example.com".into(),
            ..managed.clone()
        };
        let fallback_disabled = HubConfig {
            local_fallback_enabled: false,
            ..connected.clone()
        };
        assert_ne!(lifecycle_key(&independent), lifecycle_key(&managed));
        assert_ne!(lifecycle_key(&managed), lifecycle_key(&connected));
        assert_ne!(lifecycle_key(&connected), lifecycle_key(&fallback_disabled));
    }

    #[test]
    fn local_fallback_waits_for_the_full_offline_timeout() {
        let now = Utc::now();
        assert!(!local_fallback_timeout_elapsed(
            Some(now - chrono::Duration::seconds(119)),
            120,
            now
        ));
        assert!(local_fallback_timeout_elapsed(
            Some(now - chrono::Duration::seconds(120)),
            120,
            now
        ));
        assert!(!local_fallback_timeout_elapsed(None, 120, now));
    }

    #[test]
    fn local_automation_switches_between_hub_ownership_and_device_fallback() {
        let now = Utc::now();
        let independent = HubConfig::default();
        assert!(local_automation_scheduling_enabled_at(
            &independent,
            &HubRuntimeStatus::default(),
            now
        ));

        let managed = HubConfig {
            enabled: true,
            ..HubConfig::default()
        };
        let online = HubRuntimeStatus {
            online: true,
            ..Default::default()
        };
        assert!(!local_automation_scheduling_enabled_at(
            &managed, &online, now
        ));

        let waiting = HubRuntimeStatus {
            offline_since: Some(now - chrono::Duration::seconds(119)),
            ..Default::default()
        };
        assert!(!local_automation_scheduling_enabled_at(
            &managed, &waiting, now
        ));

        let fallback = HubRuntimeStatus {
            offline_since: Some(now - chrono::Duration::seconds(120)),
            ..Default::default()
        };
        assert!(local_automation_scheduling_enabled_at(
            &managed, &fallback, now
        ));

        let fallback_disabled = HubConfig {
            local_fallback_enabled: false,
            ..managed
        };
        assert!(!local_automation_scheduling_enabled_at(
            &fallback_disabled,
            &fallback,
            now
        ));
    }

    #[test]
    fn provisioning_requires_managed_unbound_mode_and_valid_hub_credentials() {
        let status = HubRuntimeStatus {
            enabled: true,
            ..Default::default()
        };
        let request = provision_request();
        assert!(validate_provision_request(
            &HubConfig {
                enabled: true,
                ..Default::default()
            },
            &status,
            &request
        )
        .is_ok());
        assert_eq!(
            validate_provision_request(&HubConfig::default(), &status, &request).unwrap_err(),
            "设备当前处于独立运行模式"
        );
        assert_eq!(
            validate_provision_request(
                &HubConfig {
                    enabled: true,
                    url: "https://old-hub.example.com".into(),
                    ..Default::default()
                },
                &status,
                &request
            )
            .unwrap_err(),
            "设备已经绑定 SimAdminHub"
        );
    }

    #[test]
    fn manual_hub_url_accepts_only_an_origin() {
        assert_eq!(
            normalized_hub_url("https://hub.example.com/").unwrap(),
            "https://hub.example.com"
        );
        assert!(normalized_hub_url("ftp://hub.example.com").is_err());
        assert!(normalized_hub_url("https://user@hub.example.com").is_err());
        assert!(normalized_hub_url("https://hub.example.com/path").is_err());
        assert_eq!(mdns_label("设备一"), "simadmin");
    }
}
