use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures_util::{SinkExt, StreamExt};
use reqwest::Url;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use simadmin_protocol::{
    AgentClaimResponse, AgentRegistrationRequest, AgentRegistrationResponse, AgentType,
    CommandAckPayload, CommandPayload, CommandResultPayload, CommandResultStatus,
    ConfigApplyResultPayload, ConfigApplyStatus, ConfigSyncPayload, ConnectionScope, Envelope,
    HeartbeatPayload, MessageAckPayload, SessionReadyPayload,
};
use tokio::sync::{mpsc, Notify};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, http::header, Message},
};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("agent has not been approved yet (pairing code {0})")]
    PairingPending(String),
    #[error("agent credentials are incomplete")]
    MissingCredentials,
    #[error("invalid Hub URL: {0}")]
    InvalidUrl(String),
    #[error("Hub request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("WebSocket failed: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),
    #[error(transparent)]
    Database(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("device execution failed: {0}")]
    Execution(String),
}

pub type AgentResult<T> = Result<T, AgentError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub enabled: bool,
    pub hub_url: String,
    pub installation_id: String,
    pub agent_id: Option<String>,
    pub device_ids: Vec<String>,
    pub token: String,
    pub pairing_code: Option<String>,
    pub agent_type: AgentType,
    pub connection_scope: ConnectionScope,
    pub hostname: String,
    pub version: String,
    pub suggested_device_name: Option<String>,
    #[serde(default)]
    pub enrollment_token: Option<String>,
    #[serde(default)]
    pub hub_instance_id: Option<String>,
    #[serde(default)]
    pub hub_version: Option<String>,
    #[serde(default)]
    pub canonical_hub_url: Option<String>,
    #[serde(default)]
    pub last_connected_at: Option<DateTime<Utc>>,
}

impl AgentConfig {
    pub fn new(
        hub_url: String,
        agent_type: AgentType,
        connection_scope: ConnectionScope,
        hostname: String,
        version: String,
    ) -> Self {
        Self {
            enabled: true,
            hub_url,
            installation_id: format!("install-{}", Uuid::new_v4()),
            agent_id: None,
            device_ids: vec![],
            token: format!(
                "bootstrap_{}{}",
                Uuid::new_v4().simple(),
                Uuid::new_v4().simple()
            ),
            pairing_code: None,
            agent_type,
            connection_scope,
            hostname,
            version,
            suggested_device_name: None,
            enrollment_token: None,
            hub_instance_id: None,
            hub_version: None,
            canonical_hub_url: None,
            last_connected_at: None,
        }
    }
}

pub struct AgentStore {
    connection: Mutex<Connection>,
}

impl AgentStore {
    pub fn open(path: impl AsRef<Path>) -> AgentResult<Self> {
        if let Some(parent) = path
            .as_ref()
            .parent()
            .filter(|value| !value.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path)?;
        connection.execute_batch("PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;
            CREATE TABLE IF NOT EXISTS agent_state (key TEXT PRIMARY KEY, value_json TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS agent_outbox (message_id TEXT PRIMARY KEY, message_type TEXT NOT NULL,
                envelope_json TEXT NOT NULL, created_at TEXT NOT NULL, attempt_count INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE IF NOT EXISTS command_ledger (command_id TEXT PRIMARY KEY, device_id TEXT NOT NULL,
                command_type TEXT NOT NULL, payload_json TEXT NOT NULL, status TEXT NOT NULL,
                result_json TEXT, error_message TEXT, received_at TEXT NOT NULL, finished_at TEXT);
            CREATE TABLE IF NOT EXISTS applied_policies (device_id TEXT PRIMARY KEY, desired_version INTEGER NOT NULL,
                desired_hash TEXT NOT NULL, policy_json TEXT NOT NULL, applied_at TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS agent_dead_letters (message_id TEXT PRIMARY KEY, message_type TEXT NOT NULL,
                envelope_json TEXT NOT NULL, attempt_count INTEGER NOT NULL, error_code TEXT,
                error_message TEXT, failed_at TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS delivered_items (message_type TEXT NOT NULL, item_id TEXT NOT NULL,
                delivered_at TEXT NOT NULL, PRIMARY KEY(message_type,item_id));
            UPDATE command_ledger SET status='unknown', result_json='{}',
                error_message='agent restarted while command outcome was uncertain', finished_at=CURRENT_TIMESTAMP
                WHERE status='running';" )?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn load_config(&self) -> AgentResult<Option<AgentConfig>> {
        let value: Option<String> = self
            .connection
            .lock()
            .unwrap()
            .query_row(
                "SELECT value_json FROM agent_state WHERE key='config'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        value
            .map(|value| serde_json::from_str(&value).map_err(Into::into))
            .transpose()
    }
    pub fn save_config(&self, value: &AgentConfig) -> AgentResult<()> {
        self.connection.lock().unwrap().execute("INSERT INTO agent_state (key,value_json) VALUES ('config',?1) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json", [serde_json::to_string(value)?])?;
        Ok(())
    }
    pub fn enqueue(&self, envelope: &Envelope) -> AgentResult<()> {
        self.connection.lock().unwrap().execute("INSERT OR IGNORE INTO agent_outbox (message_id,message_type,envelope_json,created_at) VALUES (?1,?2,?3,?4)", params![envelope.message_id,envelope.message_type,serde_json::to_string(envelope)?,Utc::now().to_rfc3339()])?;
        Ok(())
    }
    pub fn outbox(&self) -> AgentResult<Vec<Envelope>> {
        let connection = self.connection.lock().unwrap();
        let mut statement = connection
            .prepare("SELECT envelope_json FROM agent_outbox ORDER BY created_at LIMIT 100")?;
        let values = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        values
            .into_iter()
            .map(|value| serde_json::from_str(&value).map_err(Into::into))
            .collect()
    }
    pub fn unsent_outbox(&self) -> AgentResult<Vec<Envelope>> {
        let connection = self.connection.lock().unwrap();
        let mut statement = connection.prepare(
            "SELECT envelope_json FROM agent_outbox
             WHERE attempt_count=0 ORDER BY created_at LIMIT 100",
        )?;
        let values = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        values
            .into_iter()
            .map(|value| serde_json::from_str(&value).map_err(Into::into))
            .collect()
    }
    pub fn reset_outbox_attempts(&self) -> AgentResult<()> {
        self.connection
            .lock()
            .unwrap()
            .execute("UPDATE agent_outbox SET attempt_count=0", [])?;
        Ok(())
    }
    pub fn has_pending_item(&self, message_type: &str, item_id: &str) -> AgentResult<bool> {
        let connection = self.connection.lock().unwrap();
        let mut statement = connection.prepare(
            "SELECT envelope_json FROM agent_outbox WHERE message_type=?1 ORDER BY created_at",
        )?;
        let values = statement
            .query_map([message_type], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        for value in values {
            let envelope: Envelope = serde_json::from_str(&value)?;
            let contains = match message_type {
                "sms_batch" => envelope
                    .decode_payload::<simadmin_protocol::SmsBatchPayload>()?
                    .items
                    .iter()
                    .any(|item| item.item_id == item_id),
                "event_batch" => envelope
                    .decode_payload::<simadmin_protocol::EventBatchPayload>()?
                    .items
                    .iter()
                    .any(|item| item.item_id == item_id),
                _ => false,
            };
            if contains {
                return Ok(true);
            }
        }
        Ok(false)
    }
    pub fn mark_outbox_attempt(&self, message_id: &str) -> AgentResult<()> {
        self.connection.lock().unwrap().execute(
            "UPDATE agent_outbox SET attempt_count=attempt_count+1 WHERE message_id=?1",
            [message_id],
        )?;
        Ok(())
    }
    pub fn mark_outbox_attempts(&self, message_ids: &[String]) -> AgentResult<()> {
        if message_ids.is_empty() {
            return Ok(());
        }
        let mut connection = self.connection.lock().unwrap();
        let transaction = connection.transaction()?;
        for message_id in message_ids {
            transaction.execute(
                "UPDATE agent_outbox SET attempt_count=attempt_count+1 WHERE message_id=?1",
                [message_id],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }
    pub fn reject_message(
        &self,
        message_id: &str,
        error_code: Option<&str>,
        error_message: Option<&str>,
    ) -> AgentResult<()> {
        let mut connection = self.connection.lock().unwrap();
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT OR REPLACE INTO agent_dead_letters
             (message_id,message_type,envelope_json,attempt_count,error_code,error_message,failed_at)
             SELECT message_id,message_type,envelope_json,attempt_count,?2,?3,?4
             FROM agent_outbox WHERE message_id=?1",
            params![message_id, error_code, error_message, Utc::now().to_rfc3339()],
        )?;
        transaction.execute("DELETE FROM agent_outbox WHERE message_id=?1", [message_id])?;
        transaction.commit()?;
        Ok(())
    }
    pub fn acknowledge_message(&self, message_id: &str) -> AgentResult<()> {
        self.connection
            .lock()
            .unwrap()
            .execute("DELETE FROM agent_outbox WHERE message_id=?1", [message_id])?;
        Ok(())
    }
    pub fn message(&self, message_id: &str) -> AgentResult<Option<Envelope>> {
        let value = self
            .connection
            .lock()
            .unwrap()
            .query_row(
                "SELECT envelope_json FROM agent_outbox WHERE message_id=?1",
                [message_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        value
            .map(|value| serde_json::from_str(&value).map_err(Into::into))
            .transpose()
    }
    pub fn mark_items_delivered(&self, message_type: &str, item_ids: &[String]) -> AgentResult<()> {
        let mut connection = self.connection.lock().unwrap();
        let transaction = connection.transaction()?;
        for item_id in item_ids {
            transaction.execute(
                "INSERT OR IGNORE INTO delivered_items (message_type,item_id,delivered_at)
                 VALUES (?1,?2,?3)",
                params![message_type, item_id, Utc::now().to_rfc3339()],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }
    pub fn clear_delivered_items(&self, message_type: &str) -> AgentResult<usize> {
        self.connection
            .lock()
            .unwrap()
            .execute(
                "DELETE FROM delivered_items WHERE message_type=?1",
                [message_type],
            )
            .map_err(Into::into)
    }
    pub fn discard_outbox_type(&self, message_type: &str) -> AgentResult<usize> {
        self.connection
            .lock()
            .unwrap()
            .execute(
                "DELETE FROM agent_outbox WHERE message_type=?1",
                [message_type],
            )
            .map_err(Into::into)
    }
    pub fn retain_undelivered<T>(
        &self,
        message_type: &str,
        items: Vec<T>,
        item_id: impl Fn(&T) -> &str,
    ) -> AgentResult<Vec<T>> {
        let connection = self.connection.lock().unwrap();
        let mut statement = connection.prepare(
            "SELECT EXISTS(SELECT 1 FROM delivered_items WHERE message_type=?1 AND item_id=?2)",
        )?;
        let mut retained = Vec::with_capacity(items.len());
        for item in items {
            let delivered: i64 =
                statement.query_row(params![message_type, item_id(&item)], |row| row.get(0))?;
            if delivered == 0 {
                retained.push(item);
            }
        }
        Ok(retained)
    }
    pub fn command(&self, command_id: &str) -> AgentResult<Option<LedgerCommand>> {
        self.connection.lock().unwrap().query_row("SELECT command_id,device_id,command_type,payload_json,status,result_json,error_message,received_at,finished_at FROM command_ledger WHERE command_id=?1",[command_id],ledger_from_row).optional().map_err(Into::into)
    }
    pub fn persist_command(
        &self,
        device_id: &str,
        payload: &CommandPayload,
    ) -> AgentResult<LedgerCommand> {
        self.connection.lock().unwrap().execute("INSERT OR IGNORE INTO command_ledger (command_id,device_id,command_type,payload_json,status,received_at) VALUES (?1,?2,?3,?4,'accepted',?5)",params![payload.command_id,device_id,payload.command_type,serde_json::to_string(&payload.payload)?,Utc::now().to_rfc3339()])?;
        self.command(&payload.command_id)?
            .ok_or(AgentError::MissingCredentials)
    }
    pub fn accept_command(
        &self,
        device_id: &str,
        payload: &CommandPayload,
        ack: &Envelope,
    ) -> AgentResult<(LedgerCommand, bool)> {
        let payload_json = serde_json::to_string(&payload.payload)?;
        let ack_json = serde_json::to_string(ack)?;
        let now = Utc::now().to_rfc3339();
        let mut connection = self.connection.lock().unwrap();
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT OR IGNORE INTO command_ledger
             (command_id,device_id,command_type,payload_json,status,received_at)
             VALUES (?1,?2,?3,?4,'accepted',?5)",
            params![
                payload.command_id,
                device_id,
                payload.command_type,
                payload_json,
                now
            ],
        )?;
        let ledger = transaction.query_row(
            "SELECT command_id,device_id,command_type,payload_json,status,result_json,
             error_message,received_at,finished_at FROM command_ledger WHERE command_id=?1",
            [&payload.command_id],
            ledger_from_row,
        )?;
        transaction.execute(
            "INSERT OR IGNORE INTO agent_outbox
             (message_id,message_type,envelope_json,created_at) VALUES (?1,?2,?3,?4)",
            params![ack.message_id, ack.message_type, ack_json, now],
        )?;
        let began = if matches!(ledger.status.as_str(), "succeeded" | "failed" | "unknown") {
            false
        } else {
            transaction.execute(
                "UPDATE command_ledger SET status='running'
                 WHERE command_id=?1 AND status='accepted'",
                [&payload.command_id],
            )? == 1
        };
        transaction.commit()?;
        Ok((ledger, began))
    }
    pub fn finish_command(
        &self,
        command_id: &str,
        result: &ExecutionResult,
    ) -> AgentResult<LedgerCommand> {
        self.connection.lock().unwrap().execute("UPDATE command_ledger SET status=?2,result_json=?3,error_message=?4,finished_at=?5 WHERE command_id=?1 AND status NOT IN ('succeeded','failed','unknown')",params![command_id,result.status.as_str(),serde_json::to_string(&result.result)?,result.error_message,Utc::now().to_rfc3339()])?;
        self.command(command_id)?
            .ok_or(AgentError::MissingCredentials)
    }
    pub fn begin_command(&self, command_id: &str) -> AgentResult<bool> {
        Ok(self.connection.lock().unwrap().execute(
            "UPDATE command_ledger SET status='running' WHERE command_id=?1 AND status='accepted'",
            [command_id],
        )? == 1)
    }
    pub fn apply_policy(&self, device_id: &str, payload: &ConfigSyncPayload) -> AgentResult<()> {
        self.connection.lock().unwrap().execute("INSERT INTO applied_policies (device_id,desired_version,desired_hash,policy_json,applied_at) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(device_id) DO UPDATE SET desired_version=excluded.desired_version,desired_hash=excluded.desired_hash,policy_json=excluded.policy_json,applied_at=excluded.applied_at",params![device_id,payload.desired_version,payload.desired_hash,serde_json::to_string(&payload.managed_policy)?,Utc::now().to_rfc3339()])?;
        Ok(())
    }
    pub fn outbox_count(&self) -> AgentResult<u64> {
        Ok(self.connection.lock().unwrap().query_row(
            "SELECT COUNT(*) FROM agent_outbox",
            [],
            |row| row.get::<_, i64>(0),
        )? as u64)
    }
}

#[derive(Debug, Clone)]
pub struct LedgerCommand {
    pub command_id: String,
    pub device_id: String,
    pub command_type: String,
    pub payload: serde_json::Value,
    pub status: String,
    pub result: Option<serde_json::Value>,
    pub error_message: Option<String>,
    pub received_at: String,
    pub finished_at: Option<String>,
}
fn ledger_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LedgerCommand> {
    let payload: String = row.get(3)?;
    let result: Option<String> = row.get(5)?;
    Ok(LedgerCommand {
        command_id: row.get(0)?,
        device_id: row.get(1)?,
        command_type: row.get(2)?,
        payload: serde_json::from_str(&payload).unwrap_or_default(),
        status: row.get(4)?,
        result: result.and_then(|v| serde_json::from_str(&v).ok()),
        error_message: row.get(6)?,
        received_at: row.get(7)?,
        finished_at: row.get(8)?,
    })
}

#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub status: CommandResultStatus,
    pub result: serde_json::Value,
    pub error_message: Option<String>,
}

#[async_trait]
pub trait AgentExecutor: Send + Sync + 'static {
    async fn status_items(
        &self,
        device_ids: &[String],
    ) -> AgentResult<Vec<simadmin_protocol::DeviceStatusItem>>;
    async fn execute(
        &self,
        device_id: &str,
        command: &CommandPayload,
    ) -> AgentResult<ExecutionResult>;
    async fn apply_policy(&self, device_id: &str, policy: &ConfigSyncPayload) -> AgentResult<()>;
    async fn discovery_items(&self) -> AgentResult<Vec<simadmin_protocol::HostDiscoveryItem>> {
        Ok(vec![])
    }
    async fn set_discovery_enabled(&self, _enabled: bool) -> AgentResult<()> {
        Ok(())
    }
    async fn sms_items(
        &self,
        _device_ids: &[String],
    ) -> AgentResult<Vec<simadmin_protocol::SmsItem>> {
        Ok(vec![])
    }
    async fn event_items(
        &self,
        _device_ids: &[String],
    ) -> AgentResult<Vec<simadmin_protocol::EventItem>> {
        Ok(vec![])
    }
    async fn prepare_full_sms_sync(&self) -> AgentResult<()> {
        Ok(())
    }
    async fn handle_message_ack(
        &self,
        _source: &Envelope,
        _ack: &MessageAckPayload,
    ) -> AgentResult<()> {
        Ok(())
    }
    async fn session_state_changed(&self, _online: bool) {}
    async fn session_ready(&self, _session: &SessionReadyPayload) {}
    async fn session_error(&self, _error: &AgentError) {}
    async fn agent_config_changed(&self, _config: &AgentConfig) {}
    async fn configure_credentials(&self, _agent_id: &str, _token: &str) {}
    async fn registration_fingerprint(
        &self,
    ) -> AgentResult<Option<simadmin_protocol::HardwareFingerprintPayload>> {
        Ok(None)
    }
    async fn apply_bindings(
        &self,
        _bindings: &simadmin_protocol::BindingSyncPayload,
    ) -> AgentResult<()> {
        Ok(())
    }
    fn managed_device_count(&self, configured: &[String]) -> usize {
        configured.len()
    }
}

#[derive(Deserialize)]
struct ApiEnvelope<T> {
    data: T,
}

pub struct AgentRuntime<E: AgentExecutor> {
    pub store: Arc<AgentStore>,
    pub executor: Arc<E>,
    pub config: AgentConfig,
    client: reqwest::Client,
    wakeup: Option<mpsc::UnboundedReceiver<()>>,
    business_wakeup: Option<Arc<Notify>>,
    outbox_wakeup: Arc<Notify>,
}
impl<E: AgentExecutor> AgentRuntime<E> {
    pub fn new(store: Arc<AgentStore>, executor: Arc<E>, config: AgentConfig) -> Self {
        Self {
            store,
            executor,
            config,
            client: reqwest::Client::new(),
            wakeup: None,
            business_wakeup: None,
            outbox_wakeup: Arc::new(Notify::new()),
        }
    }
    pub fn with_wakeup(mut self, wakeup: mpsc::UnboundedReceiver<()>) -> Self {
        self.wakeup = Some(wakeup);
        self
    }
    pub fn with_business_wakeup(mut self, wakeup: Arc<Notify>) -> Self {
        self.business_wakeup = Some(wakeup);
        self
    }
    pub async fn register_or_claim(&mut self) -> AgentResult<()> {
        let base = normalize_http_base(&self.config.hub_url)?;
        if self.config.agent_id.is_none() {
            let response = self
                .client
                .post(
                    base.join("agent/register")
                        .map_err(|e| AgentError::InvalidUrl(e.to_string()))?,
                )
                .json(&AgentRegistrationRequest {
                    installation_id: self.config.installation_id.clone(),
                    agent_type: self.config.agent_type,
                    connection_scope: self.config.connection_scope,
                    hostname: self.config.hostname.clone(),
                    version: self.config.version.clone(),
                    suggested_device_name: self.config.suggested_device_name.clone(),
                    bootstrap_token: Some(self.config.token.clone()),
                    enrollment_token: self.config.enrollment_token.clone(),
                    hardware_fingerprint: self.executor.registration_fingerprint().await?,
                })
                .send()
                .await?
                .error_for_status()?
                .json::<ApiEnvelope<AgentRegistrationResponse>>()
                .await?
                .data;
            self.config.agent_id = Some(response.agent_id);
            self.config.pairing_code = Some(response.pairing_code);
            self.config.enrollment_token = None;
            self.store.save_config(&self.config)?;
            self.executor.agent_config_changed(&self.config).await;
        }
        let agent_id = self
            .config
            .agent_id
            .clone()
            .ok_or(AgentError::MissingCredentials)?;
        let mut url = base
            .join("agent/claim")
            .map_err(|e| AgentError::InvalidUrl(e.to_string()))?;
        url.query_pairs_mut().append_pair("agent_id", &agent_id);
        let claim = self
            .client
            .get(url)
            .bearer_auth(&self.config.token)
            .send()
            .await?
            .error_for_status()?
            .json::<ApiEnvelope<AgentClaimResponse>>()
            .await?
            .data;
        if !claim.approved {
            return Err(AgentError::PairingPending(
                self.config.pairing_code.clone().unwrap_or_default(),
            ));
        }
        self.config.device_ids = claim.device_ids;
        self.store.save_config(&self.config)?;
        self.executor.agent_config_changed(&self.config).await;
        self.executor
            .configure_credentials(&agent_id, &self.config.token)
            .await;
        Ok(())
    }
    pub async fn run_forever(mut self) -> AgentResult<()> {
        let mut delay = Duration::from_secs(10);
        loop {
            match self.run_session().await {
                Ok(()) => delay = Duration::from_secs(10),
                Err(error @ AgentError::PairingPending(_)) => {
                    tracing::info!(%error, "Agent is waiting for Hub approval");
                    self.executor.session_error(&error).await;
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    delay = Duration::from_secs(10);
                }
                Err(error) => {
                    tracing::warn!(%error,"Agent session ended");
                    self.executor.session_error(&error).await;
                    tokio::time::sleep(delay).await;
                    delay = (delay * 2).min(Duration::from_secs(600));
                }
            }
        }
    }
    pub async fn run_session(&mut self) -> AgentResult<()> {
        self.register_or_claim().await?;
        let agent_id = self
            .config
            .agent_id
            .clone()
            .ok_or(AgentError::MissingCredentials)?;
        let ws_url = websocket_url(&self.config.hub_url, &agent_id)?;
        let mut request = ws_url.as_str().into_client_request()?;
        request.headers_mut().insert(
            header::AUTHORIZATION,
            format!("Bearer {}", self.config.token)
                .parse()
                .map_err(|error| {
                    AgentError::InvalidUrl(format!("invalid token header: {error}"))
                })?,
        );
        let (stream, _) = connect_async(request).await?;
        let (mut sink, mut source) = stream.split();
        let ready = source
            .next()
            .await
            .ok_or(AgentError::MissingCredentials)??;
        let ready: Envelope = serde_json::from_str(ready.to_text()?)?;
        let session: SessionReadyPayload = ready.decode_payload()?;
        let previous_hub_instance_id = self.config.hub_instance_id.clone();
        if session.hub_instance_id.is_some() && session.hub_instance_id != previous_hub_instance_id
        {
            self.store.clear_delivered_items("sms_batch")?;
            self.executor.prepare_full_sms_sync().await?;
        }
        self.config.hub_instance_id = session.hub_instance_id.clone();
        self.config.hub_version = session.hub_version.clone();
        self.config.canonical_hub_url = session.canonical_public_url.clone();
        self.config.last_connected_at = Some(Utc::now());
        self.store.save_config(&self.config)?;
        self.executor.agent_config_changed(&self.config).await;
        self.executor.session_ready(&session).await;
        self.executor.session_state_changed(true).await;
        let _session_guard = SessionGuard(self.executor.clone());
        self.store.reset_outbox_attempts()?;
        self.enqueue_status().await?;
        self.enqueue_discovery().await?;
        self.flush_outbox(&mut sink).await?;
        let mut heartbeat = tokio::time::interval(Duration::from_secs(u64::from(
            session.heartbeat_interval_seconds,
        )));
        let mut status = tokio::time::interval(Duration::from_secs(30));
        let mut business = tokio::time::interval(Duration::from_secs(5));
        let mut outbox_retry = tokio::time::interval(Duration::from_secs(15));
        outbox_retry.tick().await;
        loop {
            tokio::select! {_=heartbeat.tick()=>{let envelope=Envelope::new("heartbeat",&agent_id,None,None,HeartbeatPayload{agent_type:self.config.agent_type,agent_version:self.config.version.clone(),session_generation:session.session_generation,managed_device_count:self.executor.managed_device_count(&self.config.device_ids) as u32,local_queue_size:self.store.outbox_count()?,timestamp:Utc::now(),host_summary:None})?;sink.send(Message::Text(serde_json::to_string(&envelope)?.into())).await?;self.flush_outbox(&mut sink).await?;},_=status.tick()=>{self.enqueue_status().await?;self.enqueue_discovery().await?;self.flush_outbox(&mut sink).await?;},_=business.tick()=>{self.enqueue_business().await?;self.flush_outbox(&mut sink).await?;},event=receive_notify(&self.business_wakeup)=>{if event {self.enqueue_business().await?;self.flush_outbox(&mut sink).await?;}},_=outbox_retry.tick()=>{self.store.reset_outbox_attempts()?;self.flush_outbox(&mut sink).await?;},_=self.outbox_wakeup.notified()=>{self.flush_outbox(&mut sink).await?;},event=receive_wakeup(&mut self.wakeup)=>{if event {self.enqueue_status().await?;self.enqueue_discovery().await?;self.flush_outbox(&mut sink).await?;}},incoming=source.next()=>{let Some(message)=incoming else{return Ok(())};let message=message?;if message.is_text(){self.handle_server(&agent_id,&mut sink,serde_json::from_str(message.to_text()?)?).await?;}else if message.is_close(){return Ok(())}}}
        }
    }
    async fn enqueue_status(&self) -> AgentResult<()> {
        let items = self.executor.status_items(&self.config.device_ids).await?;
        if !items.is_empty() {
            let envelope = Envelope::new(
                "device_status_batch",
                self.config.agent_id.clone().unwrap_or_default(),
                None,
                None,
                simadmin_protocol::DeviceStatusBatchPayload { items },
            )?;
            self.store.enqueue(&envelope)?;
        }
        Ok(())
    }
    async fn enqueue_discovery(&self) -> AgentResult<()> {
        let items = self.executor.discovery_items().await?;
        if !items.is_empty() {
            let envelope = Envelope::new(
                "host_discovery_batch",
                self.config.agent_id.clone().unwrap_or_default(),
                None,
                None,
                simadmin_protocol::HostDiscoveryBatchPayload { items },
            )?;
            self.store.enqueue(&envelope)?;
        }
        Ok(())
    }
    async fn enqueue_business(&self) -> AgentResult<()> {
        let agent_id = self.config.agent_id.clone().unwrap_or_default();
        let sms = self.store.retain_undelivered(
            "sms_batch",
            self.executor.sms_items(&self.config.device_ids).await?,
            |item| item.item_id.as_str(),
        )?;
        for item in sms {
            if !self.store.has_pending_item("sms_batch", &item.item_id)? {
                self.store.enqueue(&Envelope::new(
                    "sms_batch",
                    &agent_id,
                    Some(item.device_id.clone()),
                    None,
                    simadmin_protocol::SmsBatchPayload { items: vec![item] },
                )?)?;
            }
        }
        let events = self.store.retain_undelivered(
            "event_batch",
            self.executor.event_items(&self.config.device_ids).await?,
            |item| item.item_id.as_str(),
        )?;
        for item in events {
            if !self.store.has_pending_item("event_batch", &item.item_id)? {
                self.store.enqueue(&Envelope::new(
                    "event_batch",
                    &agent_id,
                    Some(item.device_id.clone()),
                    None,
                    simadmin_protocol::EventBatchPayload { items: vec![item] },
                )?)?;
            }
        }
        Ok(())
    }
    async fn flush_outbox<S>(&self, sink: &mut S) -> AgentResult<()>
    where
        S: futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
    {
        let envelopes = self.store.unsent_outbox()?;
        for envelope in &envelopes {
            sink.send(Message::Text(serde_json::to_string(&envelope)?.into()))
                .await?;
        }
        let message_ids = envelopes
            .into_iter()
            .map(|envelope| envelope.message_id)
            .collect::<Vec<_>>();
        self.store.mark_outbox_attempts(&message_ids)?;
        Ok(())
    }
    async fn handle_server<S>(
        &self,
        agent_id: &str,
        sink: &mut S,
        envelope: Envelope,
    ) -> AgentResult<()>
    where
        S: futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
    {
        match envelope.message_type.as_str() {
            "message_ack" => {
                let ack: MessageAckPayload = envelope.decode_payload()?;
                let source = self.store.message(&ack.source_message_id)?;
                if let Some(source) = source.as_ref() {
                    self.executor.handle_message_ack(source, &ack).await?;
                    let accepted = ack
                        .items
                        .iter()
                        .filter(|item| item.accepted)
                        .map(|item| item.item_id.clone())
                        .collect::<Vec<_>>();
                    self.store
                        .mark_items_delivered(&source.message_type, &accepted)?;
                }
                if ack.items.iter().all(|item| item.accepted) {
                    self.store.acknowledge_message(&ack.source_message_id)?;
                } else if let Some(item) = ack.items.iter().find(|item| {
                    !item.accepted
                        && !matches!(item.error_code.as_deref(), Some("internal_error" | "retry"))
                }) {
                    self.store.reject_message(
                        &ack.source_message_id,
                        item.error_code.as_deref(),
                        item.message.as_deref(),
                    )?;
                }
            }
            "command" => {
                let device_id = envelope
                    .device_id
                    .clone()
                    .ok_or(AgentError::MissingCredentials)?;
                let command: CommandPayload = envelope.decode_payload()?;
                let ack = Envelope::new(
                    "command_ack",
                    agent_id,
                    Some(device_id.clone()),
                    Some(envelope.message_id.clone()),
                    CommandAckPayload {
                        command_id: command.command_id.clone(),
                        accepted: true,
                        acknowledged_at: Utc::now(),
                        error_code: None,
                        message: None,
                    },
                )?;
                let (ledger, began) = self.store.accept_command(&device_id, &command, &ack)?;
                self.flush_outbox(sink).await?;
                if matches!(ledger.status.as_str(), "succeeded" | "failed" | "unknown") {
                    enqueue_command_result(
                        &self.store,
                        agent_id,
                        &device_id,
                        &envelope.message_id,
                        &ledger,
                    )?;
                    self.flush_outbox(sink).await?;
                } else if began {
                    let executor = self.executor.clone();
                    let store = self.store.clone();
                    let agent_id = agent_id.to_owned();
                    let source_message_id = envelope.message_id;
                    let outbox_wakeup = self.outbox_wakeup.clone();
                    tokio::spawn(async move {
                        let result = match executor.execute(&device_id, &command).await {
                            Ok(value) => value,
                            Err(error) => ExecutionResult {
                                status: CommandResultStatus::Failed,
                                result: serde_json::json!({}),
                                error_message: Some(error.to_string()),
                            },
                        };
                        match store.finish_command(&command.command_id, &result) {
                            Ok(finished) => {
                                if let Err(error) = enqueue_command_result(
                                    &store,
                                    &agent_id,
                                    &device_id,
                                    &source_message_id,
                                    &finished,
                                ) {
                                    tracing::error!(%error, command_id=%command.command_id, "failed to enqueue command result");
                                } else {
                                    outbox_wakeup.notify_one();
                                }
                            }
                            Err(error) => {
                                tracing::error!(%error, command_id=%command.command_id, "failed to finish command")
                            }
                        }
                    });
                }
            }
            "config_sync" => {
                let device_id = envelope
                    .device_id
                    .clone()
                    .ok_or(AgentError::MissingCredentials)?;
                let policy: ConfigSyncPayload = envelope.decode_payload()?;
                let result = self
                    .executor
                    .apply_policy(&device_id, &policy)
                    .await
                    .and_then(|_| self.store.apply_policy(&device_id, &policy));
                let payload = ConfigApplyResultPayload {
                    desired_version: policy.desired_version,
                    desired_hash: policy.desired_hash,
                    status: if result.is_ok() {
                        ConfigApplyStatus::Applied
                    } else {
                        ConfigApplyStatus::Failed
                    },
                    applied_at: Utc::now(),
                    error_message: result.err().map(|e| e.to_string()),
                };
                let reply = Envelope::new(
                    "config_apply_result",
                    agent_id,
                    Some(device_id),
                    Some(envelope.message_id),
                    payload,
                )?;
                self.store.enqueue(&reply)?;
                self.flush_outbox(sink).await?;
            }
            "binding_sync" => {
                let bindings: simadmin_protocol::BindingSyncPayload = envelope.decode_payload()?;
                let result = self.executor.apply_bindings(&bindings).await;
                let payload = simadmin_protocol::BindingApplyResultPayload {
                    version: bindings.version,
                    status: if result.is_ok() {
                        ConfigApplyStatus::Applied
                    } else {
                        ConfigApplyStatus::Failed
                    },
                    error_message: result.err().map(|e| e.to_string()),
                };
                let reply = Envelope::new(
                    "binding_apply_result",
                    agent_id,
                    None,
                    Some(envelope.message_id),
                    payload,
                )?;
                self.store.enqueue(&reply)?;
                self.flush_outbox(sink).await?;
            }
            "discovery_sync" => {
                let payload: simadmin_protocol::DiscoverySyncPayload = envelope.decode_payload()?;
                self.executor.set_discovery_enabled(payload.enabled).await?;
                if payload.enabled {
                    self.enqueue_discovery().await?;
                } else {
                    self.store.discard_outbox_type("host_discovery_batch")?;
                }
                self.flush_outbox(sink).await?;
            }
            "heartbeat_ack" | "session_ready" => {}
            _ => {}
        }
        Ok(())
    }
}
async fn receive_wakeup(receiver: &mut Option<mpsc::UnboundedReceiver<()>>) -> bool {
    match receiver {
        Some(receiver) => receiver.recv().await.is_some(),
        None => std::future::pending().await,
    }
}

async fn receive_notify(notify: &Option<Arc<Notify>>) -> bool {
    match notify {
        Some(notify) => {
            notify.notified().await;
            true
        }
        None => std::future::pending().await,
    }
}

struct SessionGuard<E: AgentExecutor>(Arc<E>);

impl<E: AgentExecutor> Drop for SessionGuard<E> {
    fn drop(&mut self) {
        let executor = self.0.clone();
        tokio::spawn(async move { executor.session_state_changed(false).await });
    }
}
fn enqueue_command_result(
    store: &AgentStore,
    agent_id: &str,
    device_id: &str,
    source_message_id: &str,
    command: &LedgerCommand,
) -> AgentResult<()> {
    let reply = Envelope::new(
        "command_result",
        agent_id,
        Some(device_id.to_owned()),
        Some(source_message_id.to_owned()),
        CommandResultPayload {
            command_id: command.command_id.clone(),
            status: status_from_str(&command.status),
            finished_at: Utc::now(),
            result: command.result.clone().unwrap_or_default(),
            error_message: command.error_message.clone(),
        },
    )?;
    store.enqueue(&reply)
}
fn status_from_str(value: &str) -> CommandResultStatus {
    match value {
        "succeeded" => CommandResultStatus::Succeeded,
        "unknown" => CommandResultStatus::Unknown,
        _ => CommandResultStatus::Failed,
    }
}
fn normalize_http_base(value: &str) -> AgentResult<Url> {
    let mut url = Url::parse(value).map_err(|e| AgentError::InvalidUrl(e.to_string()))?;
    match url.scheme() {
        "ws" => {
            url.set_scheme("http").ok();
        }
        "wss" => {
            url.set_scheme("https").ok();
        }
        "http" | "https" => {}
        _ => {
            return Err(AgentError::InvalidUrl(
                "scheme must be http(s) or ws(s)".into(),
            ))
        }
    }
    url.set_path("/");
    url.set_query(None);
    Ok(url)
}
fn websocket_url(value: &str, agent_id: &str) -> AgentResult<Url> {
    let mut url = Url::parse(value).map_err(|e| AgentError::InvalidUrl(e.to_string()))?;
    match url.scheme() {
        "http" => {
            url.set_scheme("ws").ok();
        }
        "https" => {
            url.set_scheme("wss").ok();
        }
        "ws" | "wss" => {}
        _ => {
            return Err(AgentError::InvalidUrl(
                "scheme must be http(s) or ws(s)".into(),
            ))
        }
    }
    url.set_path("/agent/ws");
    url.query_pairs_mut().append_pair("agent_id", agent_id);
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn outbox_and_command_ledger_are_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let store = AgentStore::open(dir.path().join("agent.db")).unwrap();
        let envelope =
            Envelope::new("event_batch", "a", None, None, serde_json::json!({})).unwrap();
        store.enqueue(&envelope).unwrap();
        store.enqueue(&envelope).unwrap();
        assert_eq!(store.outbox().unwrap().len(), 1);
        assert_eq!(store.unsent_outbox().unwrap().len(), 1);
        store.mark_outbox_attempt(&envelope.message_id).unwrap();
        assert!(store.unsent_outbox().unwrap().is_empty());
        store.reset_outbox_attempts().unwrap();
        assert_eq!(store.unsent_outbox().unwrap().len(), 1);
        store
            .mark_items_delivered("sms_batch", &["sms-1".into()])
            .unwrap();
        assert!(store
            .retain_undelivered("sms_batch", vec!["sms-1".to_string()], String::as_str)
            .unwrap()
            .is_empty());
        assert_eq!(store.clear_delivered_items("sms_batch").unwrap(), 1);
        assert_eq!(
            store
                .retain_undelivered("sms_batch", vec!["sms-1".to_string()], String::as_str)
                .unwrap(),
            vec!["sms-1"]
        );
        let command = CommandPayload {
            command_id: "c1".into(),
            trace_id: "t1".into(),
            command_type: "send_sms".into(),
            expires_at: Utc::now(),
            payload: serde_json::json!({"content":"one"}),
        };
        store.persist_command("d1", &command).unwrap();
        let changed = CommandPayload {
            payload: serde_json::json!({"content":"two"}),
            ..command.clone()
        };
        let existing = store.persist_command("d1", &changed).unwrap();
        assert_eq!(existing.payload["content"], "one");
        store
            .finish_command(
                "c1",
                &ExecutionResult {
                    status: CommandResultStatus::Succeeded,
                    result: serde_json::json!({"ok":true}),
                    error_message: None,
                },
            )
            .unwrap();
        assert_eq!(store.command("c1").unwrap().unwrap().status, "succeeded");
        let transactional_command = CommandPayload {
            command_id: "c2".into(),
            trace_id: "t2".into(),
            command_type: "device_api_request".into(),
            expires_at: Utc::now(),
            payload: serde_json::json!({"path":"/device"}),
        };
        let command_ack = Envelope::new(
            "command_ack",
            "a",
            Some("d1".into()),
            None,
            serde_json::json!({"command_id":"c2"}),
        )
        .unwrap();
        let (accepted, began) = store
            .accept_command("d1", &transactional_command, &command_ack)
            .unwrap();
        assert_eq!(accepted.status, "accepted");
        assert!(began);
        assert_eq!(store.command("c2").unwrap().unwrap().status, "running");
        let (_, began_again) = store
            .accept_command("d1", &transactional_command, &command_ack)
            .unwrap();
        assert!(!began_again);
        store.acknowledge_message(&command_ack.message_id).unwrap();
        store.enqueue(&envelope).unwrap();
        let partial = MessageAckPayload {
            source_message_id: envelope.message_id.clone(),
            items: vec![simadmin_protocol::ItemAck {
                item_id: "one".into(),
                accepted: false,
                error_code: Some("retry".into()),
                message: None,
                hub_handled: None,
            }],
        };
        if partial.items.iter().all(|item| item.accepted) {
            store
                .acknowledge_message(&partial.source_message_id)
                .unwrap();
        }
        assert_eq!(store.outbox().unwrap().len(), 1);
    }
}
