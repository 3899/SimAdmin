//! 数据库模块
//!
//! 使用 SQLite 存储短信历史记录和通话记录

use chrono::{DateTime, Duration, FixedOffset, NaiveDateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Result, Row};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const BEIJING_UTC_OFFSET_SECONDS: i32 = 8 * 60 * 60;
const SMS_TIMESTAMP_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

/// 短信记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmsMessage {
    pub id: i64,
    pub direction: String,    // "incoming" 或 "outgoing"
    pub phone_number: String, // 发件人或收件人
    pub content: String,      // 短信内容
    pub timestamp: String,    // ISO 8601 格式时间
    pub status: String,       // "pending", "sent", "failed", "received"
    pub pdu: Option<String>,  // 原始 PDU（如果有）
}

/// 通话记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallRecord {
    pub id: i64,
    pub direction: String,        // "incoming" / "outgoing" / "missed"
    pub phone_number: String,     // 电话号码
    pub duration: i64,            // 通话时长（秒）
    pub start_time: String,       // 开始时间 ISO 8601
    pub end_time: Option<String>, // 结束时间 ISO 8601
    pub answered: bool,           // 是否接通
}

/// 短信统计
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct SmsStats {
    pub total: i64,
    pub incoming: i64,
    pub outgoing: i64,
    #[serde(default)]
    pub pushed: i64,
    #[serde(default)]
    pub push_attempted: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationLogEntry {
    pub id: i64,
    pub event_type: String,
    pub status: String,
    pub summary: String,
    pub rule_id: String,
    pub rule_name: String,
    pub channel_id: String,
    pub channel_name: String,
    pub message: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NotificationLogsResponse {
    pub logs: Vec<NotificationLogEntry>,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationLogEntry {
    pub id: i64,
    pub task_id: String,
    pub task_name: String,
    pub task_type: String,
    pub status: String,
    pub detail: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AutomationLogsResponse {
    pub logs: Vec<AutomationLogEntry>,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NotificationStatusCounts {
    pub success: i64,
    pub failed: i64,
    pub quiet_hours: i64,
    pub unmatched: i64,
    pub no_available_channel: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PeriodSmsStats {
    pub incoming: i64,
    pub forwarding: NotificationStatusCounts,
}

pub struct NewNotificationLog<'a> {
    pub event_type: &'a str,
    pub status: &'a str,
    pub summary: &'a str,
    pub rule_id: &'a str,
    pub rule_name: &'a str,
    pub channel_id: &'a str,
    pub channel_name: &'a str,
    pub message: &'a str,
}

pub struct NewNotificationQueueItem<'a> {
    pub status: &'a str,
    pub event_type: &'a str,
    pub event_label: &'a str,
    pub summary: &'a str,
    pub reason: &'a str,
    pub rule_id: &'a str,
    pub rule_name: &'a str,
    pub channel_id: &'a str,
    pub channel_name: &'a str,
    pub channel_type: &'a str,
    pub title: &'a str,
    pub body: &'a str,
    pub next_attempt_at: &'a str,
    pub max_attempts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationQueueEntry {
    pub id: i64,
    pub status: String,
    pub event_type: String,
    pub event_label: String,
    pub summary: String,
    pub reason: String,
    pub channel_id: String,
    pub channel_name: String,
    pub channel_type: String,
    pub rule_id: String,
    pub rule_name: String,
    pub title: String,
    pub body: String,
    pub next_attempt_at: String,
    pub attempt_count: i64,
    pub max_attempts: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NotificationQueueResponse {
    pub items: Vec<NotificationQueueEntry>,
    pub total: i64,
}

/// 通话统计
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct CallStats {
    pub total: i64,
    pub incoming: i64,
    pub outgoing: i64,
    pub missed: i64,
    pub total_duration: i64, // 总通话时长（秒）
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmscCacheEntry {
    pub sms_center: String,
    pub source: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnNumberCacheEntry {
    pub phone_numbers: Vec<String>,
    pub source: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmsStorageCacheEntry {
    pub sms_used: Option<u32>,
    pub sms_total: Option<u32>,
    pub source: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EsimProfileCacheEntry {
    pub iccid: String,
    pub name: Option<String>,
    pub provider: Option<String>,
    pub state: Option<String>,
    pub profile_class: Option<String>,
    pub imsi: Option<String>,
    pub msisdn: Option<String>,
    pub smsc: Option<String>,
    pub smdp: Option<String>,
    pub matching_id: Option<String>,
    pub isdp_aid: Option<String>,
    pub mcc: Option<String>,
    pub mnc: Option<String>,
    pub disable_allowed: Option<bool>,
    pub delete_allowed: Option<bool>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EsimEuiccCacheEntry {
    pub cache_key: String,
    pub eid: String,
    pub status: String,
    pub manufacturer: String,
    pub memory_total_kb: Option<f64>,
    pub memory_available_kb: Option<f64>,
    pub memory_total_customizable: Option<bool>,
    pub raw: String,
    pub updated_at: String,
}

/// 数据库管理器
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

fn beijing_offset() -> FixedOffset {
    FixedOffset::east_opt(BEIJING_UTC_OFFSET_SECONDS).expect("valid Beijing UTC offset")
}

pub fn beijing_sms_now_string() -> String {
    Utc::now()
        .with_timezone(&beijing_offset())
        .format(SMS_TIMESTAMP_FORMAT)
        .to_string()
}

pub fn normalize_sms_timestamp_for_display(timestamp: &str) -> Option<String> {
    let timestamp = timestamp.trim();
    if timestamp.is_empty() {
        return None;
    }

    if let Some(parsed) = parse_sms_timestamp_with_offset(timestamp) {
        return Some(parsed);
    }

    for format in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S"] {
        if let Ok(parsed) = NaiveDateTime::parse_from_str(timestamp, format) {
            return Some(parsed.format(SMS_TIMESTAMP_FORMAT).to_string());
        }
    }

    None
}

fn parse_sms_timestamp_with_offset(timestamp: &str) -> Option<String> {
    let timestamp = timestamp.replace(' ', "T");

    if let Ok(parsed) = DateTime::parse_from_rfc3339(&timestamp) {
        return Some(
            parsed
                .with_timezone(&beijing_offset())
                .format(SMS_TIMESTAMP_FORMAT)
                .to_string(),
        );
    }

    let offset_start = timestamp
        .char_indices()
        .rev()
        .find_map(|(index, ch)| (index > 10 && matches!(ch, '+' | '-')).then_some(index))?;

    let (datetime, offset) = timestamp.split_at(offset_start);
    let normalized_offset = match offset.len() {
        3 => format!("{offset}:00"),
        5 if !offset.contains(':') => format!("{}:{}", &offset[..3], &offset[3..]),
        _ => offset.to_string(),
    };
    let candidate = format!("{datetime}{normalized_offset}");

    DateTime::parse_from_rfc3339(&candidate).ok().map(|parsed| {
        parsed
            .with_timezone(&beijing_offset())
            .format(SMS_TIMESTAMP_FORMAT)
            .to_string()
    })
}

fn sms_timestamp_for_storage(timestamp: &str) -> String {
    normalize_sms_timestamp_for_display(timestamp).unwrap_or_else(beijing_sms_now_string)
}

fn sms_timestamp_for_display(timestamp: String) -> String {
    normalize_sms_timestamp_for_display(&timestamp).unwrap_or(timestamp)
}

fn notification_log_date_bound(value: &str, suffix: &str) -> String {
    let value = value.trim().replace('/', "-");
    if value.is_empty() {
        String::new()
    } else if value.len() <= 10 {
        format!("{value} {suffix}")
    } else {
        value
    }
}

fn notification_log_start_bound(value: &str) -> String {
    notification_log_date_bound(value, "00:00:00")
}

fn notification_log_end_bound(value: &str) -> String {
    notification_log_date_bound(value, "23:59:59")
}

fn sms_message_from_row(row: &Row<'_>) -> Result<SmsMessage> {
    let timestamp: String = row.get(4)?;
    Ok(SmsMessage {
        id: row.get(0)?,
        direction: row.get(1)?,
        phone_number: row.get(2)?,
        content: row.get(3)?,
        timestamp: sms_timestamp_for_display(timestamp),
        status: row.get(5)?,
        pdu: row.get(6)?,
    })
}

fn normalize_existing_sms_timestamps(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare("SELECT id, timestamp FROM sms_messages")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut updates = Vec::new();
    for row in rows {
        let (id, timestamp) = row?;
        if let Some(normalized) = normalize_sms_timestamp_for_display(&timestamp) {
            if normalized != timestamp {
                updates.push((id, normalized));
            }
        }
    }
    drop(stmt);

    for (id, timestamp) in updates {
        conn.execute(
            "UPDATE sms_messages SET timestamp = ?1 WHERE id = ?2",
            params![timestamp, id],
        )?;
    }

    Ok(())
}

fn non_empty_option(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn table_has_column(conn: &Connection, table_name: &str, column_name: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table_name})"))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;

    for row in rows {
        if row? == column_name {
            return Ok(true);
        }
    }

    Ok(false)
}

impl Database {
    /// 创建或打开数据库
    pub fn new(db_path: PathBuf) -> Result<Self> {
        let conn = Connection::open(db_path)?;

        // 创建短信表（如果不存在）
        conn.execute(
            "CREATE TABLE IF NOT EXISTS sms_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                direction TEXT NOT NULL,
                phone_number TEXT NOT NULL,
                content TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                status TEXT NOT NULL,
                notification_status TEXT NOT NULL DEFAULT 'pending',
                pdu TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        if !table_has_column(&conn, "sms_messages", "notification_status")? {
            conn.execute(
                "ALTER TABLE sms_messages
                 ADD COLUMN notification_status TEXT NOT NULL DEFAULT 'pending'",
                [],
            )?;
        }

        // 创建短信索引
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_sms_timestamp ON sms_messages(timestamp DESC)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_sms_phone ON sms_messages(phone_number)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_sms_notification_status ON sms_messages(notification_status)",
            [],
        )?;
        normalize_existing_sms_timestamps(&conn)?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS notification_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                event_type TEXT NOT NULL,
                status TEXT NOT NULL,
                summary TEXT NOT NULL,
                rule_id TEXT NOT NULL,
                rule_name TEXT NOT NULL,
                channel_id TEXT NOT NULL,
                channel_name TEXT NOT NULL,
                message TEXT NOT NULL,
                created_at TEXT NOT NULL
            )",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_notification_logs_created_at ON notification_logs(created_at DESC)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_notification_logs_type_status ON notification_logs(event_type, status)",
            [],
        )?;

        // 创建通话记录表（如果不存在）
        conn.execute(
            "CREATE TABLE IF NOT EXISTS notification_queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                status TEXT NOT NULL,
                event_type TEXT NOT NULL,
                event_label TEXT NOT NULL,
                summary TEXT NOT NULL,
                reason TEXT NOT NULL DEFAULT '',
                rule_id TEXT NOT NULL DEFAULT '',
                rule_name TEXT NOT NULL DEFAULT '',
                channel_id TEXT NOT NULL,
                channel_name TEXT NOT NULL,
                channel_type TEXT NOT NULL,
                title TEXT NOT NULL DEFAULT '',
                body TEXT NOT NULL DEFAULT '',
                next_attempt_at TEXT NOT NULL,
                attempt_count INTEGER NOT NULL DEFAULT 0,
                max_attempts INTEGER NOT NULL DEFAULT 5,
                last_error TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                expires_at TEXT NOT NULL DEFAULT ''
            )",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_notification_queue_status_next_attempt
             ON notification_queue(status, next_attempt_at, id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_notification_queue_channel_status
             ON notification_queue(channel_id, status, next_attempt_at)",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS call_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                direction TEXT NOT NULL,
                phone_number TEXT NOT NULL,
                duration INTEGER DEFAULT 0,
                start_time TEXT NOT NULL,
                end_time TEXT,
                answered INTEGER DEFAULT 0,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // 创建通话记录索引
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_call_start_time ON call_history(start_time DESC)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_call_phone ON call_history(phone_number)",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS smsc_cache (
                identity_key TEXT PRIMARY KEY,
                iccid TEXT,
                imsi TEXT,
                operator_id TEXT,
                sms_center TEXT NOT NULL,
                source TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS own_number_cache (
                identity_key TEXT PRIMARY KEY,
                iccid TEXT,
                imsi TEXT,
                operator_id TEXT,
                phone_numbers TEXT NOT NULL,
                source TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS sms_storage_cache (
                identity_key TEXT PRIMARY KEY,
                iccid TEXT,
                imsi TEXT,
                operator_id TEXT,
                sms_used INTEGER,
                sms_total INTEGER,
                source TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS esim_profile_cache (
                iccid TEXT PRIMARY KEY,
                name TEXT,
                provider TEXT,
                profile_class TEXT,
                imsi TEXT,
                msisdn TEXT,
                smsc TEXT,
                smdp TEXT,
                isdp_aid TEXT,
                mcc TEXT,
                mnc TEXT,
                updated_at TEXT NOT NULL
            )",
            [],
        )?;

        if !table_has_column(&conn, "esim_profile_cache", "matching_id")? {
            conn.execute(
                "ALTER TABLE esim_profile_cache ADD COLUMN matching_id TEXT",
                [],
            )?;
        }
        if !table_has_column(&conn, "esim_profile_cache", "state")? {
            conn.execute("ALTER TABLE esim_profile_cache ADD COLUMN state TEXT", [])?;
        }
        if !table_has_column(&conn, "esim_profile_cache", "disable_allowed")? {
            conn.execute(
                "ALTER TABLE esim_profile_cache ADD COLUMN disable_allowed INTEGER",
                [],
            )?;
        }
        if !table_has_column(&conn, "esim_profile_cache", "delete_allowed")? {
            conn.execute(
                "ALTER TABLE esim_profile_cache ADD COLUMN delete_allowed INTEGER",
                [],
            )?;
        }

        conn.execute(
            "CREATE TABLE IF NOT EXISTS esim_euicc_cache (
                cache_key TEXT PRIMARY KEY,
                eid TEXT,
                status TEXT,
                manufacturer TEXT,
                memory_total_kb REAL,
                memory_available_kb REAL,
                memory_total_customizable INTEGER,
                raw TEXT,
                updated_at TEXT NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS auth_config (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS auth_sessions (
                session_hash TEXT PRIMARY KEY,
                created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_auth_sessions_expires_at ON auth_sessions(expires_at)",
            [],
        )?;

        // 创建自动化运行日志表
        conn.execute(
            "CREATE TABLE IF NOT EXISTS automation_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                task_id TEXT NOT NULL,
                task_name TEXT NOT NULL,
                task_type TEXT NOT NULL,
                status TEXT NOT NULL,
                detail TEXT NOT NULL,
                created_at TEXT NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_automation_logs_created_at ON automation_logs(created_at DESC)",
            [],
        )?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub(crate) fn with_connection<T, F>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> Result<T>,
    {
        let conn = self.conn.lock().unwrap();
        f(&conn)
    }

    pub(crate) fn with_transaction<T, F>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&rusqlite::Transaction<'_>) -> Result<T>,
    {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let result = f(&tx)?;
        tx.commit()?;
        Ok(result)
    }

    // ==================== 认证相关方法 ====================

    pub fn auth_is_configured(&self) -> Result<bool> {
        Ok(self.get_auth_config_value("admin_password_hash")?.is_some())
    }

    pub fn get_auth_config_value(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT value FROM auth_config WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()
    }

    pub fn set_auth_config_value(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().timestamp();
        conn.execute(
            "INSERT INTO auth_config (key, value, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                updated_at = excluded.updated_at",
            params![key, value, now],
        )?;
        Ok(())
    }

    pub fn replace_admin_password_hash(&self, password_hash: &str) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let now = Utc::now().timestamp();
        tx.execute(
            "INSERT INTO auth_config (key, value, updated_at)
             VALUES ('admin_password_hash', ?1, ?2)
             ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                updated_at = excluded.updated_at",
            params![password_hash, now],
        )?;
        tx.execute("DELETE FROM auth_sessions", [])?;
        tx.commit()?;
        Ok(())
    }

    pub fn clear_admin_auth(&self) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM auth_config WHERE key = 'admin_password_hash'",
            [],
        )?;
        tx.execute("DELETE FROM auth_sessions", [])?;
        tx.commit()?;
        Ok(())
    }

    pub fn insert_auth_session(&self, session_hash: &str, ttl_seconds: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().timestamp();
        conn.execute(
            "INSERT INTO auth_sessions (session_hash, created_at, expires_at)
             VALUES (?1, ?2, ?3)",
            params![session_hash, now, now + ttl_seconds],
        )?;
        conn.execute(
            "DELETE FROM auth_sessions WHERE expires_at <= ?1",
            params![now],
        )?;
        Ok(())
    }

    pub fn auth_session_valid(&self, session_hash: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().timestamp();
        conn.execute(
            "DELETE FROM auth_sessions WHERE expires_at <= ?1",
            params![now],
        )?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM auth_sessions
             WHERE session_hash = ?1 AND expires_at > ?2",
            params![session_hash, now],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn delete_auth_session(&self, session_hash: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM auth_sessions WHERE session_hash = ?1",
            params![session_hash],
        )?;
        Ok(())
    }

    pub fn clear_auth_sessions(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM auth_sessions", [])?;
        Ok(())
    }

    pub fn refresh_auth_session(&self, session_hash: &str, ttl_seconds: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().timestamp();
        conn.execute(
            "UPDATE auth_sessions SET expires_at = ?1 WHERE session_hash = ?2",
            params![now + ttl_seconds, session_hash],
        )?;
        Ok(())
    }

    // ==================== 短信相关方法 ====================

    /// 插入新短信
    pub fn insert_sms(
        &self,
        direction: &str,
        phone_number: &str,
        content: &str,
        status: &str,
        pdu: Option<&str>,
    ) -> Result<i64> {
        let timestamp = beijing_sms_now_string();
        self.insert_sms_at(direction, phone_number, content, &timestamp, status, pdu)
    }

    pub fn insert_sms_at(
        &self,
        direction: &str,
        phone_number: &str,
        content: &str,
        timestamp: &str,
        status: &str,
        pdu: Option<&str>,
    ) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let timestamp = sms_timestamp_for_storage(timestamp);
        conn.execute(
            "INSERT INTO sms_messages (direction, phone_number, content, timestamp, status, pdu)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![direction, phone_number, content, timestamp, status, pdu],
        )?;

        Ok(conn.last_insert_rowid())
    }

    /// Check whether an SMS marker has already been stored.
    pub fn sms_exists_by_pdu(&self, pdu: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sms_messages WHERE pdu = ?1",
            params![pdu],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn incoming_sms_exists_by_timestamp(
        &self,
        phone_number: &str,
        content: &str,
        timestamp: &str,
    ) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let normalized_timestamp = normalize_sms_timestamp_for_display(timestamp)
            .unwrap_or_else(|| timestamp.trim().to_string());
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sms_messages
             WHERE direction = 'incoming'
               AND phone_number = ?1
               AND content = ?2
               AND (timestamp = ?3 OR timestamp = ?4)",
            params![phone_number, content, timestamp, normalized_timestamp],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn incoming_sms_exists_by_legacy_content(
        &self,
        phone_number: &str,
        content: &str,
    ) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sms_messages
             WHERE direction = 'incoming'
               AND phone_number = ?1
               AND content = ?2
               AND (pdu IS NULL OR pdu NOT LIKE 'mmfp:%')",
            params![phone_number, content],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// 获取所有短信（分页）
    pub fn get_sms_messages(
        &self,
        limit: i64,
        offset: i64,
        direction: Option<&str>,
    ) -> Result<Vec<SmsMessage>> {
        let conn = self.conn.lock().unwrap();
        match direction {
            Some(direction) => {
                let mut stmt = conn.prepare(
                    "SELECT id, direction, phone_number, content, timestamp, status, pdu
                     FROM sms_messages
                     WHERE direction = ?1
                     ORDER BY timestamp DESC, id DESC
                     LIMIT ?2 OFFSET ?3",
                )?;

                let messages =
                    stmt.query_map(params![direction, limit, offset], sms_message_from_row)?;

                let mut result = Vec::new();
                for message in messages {
                    result.push(message?);
                }

                Ok(result)
            }
            None => {
                let mut stmt = conn.prepare(
                    "SELECT id, direction, phone_number, content, timestamp, status, pdu
                     FROM sms_messages
                     ORDER BY timestamp DESC, id DESC
                     LIMIT ?1 OFFSET ?2",
                )?;

                let messages = stmt.query_map(params![limit, offset], sms_message_from_row)?;

                let mut result = Vec::new();
                for message in messages {
                    result.push(message?);
                }

                Ok(result)
            }
        }
    }

    /// 获取与特定号码的对话历史
    pub fn get_sms_conversation(&self, phone_number: &str, limit: i64) -> Result<Vec<SmsMessage>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, direction, phone_number, content, timestamp, status, pdu
             FROM sms_messages
             WHERE phone_number = ?1
             ORDER BY timestamp DESC, id DESC
             LIMIT ?2",
        )?;

        let messages = stmt.query_map(params![phone_number, limit], sms_message_from_row)?;

        let mut result = Vec::new();
        for message in messages {
            result.push(message?);
        }

        Ok(result)
    }

    /// 更新短信通知转发状态："pending", "success", "failed", "skipped"
    pub fn update_sms_notification_status(&self, id: i64, status: &str) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE sms_messages SET notification_status = ?1 WHERE id = ?2",
            params![status, id],
        )
    }

    /// 获取短信统计
    pub fn insert_notification_log(&self, log: NewNotificationLog<'_>) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO notification_logs (
                event_type, status, summary, rule_id, rule_name,
                channel_id, channel_name, message, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                log.event_type,
                log.status,
                log.summary,
                log.rule_id,
                log.rule_name,
                log.channel_id,
                log.channel_name,
                log.message,
                beijing_sms_now_string(),
            ],
        )
    }

    pub fn insert_notification_queue_item(
        &self,
        item: NewNotificationQueueItem<'_>,
    ) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let now = beijing_sms_now_string();
        conn.execute(
            "INSERT INTO notification_queue (
                status, event_type, event_label, summary, reason,
                rule_id, rule_name, channel_id, channel_name, channel_type,
                title, body, next_attempt_at, max_attempts, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15)",
            params![
                item.status,
                item.event_type,
                item.event_label,
                item.summary,
                item.reason,
                item.rule_id,
                item.rule_name,
                item.channel_id,
                item.channel_name,
                item.channel_type,
                item.title,
                item.body,
                item.next_attempt_at,
                item.max_attempts,
                now,
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn notification_channel_success_count_since(
        &self,
        channel_id: &str,
        since: &str,
    ) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM notification_logs
             WHERE status = 'success'
               AND channel_id = ?1
               AND created_at >= ?2",
            params![channel_id, since],
            |row| row.get(0),
        )
    }

    pub fn get_notification_logs(
        &self,
        event_type: &str,
        status: &str,
        query: &str,
        start_date: &str,
        end_date: &str,
        limit: i64,
        offset: i64,
    ) -> Result<NotificationLogsResponse> {
        let conn = self.conn.lock().unwrap();
        let limit = limit.clamp(1, 200);
        let offset = offset.max(0);
        let event_type = event_type.trim();
        let status = status.trim();
        let query = query.trim();
        let start_at = notification_log_start_bound(start_date);
        let end_at = notification_log_end_bound(end_date);

        let total = conn.query_row(
            "SELECT COUNT(*) FROM notification_logs
             WHERE (?1 = '' OR event_type = ?1)
               AND (?2 = '' OR status = ?2)
               AND (
                    ?3 = ''
                    OR summary LIKE '%' || ?3 || '%'
                    OR rule_name LIKE '%' || ?3 || '%'
                    OR channel_name LIKE '%' || ?3 || '%'
                    OR message LIKE '%' || ?3 || '%'
               )
               AND (?4 = '' OR created_at >= ?4)
               AND (?5 = '' OR created_at <= ?5)",
            params![event_type, status, query, start_at, end_at],
            |row| row.get(0),
        )?;

        let mut stmt = conn.prepare(
            "SELECT id, event_type, status, summary, rule_id, rule_name,
                    channel_id, channel_name, message, created_at
             FROM notification_logs
             WHERE (?1 = '' OR event_type = ?1)
               AND (?2 = '' OR status = ?2)
               AND (
                    ?3 = ''
                    OR summary LIKE '%' || ?3 || '%'
                    OR rule_name LIKE '%' || ?3 || '%'
                    OR channel_name LIKE '%' || ?3 || '%'
                    OR message LIKE '%' || ?3 || '%'
               )
               AND (?4 = '' OR created_at >= ?4)
               AND (?5 = '' OR created_at <= ?5)
             ORDER BY id DESC
             LIMIT ?6 OFFSET ?7",
        )?;

        let rows = stmt.query_map(
            params![event_type, status, query, start_at, end_at, limit, offset],
            |row| {
                Ok(NotificationLogEntry {
                    id: row.get(0)?,
                    event_type: row.get(1)?,
                    status: row.get(2)?,
                    summary: row.get(3)?,
                    rule_id: row.get(4)?,
                    rule_name: row.get(5)?,
                    channel_id: row.get(6)?,
                    channel_name: row.get(7)?,
                    message: row.get(8)?,
                    created_at: row.get(9)?,
                })
            },
        )?;

        let mut logs = Vec::new();
        for row in rows {
            logs.push(row?);
        }

        Ok(NotificationLogsResponse { logs, total })
    }

    pub fn clear_notification_logs(
        &self,
        event_type: &str,
        status: &str,
        start_date: &str,
        end_date: &str,
    ) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let event_type = event_type.trim();
        let status = status.trim();
        let start_at = notification_log_start_bound(start_date);
        let end_at = notification_log_end_bound(end_date);
        conn.execute(
            "DELETE FROM notification_logs
             WHERE (?1 = '' OR event_type = ?1)
               AND (?2 = '' OR status = ?2)
               AND (?3 = '' OR created_at >= ?3)
               AND (?4 = '' OR created_at <= ?4)",
            params![event_type, status, start_at, end_at],
        )
    }

    pub fn get_notification_queue(&self, limit: i64) -> Result<NotificationQueueResponse> {
        let conn = self.conn.lock().unwrap();
        let limit = limit.clamp(1, 500);

        let total = conn.query_row(
            "SELECT COUNT(*) FROM notification_queue
             WHERE status IN ('pending', 'scheduled', 'retrying', 'sending', 'failed')",
            [],
            |row| row.get(0),
        )?;

        let mut stmt = conn.prepare(
            "SELECT id, status, event_type, event_label, summary,
                    COALESCE(NULLIF(last_error, ''), reason) AS display_reason,
                    channel_id, channel_name, channel_type, rule_id, rule_name,
                    title, body, next_attempt_at,
                    attempt_count, max_attempts, created_at, updated_at
             FROM notification_queue
             WHERE status IN ('pending', 'scheduled', 'retrying', 'sending', 'failed')
             ORDER BY next_attempt_at ASC, id ASC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok(NotificationQueueEntry {
                id: row.get(0)?,
                status: row.get(1)?,
                event_type: row.get(2)?,
                event_label: row.get(3)?,
                summary: row.get(4)?,
                reason: row.get(5)?,
                channel_id: row.get(6)?,
                channel_name: row.get(7)?,
                channel_type: row.get(8)?,
                rule_id: row.get(9)?,
                rule_name: row.get(10)?,
                title: row.get(11)?,
                body: row.get(12)?,
                next_attempt_at: row.get(13)?,
                attempt_count: row.get(14)?,
                max_attempts: row.get(15)?,
                created_at: row.get(16)?,
                updated_at: row.get(17)?,
            })
        })?;

        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }

        Ok(NotificationQueueResponse { items, total })
    }

    pub fn retry_notification_queue_item(&self, id: i64) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let now = beijing_sms_now_string();
        conn.execute(
            "UPDATE notification_queue
             SET status = 'pending',
                 attempt_count = 0,
                 next_attempt_at = ?1,
                 last_error = '',
                 updated_at = ?1
             WHERE id = ?2
               AND status IN ('pending', 'scheduled', 'retrying', 'sending', 'failed')",
            params![now, id],
        )
    }

    pub fn delete_notification_queue_item(&self, id: i64) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE notification_queue
             SET status = 'cancelled',
                 updated_at = ?1
             WHERE id = ?2
               AND status IN ('pending', 'scheduled', 'retrying', 'sending', 'failed')",
            params![beijing_sms_now_string(), id],
        )
    }

    pub fn retry_all_notification_queue_items(&self) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let now = beijing_sms_now_string();
        conn.execute(
            "UPDATE notification_queue
             SET status = 'pending',
                 attempt_count = 0,
                 next_attempt_at = ?1,
                 last_error = '',
                 updated_at = ?1
             WHERE status IN ('pending', 'scheduled', 'retrying', 'sending', 'failed')",
            params![now],
        )
    }

    pub fn clear_active_notification_queue(&self) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE notification_queue
             SET status = 'cancelled',
                 updated_at = ?1
             WHERE status IN ('pending', 'scheduled', 'retrying', 'sending', 'failed')",
            params![beijing_sms_now_string()],
        )
    }

    pub fn get_due_notification_queue_items(
        &self,
        limit: i64,
    ) -> Result<Vec<NotificationQueueEntry>> {
        let conn = self.conn.lock().unwrap();
        let now = beijing_sms_now_string();
        let limit = limit.clamp(1, 100);
        let mut stmt = conn.prepare(
            "SELECT id, status, event_type, event_label, summary,
                    COALESCE(NULLIF(last_error, ''), reason) AS display_reason,
                    channel_id, channel_name, channel_type, rule_id, rule_name,
                    title, body, next_attempt_at,
                    attempt_count, max_attempts, created_at, updated_at
             FROM notification_queue
             WHERE status IN ('pending', 'scheduled', 'retrying')
               AND next_attempt_at <= ?1
             ORDER BY next_attempt_at ASC, id ASC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![now, limit], |row| {
            Ok(NotificationQueueEntry {
                id: row.get(0)?,
                status: row.get(1)?,
                event_type: row.get(2)?,
                event_label: row.get(3)?,
                summary: row.get(4)?,
                reason: row.get(5)?,
                channel_id: row.get(6)?,
                channel_name: row.get(7)?,
                channel_type: row.get(8)?,
                rule_id: row.get(9)?,
                rule_name: row.get(10)?,
                title: row.get(11)?,
                body: row.get(12)?,
                next_attempt_at: row.get(13)?,
                attempt_count: row.get(14)?,
                max_attempts: row.get(15)?,
                created_at: row.get(16)?,
                updated_at: row.get(17)?,
            })
        })?;

        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }

    pub fn mark_notification_queue_sending(&self, id: i64) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE notification_queue
             SET status = 'sending',
                 updated_at = ?1
             WHERE id = ?2
               AND status IN ('pending', 'scheduled', 'retrying')",
            params![beijing_sms_now_string(), id],
        )
    }

    pub fn mark_notification_queue_sent(&self, id: i64) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE notification_queue
             SET status = 'sent',
                 last_error = '',
                 updated_at = ?1
             WHERE id = ?2",
            params![beijing_sms_now_string(), id],
        )
    }

    pub fn mark_notification_queue_retry(
        &self,
        id: i64,
        last_error: &str,
        next_attempt_at: &str,
    ) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let now = beijing_sms_now_string();
        conn.execute(
            "UPDATE notification_queue
             SET status = 'retrying',
                 attempt_count = attempt_count + 1,
                 last_error = ?1,
                 next_attempt_at = ?2,
                 updated_at = ?3
             WHERE id = ?4",
            params![last_error, next_attempt_at, now, id],
        )
    }

    pub fn mark_notification_queue_scheduled(
        &self,
        id: i64,
        reason: &str,
        next_attempt_at: &str,
    ) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let now = beijing_sms_now_string();
        conn.execute(
            "UPDATE notification_queue
             SET status = 'scheduled',
                 reason = ?1,
                 next_attempt_at = ?2,
                 updated_at = ?3
             WHERE id = ?4",
            params![reason, next_attempt_at, now, id],
        )
    }

    pub fn mark_notification_queue_failed(&self, id: i64, last_error: &str) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE notification_queue
             SET status = 'failed',
                 attempt_count = attempt_count + 1,
                 last_error = ?1,
                 updated_at = ?2
             WHERE id = ?3",
            params![last_error, beijing_sms_now_string(), id],
        )
    }

    pub fn cleanup_notification_logs(
        &self,
        retention_days: Option<u32>,
        max_entries: Option<u32>,
    ) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let mut deleted = 0usize;

        if let Some(days) = retention_days.filter(|days| *days > 0) {
            let cutoff = Utc::now()
                .with_timezone(&beijing_offset())
                .checked_sub_signed(Duration::days(i64::from(days)))
                .unwrap_or_else(|| Utc::now().with_timezone(&beijing_offset()))
                .format(SMS_TIMESTAMP_FORMAT)
                .to_string();
            deleted += conn.execute(
                "DELETE FROM notification_logs WHERE created_at < ?1",
                params![cutoff],
            )?;
        }

        if let Some(max_entries) = max_entries.filter(|max_entries| *max_entries > 0) {
            deleted += conn.execute(
                "DELETE FROM notification_logs
                 WHERE id NOT IN (
                    SELECT id FROM notification_logs
                    ORDER BY id DESC
                    LIMIT ?1
                 )",
                params![i64::from(max_entries)],
            )?;
        }

        Ok(deleted)
    }

    pub fn notification_status_counts(
        &self,
        event_type: &str,
        since: Option<&str>,
    ) -> Result<NotificationStatusCounts> {
        let conn = self.conn.lock().unwrap();
        let mut counts = NotificationStatusCounts::default();
        let since = since.unwrap_or("").trim();
        let mut stmt = conn.prepare(
            "SELECT status, COUNT(*)
             FROM notification_logs
             WHERE event_type = ?1
               AND (?2 = '' OR created_at >= ?2)
             GROUP BY status",
        )?;
        let rows = stmt.query_map(params![event_type, since], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (status, count) = row?;
            match status.as_str() {
                "success" => counts.success = count,
                "failed" => counts.failed = count,
                "quiet_hours" => counts.quiet_hours = count,
                "unmatched" => counts.unmatched = count,
                "no_available_channel" => counts.no_available_channel = count,
                _ => {}
            }
        }
        Ok(counts)
    }

    pub fn period_sms_stats(&self, since: Option<&str>) -> Result<PeriodSmsStats> {
        let conn = self.conn.lock().unwrap();
        let since = since.unwrap_or("").trim();
        let incoming: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sms_messages
             WHERE direction = 'incoming'
               AND status = 'received'
               AND (?1 = '' OR timestamp >= ?1)",
            params![since],
            |row| row.get(0),
        )?;
        drop(conn);
        let forwarding = self.notification_status_counts("sms", Some(since))?;
        Ok(PeriodSmsStats {
            incoming,
            forwarding,
        })
    }

    pub fn get_sms_stats(&self) -> Result<SmsStats> {
        let conn = self.conn.lock().unwrap();

        let total: i64 =
            conn.query_row("SELECT COUNT(*) FROM sms_messages", [], |row| row.get(0))?;

        let incoming: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sms_messages
             WHERE direction = 'incoming' AND status = 'received'",
            [],
            |row| row.get(0),
        )?;

        let outgoing: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sms_messages WHERE direction = 'outgoing'",
            [],
            |row| row.get(0),
        )?;

        let pushed: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sms_messages
             WHERE direction = 'incoming'
               AND status = 'received'
               AND notification_status = 'success'",
            [],
            |row| row.get(0),
        )?;

        let push_attempted: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sms_messages
             WHERE direction = 'incoming'
               AND status = 'received'
               AND notification_status IN ('success', 'failed')",
            [],
            |row| row.get(0),
        )?;

        Ok(SmsStats {
            total,
            incoming,
            outgoing,
            pushed,
            push_attempted,
        })
    }

    /// 删除所有短信
    pub fn clear_all_sms(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM sms_messages", [])?;
        Ok(())
    }

    /// 删除单条短信
    pub fn delete_sms(&self, id: i64) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM sms_messages WHERE id = ?1", params![id])
    }

    /// 删除一个对话的所有短信
    pub fn delete_sms_conversation(&self, phone_number: &str) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM sms_messages WHERE phone_number = ?1",
            params![phone_number],
        )
    }

    /// 按短信 ID 和对话号码批量删除
    pub fn delete_sms_batch(&self, ids: &[i64], phone_numbers: &[String]) -> Result<usize> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let mut deleted = 0usize;

        for phone_number in phone_numbers {
            deleted += tx.execute(
                "DELETE FROM sms_messages WHERE phone_number = ?1",
                params![phone_number],
            )?;
        }

        for id in ids {
            deleted += tx.execute("DELETE FROM sms_messages WHERE id = ?1", params![id])?;
        }

        tx.commit()?;
        Ok(deleted)
    }

    pub fn vacuum(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch("VACUUM")?;
        Ok(())
    }

    // ==================== SMSC cache ====================

    pub fn upsert_smsc_cache(
        &self,
        identity_key: &str,
        iccid: &str,
        imsi: &str,
        operator_id: &str,
        sms_center: &str,
        source: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let updated_at = Utc::now().to_rfc3339();
        conn.execute(
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
                identity_key,
                iccid,
                imsi,
                operator_id,
                sms_center,
                source,
                updated_at
            ],
        )?;
        Ok(())
    }

    pub fn get_smsc_cache(&self, identity_keys: &[String]) -> Result<Option<SmscCacheEntry>> {
        let conn = self.conn.lock().unwrap();
        for key in identity_keys {
            let entry = conn
                .query_row(
                    "SELECT sms_center, source, updated_at
                     FROM smsc_cache
                     WHERE identity_key = ?1",
                    params![key],
                    |row| {
                        Ok(SmscCacheEntry {
                            sms_center: row.get(0)?,
                            source: row.get(1)?,
                            updated_at: row.get(2)?,
                        })
                    },
                )
                .optional()?;
            if entry.is_some() {
                return Ok(entry);
            }
        }
        Ok(None)
    }

    // ==================== Own number cache ====================

    pub fn upsert_own_number_cache(
        &self,
        identity_key: &str,
        iccid: &str,
        imsi: &str,
        operator_id: &str,
        phone_numbers: &[String],
        source: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let updated_at = Utc::now().to_rfc3339();
        let phone_numbers = phone_numbers.join("\n");
        conn.execute(
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
                identity_key,
                iccid,
                imsi,
                operator_id,
                phone_numbers,
                source,
                updated_at
            ],
        )?;
        Ok(())
    }

    pub fn get_own_number_cache(
        &self,
        identity_keys: &[String],
    ) -> Result<Option<OwnNumberCacheEntry>> {
        let conn = self.conn.lock().unwrap();
        for key in identity_keys {
            let entry = conn
                .query_row(
                    "SELECT phone_numbers, source, updated_at
                     FROM own_number_cache
                     WHERE identity_key = ?1",
                    params![key],
                    |row| {
                        let phone_numbers: String = row.get(0)?;
                        Ok(OwnNumberCacheEntry {
                            phone_numbers: phone_numbers
                                .lines()
                                .map(str::trim)
                                .filter(|line| !line.is_empty())
                                .map(ToString::to_string)
                                .collect(),
                            source: row.get(1)?,
                            updated_at: row.get(2)?,
                        })
                    },
                )
                .optional()?;
            if entry.is_some() {
                return Ok(entry);
            }
        }
        Ok(None)
    }

    // ==================== SMS storage cache ====================

    pub fn upsert_sms_storage_cache(
        &self,
        identity_key: &str,
        iccid: &str,
        imsi: &str,
        operator_id: &str,
        sms_used: Option<u32>,
        sms_total: Option<u32>,
        source: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let updated_at = Utc::now().to_rfc3339();
        conn.execute(
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
                identity_key,
                iccid,
                imsi,
                operator_id,
                sms_used,
                sms_total,
                source,
                updated_at
            ],
        )?;
        Ok(())
    }

    pub fn get_sms_storage_cache(
        &self,
        identity_keys: &[String],
    ) -> Result<Option<SmsStorageCacheEntry>> {
        let conn = self.conn.lock().unwrap();
        for key in identity_keys {
            let entry = conn
                .query_row(
                    "SELECT sms_used, sms_total, source, updated_at
                     FROM sms_storage_cache
                     WHERE identity_key = ?1",
                    params![key],
                    |row| {
                        Ok(SmsStorageCacheEntry {
                            sms_used: row.get(0)?,
                            sms_total: row.get(1)?,
                            source: row.get(2)?,
                            updated_at: row.get(3)?,
                        })
                    },
                )
                .optional()?;
            if entry.is_some() {
                return Ok(entry);
            }
        }
        Ok(None)
    }

    // ==================== eSIM Profile cache ====================

    pub fn upsert_esim_profile_cache(&self, entry: &EsimProfileCacheEntry) -> Result<()> {
        let iccid = crate::utils::normalize_iccid(&entry.iccid);
        if iccid.is_empty() {
            return Ok(());
        }

        let has_profile_data = [
            entry.name.as_deref(),
            entry.provider.as_deref(),
            entry.state.as_deref(),
            entry.profile_class.as_deref(),
            entry.imsi.as_deref(),
            entry.msisdn.as_deref(),
            entry.smsc.as_deref(),
            entry.smdp.as_deref(),
            entry.matching_id.as_deref(),
            entry.isdp_aid.as_deref(),
            entry.mcc.as_deref(),
            entry.mnc.as_deref(),
        ]
        .into_iter()
        .flatten()
        .any(|value| !value.trim().is_empty());

        if !has_profile_data {
            return Ok(());
        }

        let conn = self.conn.lock().unwrap();
        let updated_at = Utc::now().to_rfc3339();
        conn.execute(
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
                &iccid,
                non_empty_option(entry.name.as_deref()),
                non_empty_option(entry.provider.as_deref()),
                non_empty_option(entry.state.as_deref()),
                non_empty_option(entry.profile_class.as_deref()),
                non_empty_option(entry.imsi.as_deref()),
                non_empty_option(entry.msisdn.as_deref()),
                non_empty_option(entry.smsc.as_deref()),
                non_empty_option(entry.smdp.as_deref()),
                non_empty_option(entry.matching_id.as_deref()),
                non_empty_option(entry.isdp_aid.as_deref()),
                non_empty_option(entry.mcc.as_deref()),
                non_empty_option(entry.mnc.as_deref()),
                entry.disable_allowed,
                entry.delete_allowed,
                updated_at
            ],
        )?;
        Ok(())
    }

    pub fn get_esim_profile_cache(&self, iccid: &str) -> Result<Option<EsimProfileCacheEntry>> {
        let iccid = crate::utils::normalize_iccid(iccid);
        if iccid.is_empty() {
            return Ok(None);
        }

        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT iccid, name, provider, state, profile_class, imsi, msisdn, smsc, smdp,
                    matching_id, isdp_aid, mcc, mnc, disable_allowed, delete_allowed, updated_at
             FROM esim_profile_cache
             WHERE iccid = ?1",
            params![&iccid],
            |row| {
                Ok(EsimProfileCacheEntry {
                    iccid: row.get(0)?,
                    name: row.get(1)?,
                    provider: row.get(2)?,
                    state: row.get(3)?,
                    profile_class: row.get(4)?,
                    imsi: row.get(5)?,
                    msisdn: row.get(6)?,
                    smsc: row.get(7)?,
                    smdp: row.get(8)?,
                    matching_id: row.get(9)?,
                    isdp_aid: row.get(10)?,
                    mcc: row.get(11)?,
                    mnc: row.get(12)?,
                    disable_allowed: row.get(13)?,
                    delete_allowed: row.get(14)?,
                    updated_at: row.get(15)?,
                })
            },
        )
        .optional()
    }

    pub fn list_esim_profile_cache(&self) -> Result<Vec<EsimProfileCacheEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut statement = conn.prepare(
            "SELECT iccid, name, provider, state, profile_class, imsi, msisdn, smsc, smdp,
                    matching_id, isdp_aid, mcc, mnc, disable_allowed, delete_allowed, updated_at
             FROM esim_profile_cache
             ORDER BY iccid ASC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(EsimProfileCacheEntry {
                iccid: row.get(0)?,
                name: row.get(1)?,
                provider: row.get(2)?,
                state: row.get(3)?,
                profile_class: row.get(4)?,
                imsi: row.get(5)?,
                msisdn: row.get(6)?,
                smsc: row.get(7)?,
                smdp: row.get(8)?,
                matching_id: row.get(9)?,
                isdp_aid: row.get(10)?,
                mcc: row.get(11)?,
                mnc: row.get(12)?,
                disable_allowed: row.get(13)?,
                delete_allowed: row.get(14)?,
                updated_at: row.get(15)?,
            })
        })?;

        rows.collect()
    }

    pub fn delete_esim_profile_cache(&self, iccid: &str) -> Result<()> {
        let iccid = crate::utils::normalize_iccid(iccid);
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM esim_profile_cache WHERE iccid = ?1",
            params![&iccid],
        )?;
        Ok(())
    }

    // ==================== eUICC cache ====================

    pub fn upsert_esim_euicc_cache(&self, entry: &EsimEuiccCacheEntry) -> Result<()> {
        let cache_key = if entry.cache_key.trim().is_empty() {
            if entry.eid.trim().is_empty() {
                "default".to_string()
            } else {
                format!("eid:{}", entry.eid.trim())
            }
        } else {
            entry.cache_key.trim().to_string()
        };
        let conn = self.conn.lock().unwrap();
        let updated_at = Utc::now().to_rfc3339();
        conn.execute(
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
                entry.eid,
                entry.status,
                entry.manufacturer,
                entry.memory_total_kb,
                entry.memory_available_kb,
                entry.memory_total_customizable,
                entry.raw,
                updated_at
            ],
        )?;
        Ok(())
    }

    pub fn get_esim_euicc_cache(&self, cache_key: &str) -> Result<Option<EsimEuiccCacheEntry>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT cache_key, eid, status, manufacturer, memory_total_kb,
                    memory_available_kb, memory_total_customizable, raw, updated_at
             FROM esim_euicc_cache
             WHERE cache_key = ?1",
            params![cache_key],
            |row| {
                Ok(EsimEuiccCacheEntry {
                    cache_key: row.get(0)?,
                    eid: row.get(1)?,
                    status: row.get(2)?,
                    manufacturer: row.get(3)?,
                    memory_total_kb: row.get(4)?,
                    memory_available_kb: row.get(5)?,
                    memory_total_customizable: row.get(6)?,
                    raw: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            },
        )
        .optional()
    }

    pub fn latest_esim_euicc_cache(&self) -> Result<Option<EsimEuiccCacheEntry>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT cache_key, eid, status, manufacturer, memory_total_kb,
                    memory_available_kb, memory_total_customizable, raw, updated_at
             FROM esim_euicc_cache
             ORDER BY updated_at DESC
             LIMIT 1",
            [],
            |row| {
                Ok(EsimEuiccCacheEntry {
                    cache_key: row.get(0)?,
                    eid: row.get(1)?,
                    status: row.get(2)?,
                    manufacturer: row.get(3)?,
                    memory_total_kb: row.get(4)?,
                    memory_available_kb: row.get(5)?,
                    memory_total_customizable: row.get(6)?,
                    raw: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            },
        )
        .optional()
    }

    // ==================== 通话记录相关方法 ====================

    /// 插入新通话记录
    pub fn insert_call(&self, direction: &str, phone_number: &str, answered: bool) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let start_time = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO call_history (direction, phone_number, duration, start_time, answered)
             VALUES (?1, ?2, 0, ?3, ?4)",
            params![direction, phone_number, start_time, answered as i32],
        )?;

        Ok(conn.last_insert_rowid())
    }

    /// 更新通话记录（通话结束时调用）
    pub fn update_call_end(&self, id: i64, duration: i64, answered: bool) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let end_time = Utc::now().to_rfc3339();

        conn.execute(
            "UPDATE call_history SET duration = ?1, end_time = ?2, answered = ?3 WHERE id = ?4",
            params![duration, end_time, answered as i32, id],
        )?;
        Ok(())
    }

    /// 标记通话为未接来电
    pub fn mark_call_missed(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let end_time = Utc::now().to_rfc3339();

        conn.execute(
            "UPDATE call_history SET direction = 'missed', end_time = ?1, answered = 0 WHERE id = ?2",
            params![end_time, id],
        )?;
        Ok(())
    }

    /// 获取通话记录（分页）
    pub fn get_call_history(&self, limit: i64, offset: i64) -> Result<Vec<CallRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, direction, phone_number, duration, start_time, end_time, answered
             FROM call_history
             ORDER BY start_time DESC
             LIMIT ?1 OFFSET ?2",
        )?;

        let records = stmt.query_map(params![limit, offset], |row| {
            Ok(CallRecord {
                id: row.get(0)?,
                direction: row.get(1)?,
                phone_number: row.get(2)?,
                duration: row.get(3)?,
                start_time: row.get(4)?,
                end_time: row.get(5)?,
                answered: row.get::<_, i32>(6)? != 0,
            })
        })?;

        let mut result = Vec::new();
        for record in records {
            result.push(record?);
        }

        Ok(result)
    }

    /// 获取通话统计
    pub fn get_call_stats(&self) -> Result<CallStats> {
        let conn = self.conn.lock().unwrap();

        let total: i64 =
            conn.query_row("SELECT COUNT(*) FROM call_history", [], |row| row.get(0))?;

        let incoming: i64 = conn.query_row(
            "SELECT COUNT(*) FROM call_history WHERE direction = 'incoming'",
            [],
            |row| row.get(0),
        )?;

        let outgoing: i64 = conn.query_row(
            "SELECT COUNT(*) FROM call_history WHERE direction = 'outgoing'",
            [],
            |row| row.get(0),
        )?;

        let missed: i64 = conn.query_row(
            "SELECT COUNT(*) FROM call_history WHERE direction = 'missed'",
            [],
            |row| row.get(0),
        )?;

        let total_duration: i64 = conn.query_row(
            "SELECT COALESCE(SUM(duration), 0) FROM call_history WHERE answered = 1",
            [],
            |row| row.get(0),
        )?;

        Ok(CallStats {
            total,
            incoming,
            outgoing,
            missed,
            total_duration,
        })
    }

    /// 删除单条通话记录
    pub fn delete_call(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM call_history WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// 删除所有通话记录
    pub fn clear_all_calls(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM call_history", [])?;
        Ok(())
    }

    // ==================== 自动化运行日志相关方法 ====================

    /// 插入新自动化执行日志
    pub fn insert_automation_log(
        &self,
        task_id: &str,
        task_name: &str,
        task_type: &str,
        status: &str,
        detail: &str,
    ) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let created_at = beijing_sms_now_string();
        conn.execute(
            "INSERT INTO automation_logs (task_id, task_name, task_type, status, detail, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![task_id, task_name, task_type, status, detail, created_at],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// 获取自动化执行日志（分页与过滤）
    pub fn get_automation_logs(
        &self,
        task_type: &str,
        status: &str,
        query: &str,
        start_date: &str,
        end_date: &str,
        limit: i64,
        offset: i64,
    ) -> Result<AutomationLogsResponse> {
        let conn = self.conn.lock().unwrap();
        let limit = limit.clamp(1, 200);
        let offset = offset.max(0);
        let task_type = task_type.trim();
        let status = status.trim();
        let query = query.trim();

        let start_at = notification_log_start_bound(start_date);
        let end_at = notification_log_end_bound(end_date);

        let total = conn.query_row(
            "SELECT COUNT(*) FROM automation_logs
             WHERE (?1 = '' OR task_type = ?1)
               AND (?2 = '' OR status = ?2)
               AND (
                    ?3 = ''
                    OR task_name LIKE '%' || ?3 || '%'
                    OR detail LIKE '%' || ?3 || '%'
               )
               AND (?4 = '' OR created_at >= ?4)
               AND (?5 = '' OR created_at <= ?5)",
            params![task_type, status, query, start_at, end_at],
            |row| row.get(0),
        )?;

        let mut stmt = conn.prepare(
            "SELECT id, task_id, task_name, task_type, status, detail, created_at
             FROM automation_logs
             WHERE (?1 = '' OR task_type = ?1)
               AND (?2 = '' OR status = ?2)
               AND (
                    ?3 = ''
                    OR task_name LIKE '%' || ?3 || '%'
                    OR detail LIKE '%' || ?3 || '%'
               )
               AND (?4 = '' OR created_at >= ?4)
               AND (?5 = '' OR created_at <= ?5)
             ORDER BY created_at DESC
             LIMIT ?6 OFFSET ?7",
        )?;

        let rows = stmt.query_map(
            params![task_type, status, query, start_at, end_at, limit, offset],
            |row| {
                let mut detail: String = row.get(5)?;
                if detail == "执行成功 (0)" || detail.starts_with("执行成功 (0)") {
                    detail = "执行成功".to_string();
                }
                Ok(AutomationLogEntry {
                    id: row.get(0)?,
                    task_id: row.get(1)?,
                    task_name: row.get(2)?,
                    task_type: row.get(3)?,
                    status: row.get(4)?,
                    detail,
                    created_at: row.get(6)?,
                })
            },
        )?;

        let mut logs = Vec::new();
        for row in rows {
            logs.push(row?);
        }

        Ok(AutomationLogsResponse { logs, total })
    }

    /// 获取特定任务的最后一次运行日志
    pub fn get_last_log_for_task(&self, task_id: &str) -> Result<Option<AutomationLogEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, task_id, task_name, task_type, status, detail, created_at
             FROM automation_logs
             WHERE task_id = ?1
             ORDER BY created_at DESC
             LIMIT 1",
        )?;
        let mut rows = stmt.query_map(params![task_id], |row| {
            let mut detail: String = row.get(5)?;
            if detail == "执行成功 (0)" || detail.starts_with("执行成功 (0)") {
                detail = "执行成功".to_string();
            }
            Ok(AutomationLogEntry {
                id: row.get(0)?,
                task_id: row.get(1)?,
                task_name: row.get(2)?,
                task_type: row.get(3)?,
                status: row.get(4)?,
                detail,
                created_at: row.get(6)?,
            })
        })?;
        if let Some(row) = rows.next() {
            Ok(Some(row?))
        } else {
            Ok(None)
        }
    }

    /// 清理过滤的日志
    pub fn clear_automation_logs(
        &self,
        task_type: &str,
        status: &str,
        start_date: &str,
        end_date: &str,
    ) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let task_type = task_type.trim();
        let status = status.trim();

        let start_at = notification_log_start_bound(start_date);
        let end_at = notification_log_end_bound(end_date);

        conn.execute(
            "DELETE FROM automation_logs
             WHERE (?1 = '' OR task_type = ?1)
               AND (?2 = '' OR status = ?2)
               AND (?3 = '' OR created_at >= ?3)
               AND (?4 = '' OR created_at <= ?4)",
            params![task_type, status, start_at, end_at],
        )
    }

    /// 自动保留策略清理
    pub fn cleanup_automation_logs(
        &self,
        retention_days: Option<u32>,
        max_entries: Option<u32>,
    ) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let mut deleted = 0usize;

        if let Some(days) = retention_days.filter(|days| *days > 0) {
            let cutoff = Utc::now()
                .with_timezone(&beijing_offset())
                .checked_sub_signed(Duration::days(i64::from(days)))
                .unwrap_or_else(|| Utc::now().with_timezone(&beijing_offset()))
                .format(SMS_TIMESTAMP_FORMAT)
                .to_string();
            deleted += conn.execute(
                "DELETE FROM automation_logs WHERE created_at < ?1",
                params![cutoff],
            )?;
        }

        if let Some(max_entries) = max_entries.filter(|max_entries| *max_entries > 0) {
            deleted += conn.execute(
                "DELETE FROM automation_logs
                 WHERE id NOT IN (
                    SELECT id FROM automation_logs
                    ORDER BY id DESC
                    LIMIT ?1
                 )",
                params![i64::from(max_entries)],
            )?;
        }

        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Database {
        Database::new(PathBuf::from(":memory:")).unwrap()
    }

    #[test]
    fn own_number_cache_allows_empty_result() {
        let db = test_db();

        db.upsert_own_number_cache(
            "iccid:TEST_ICCID_001",
            "TEST_ICCID_001",
            "001010",
            "00101",
            &[],
            "empty",
        )
        .unwrap();

        let entry = db
            .get_own_number_cache(&["iccid:TEST_ICCID_001".to_string()])
            .unwrap()
            .unwrap();
        assert!(entry.phone_numbers.is_empty());
        assert_eq!(entry.source, "empty");
        assert!(!entry.updated_at.is_empty());
    }

    #[test]
    fn sms_storage_cache_allows_empty_result() {
        let db = test_db();

        db.upsert_sms_storage_cache(
            "iccid:TEST_ICCID_001",
            "TEST_ICCID_001",
            "001010",
            "00101",
            None,
            None,
            "empty",
        )
        .unwrap();

        let entry = db
            .get_sms_storage_cache(&["iccid:TEST_ICCID_001".to_string()])
            .unwrap()
            .unwrap();
        assert_eq!(entry.sms_used, None);
        assert_eq!(entry.sms_total, None);
        assert_eq!(entry.source, "empty");
        assert!(!entry.updated_at.is_empty());
    }

    #[test]
    fn esim_profile_cache_persists_state_permissions_and_updated_at() {
        let db = test_db();
        db.upsert_esim_profile_cache(&EsimProfileCacheEntry {
            iccid: "8901000000000000001".to_string(),
            name: Some("Profile A".to_string()),
            provider: Some("Provider".to_string()),
            state: Some("enabled".to_string()),
            disable_allowed: Some(false),
            delete_allowed: Some(true),
            ..Default::default()
        })
        .unwrap();

        let entry = db
            .get_esim_profile_cache("8901000000000000001")
            .unwrap()
            .unwrap();
        assert_eq!(entry.state.as_deref(), Some("enabled"));
        assert_eq!(entry.disable_allowed, Some(false));
        assert_eq!(entry.delete_allowed, Some(true));
        assert!(!entry.updated_at.is_empty());

        let listed = db.list_esim_profile_cache().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].state.as_deref(), Some("enabled"));
        assert_eq!(listed[0].disable_allowed, Some(false));
        assert_eq!(listed[0].delete_allowed, Some(true));
    }

    #[test]
    fn esim_euicc_cache_persists_latest_snapshot() {
        let db = test_db();
        db.upsert_esim_euicc_cache(&EsimEuiccCacheEntry {
            cache_key: "eid:EID001".to_string(),
            eid: "EID001".to_string(),
            status: "ready".to_string(),
            manufacturer: "Test".to_string(),
            memory_total_kb: Some(1024.0),
            memory_available_kb: Some(512.0),
            memory_total_customizable: Some(true),
            raw: "{}".to_string(),
            updated_at: String::new(),
        })
        .unwrap();

        let by_key = db.get_esim_euicc_cache("eid:EID001").unwrap().unwrap();
        assert_eq!(by_key.eid, "EID001");
        assert_eq!(by_key.memory_total_kb, Some(1024.0));
        assert_eq!(by_key.memory_available_kb, Some(512.0));
        assert_eq!(by_key.memory_total_customizable, Some(true));
        assert!(!by_key.updated_at.is_empty());

        let latest = db.latest_esim_euicc_cache().unwrap().unwrap();
        assert_eq!(latest.cache_key, "eid:EID001");
    }
}
