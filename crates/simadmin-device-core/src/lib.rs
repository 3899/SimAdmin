use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Mutex,
};

use chrono::{Duration, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use simadmin_protocol::{
    AccessMethod, BindingControlMode, CapabilityManifest, DeviceFeatureSnapshot, DeviceKind,
};

#[derive(Debug, thiserror::Error)]
pub enum DeviceError {
    #[error("invalid hardware identity: {0}")]
    InvalidIdentity(String),
    #[error("hardware identity conflict: {0}")]
    IdentityConflict(String),
    #[error("device binding not found")]
    BindingNotFound,
    #[error("device worker is stale")]
    StaleWorker,
    #[error("hardware ownership lease is unavailable")]
    LeaseUnavailable,
    #[error("device capability is unavailable: {0}")]
    Unsupported(String),
    #[error("backend operation failed: {0}")]
    Backend(String),
    #[error(transparent)]
    Database(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type DeviceResult<T> = Result<T, DeviceError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct HardwareFingerprint {
    pub imei: Option<String>,
    pub usb_serial: Option<String>,
    pub vendor_id: Option<String>,
    pub product_id: Option<String>,
}

impl HardwareFingerprint {
    pub fn normalized(mut self) -> DeviceResult<Self> {
        self.imei = normalize_optional(self.imei);
        self.usb_serial = normalize_optional(self.usb_serial);
        self.vendor_id = normalize_hex(self.vendor_id);
        self.product_id = normalize_hex(self.product_id);
        if let Some(imei) = &self.imei {
            if !(14..=16).contains(&imei.len())
                || !imei.bytes().all(|byte| byte.is_ascii_digit())
                || imei.bytes().all(|byte| byte == imei.as_bytes()[0])
            {
                return Err(DeviceError::InvalidIdentity(
                    "IMEI must be 14-16 non-placeholder digits".into(),
                ));
            }
        }
        if self.imei.is_none() && self.usb_serial.is_none() {
            return Err(DeviceError::InvalidIdentity(
                "IMEI or USB serial is required for a confirmed binding".into(),
            ));
        }
        Ok(self)
    }

    pub fn stable_key(&self) -> DeviceResult<String> {
        let value = self.clone().normalized()?;
        Ok(hex_sha256(&serde_json::to_vec(&value)?))
    }

    fn normalized_for_slot(mut self) -> Self {
        self.imei = normalize_optional(self.imei);
        self.usb_serial = normalize_optional(self.usb_serial);
        self.vendor_id = normalize_hex(self.vendor_id);
        self.product_id = normalize_hex(self.product_id);
        self
    }

    fn has_stable_identity(&self) -> bool {
        self.imei.as_ref().is_some_and(|value| !value.is_empty())
            || self
                .usb_serial
                .as_ref()
                .is_some_and(|value| !value.is_empty())
    }

    fn slot_hardware_compatible(&self, other: &Self) -> bool {
        if self.has_stable_identity() && other.has_stable_identity() {
            return self.matches(other);
        }
        let vendor_matches = self
            .vendor_id
            .as_ref()
            .zip(other.vendor_id.as_ref())
            .is_none_or(|(left, right)| left.eq_ignore_ascii_case(right));
        let product_matches = self
            .product_id
            .as_ref()
            .zip(other.product_id.as_ref())
            .is_none_or(|(left, right)| left.eq_ignore_ascii_case(right));
        vendor_matches && product_matches
    }

    pub fn matches(&self, other: &Self) -> bool {
        let imei = self
            .imei
            .as_ref()
            .zip(other.imei.as_ref())
            .map(|(a, b)| a == b);
        let serial = self
            .usb_serial
            .as_ref()
            .zip(other.usb_serial.as_ref())
            .map(|(a, b)| a.eq_ignore_ascii_case(b));
        match (imei, serial) {
            (Some(false), _) | (_, Some(false)) => false,
            (Some(true), _) | (_, Some(true)) => true,
            _ => false,
        }
    }
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}
fn normalize_hex(value: Option<String>) -> Option<String> {
    normalize_optional(value).map(|value| value.trim_start_matches("0x").to_ascii_lowercase())
}
fn hex_sha256(value: &[u8]) -> String {
    Sha256::digest(value)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    ModemManager,
    DirectAt,
    Qmi,
    Mbim,
    NetworkOnly,
    Mock,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscoveredDevice {
    pub discovery_id: String,
    pub fingerprint: HardwareFingerprint,
    pub usb_path: String,
    pub control_paths: Vec<String>,
    pub network_interfaces: Vec<String>,
    pub simadmin_urls: Vec<String>,
    pub backend: BackendKind,
    pub capabilities: Vec<String>,
    pub device_kind: DeviceKind,
    pub access_method: AccessMethod,
    pub capability_manifest: CapabilityManifest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BindingPolicy {
    HardwareBound,
    SlotBound,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceBinding {
    pub device_id: String,
    pub fingerprint: HardwareFingerprint,
    pub policy: BindingPolicy,
    pub control_mode: BindingControlMode,
    pub slot_id: Option<String>,
    pub current_usb_path: String,
    pub control_paths: Vec<String>,
    pub backend: BackendKind,
    pub capabilities: Vec<String>,
    pub device_kind: DeviceKind,
    pub access_method: AccessMethod,
    pub capability_manifest: CapabilityManifest,
    pub binding_version: i64,
    pub worker_generation: i64,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconcileOutcome {
    Ready {
        device_id: String,
        generation: i64,
        path_changed: bool,
    },
    Pending {
        discovery_id: String,
    },
    Conflict {
        discovery_id: String,
        reason: String,
    },
    Offline {
        device_id: String,
    },
}

pub struct BindingStore {
    connection: Mutex<Connection>,
}

impl BindingStore {
    pub fn open(path: impl AsRef<Path>) -> DeviceResult<Self> {
        if let Some(parent) = path
            .as_ref()
            .parent()
            .filter(|value| !value.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path)?;
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
            CREATE TABLE IF NOT EXISTS device_bindings (
                device_id TEXT PRIMARY KEY, fingerprint_json TEXT NOT NULL, fingerprint_key TEXT NOT NULL,
                policy TEXT NOT NULL, slot_id TEXT, current_usb_path TEXT NOT NULL, control_paths_json TEXT NOT NULL,
                backend_json TEXT NOT NULL, capabilities_json TEXT NOT NULL, binding_version INTEGER NOT NULL,
                worker_generation INTEGER NOT NULL DEFAULT 0, status TEXT NOT NULL, updated_at TEXT NOT NULL,
                control_mode TEXT NOT NULL DEFAULT 'control',
                device_kind TEXT NOT NULL DEFAULT 'unknown', access_method TEXT NOT NULL DEFAULT 'host_direct',
                capability_manifest_json TEXT NOT NULL DEFAULT '{}');
            CREATE TABLE IF NOT EXISTS ownership_leases (
                fingerprint_key TEXT PRIMARY KEY, owner TEXT NOT NULL, worker_generation INTEGER NOT NULL,
                expires_at TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS connection_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT, device_id TEXT NOT NULL, usb_path TEXT NOT NULL,
                worker_generation INTEGER NOT NULL, event TEXT NOT NULL, created_at TEXT NOT NULL);" )?;
        ensure_column(
            &connection,
            "device_bindings",
            "device_kind",
            "TEXT NOT NULL DEFAULT 'unknown'",
        )?;
        ensure_column(
            &connection,
            "device_bindings",
            "capability_manifest_json",
            "TEXT NOT NULL DEFAULT '{}'",
        )?;
        ensure_column(
            &connection,
            "device_bindings",
            "access_method",
            "TEXT NOT NULL DEFAULT 'host_direct'",
        )?;
        ensure_column(
            &connection,
            "device_bindings",
            "control_mode",
            "TEXT NOT NULL DEFAULT 'control'",
        )?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn replace_confirmed_bindings(&self, bindings: &[DeviceBinding]) -> DeviceResult<()> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| DeviceError::Backend("binding lock poisoned".into()))?;
        let transaction = connection.transaction()?;
        let incoming: HashSet<&str> = bindings
            .iter()
            .map(|binding| binding.device_id.as_str())
            .collect();
        for binding in bindings {
            let fingerprint = match binding.policy {
                BindingPolicy::HardwareBound => binding.fingerprint.clone().normalized()?,
                BindingPolicy::SlotBound => binding.fingerprint.clone().normalized_for_slot(),
            };
            let key = match binding.policy {
                BindingPolicy::HardwareBound => fingerprint.stable_key()?,
                BindingPolicy::SlotBound => {
                    let slot_id = binding
                        .slot_id
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .ok_or_else(|| {
                            DeviceError::InvalidIdentity(
                                "slot-bound binding requires a stable slot_id".into(),
                            )
                        })?;
                    hex_sha256(format!("slot:{slot_id}").as_bytes())
                }
            };
            transaction.execute("INSERT INTO device_bindings (device_id, fingerprint_json, fingerprint_key, policy,
                slot_id, current_usb_path, control_paths_json, backend_json, capabilities_json, binding_version,
                worker_generation, status, updated_at, device_kind, access_method,
                capability_manifest_json, control_mode)
                VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)
                ON CONFLICT(device_id) DO UPDATE SET fingerprint_json=excluded.fingerprint_json,
                fingerprint_key=excluded.fingerprint_key, policy=excluded.policy, slot_id=excluded.slot_id,
                backend_json=excluded.backend_json, capabilities_json=excluded.capabilities_json,
                binding_version=excluded.binding_version, device_kind=excluded.device_kind,
                access_method=excluded.access_method,
                capability_manifest_json=excluded.capability_manifest_json,
                control_mode=excluded.control_mode, updated_at=excluded.updated_at",
                params![binding.device_id, serde_json::to_string(&fingerprint)?, key,
                    serde_json::to_string(&binding.policy)?, binding.slot_id, binding.current_usb_path,
                    serde_json::to_string(&binding.control_paths)?, serde_json::to_string(&binding.backend)?,
                    serde_json::to_string(&binding.capabilities)?, binding.binding_version,
                    binding.worker_generation, binding.status, Utc::now().to_rfc3339(),
                    binding.device_kind.as_str(), binding.access_method.as_str(),
                    serde_json::to_string(&binding.capability_manifest)?,
                    control_mode_label(binding.control_mode)])?;
        }
        let existing = {
            let mut statement = transaction.prepare("SELECT device_id FROM device_bindings")?;
            let values = statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            values
        };
        for id in existing {
            if !incoming.contains(id.as_str()) {
                transaction.execute(
                    "UPDATE device_bindings SET status='revoked' WHERE device_id=?1",
                    [id],
                )?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn bindings(&self) -> DeviceResult<Vec<DeviceBinding>> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| DeviceError::Backend("binding lock poisoned".into()))?;
        let mut statement = connection.prepare("SELECT device_id, fingerprint_json, policy, slot_id, current_usb_path,
            control_paths_json, backend_json, capabilities_json, binding_version, worker_generation, status,
            device_kind, access_method, capability_manifest_json, control_mode
            FROM device_bindings WHERE status != 'revoked' ORDER BY device_id")?;
        let rows = statement.query_map([], |row| {
            Ok(DeviceBinding {
                device_id: row.get(0)?,
                fingerprint: parse(row, 1)?,
                policy: parse(row, 2)?,
                slot_id: row.get(3)?,
                current_usb_path: row.get(4)?,
                control_paths: parse(row, 5)?,
                backend: parse(row, 6)?,
                capabilities: parse(row, 7)?,
                binding_version: row.get(8)?,
                worker_generation: row.get(9)?,
                status: row.get(10)?,
                device_kind: parse_device_kind(row.get::<_, String>(11)?),
                access_method: parse_access_method(row.get::<_, String>(12)?),
                capability_manifest: parse_manifest(row, 13, 7)?,
                control_mode: parse_control_mode(row.get::<_, String>(14)?),
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn reconcile(
        &self,
        discovered: &[DiscoveredDevice],
    ) -> DeviceResult<Vec<ReconcileOutcome>> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| DeviceError::Backend("binding lock poisoned".into()))?;
        let transaction = connection.transaction()?;
        let mut outcomes = Vec::new();
        let mut matched = HashSet::new();
        for candidate in discovered {
            let all = load_bindings(&transaction)?;
            let slot_candidates = all
                .iter()
                .filter(|binding| {
                    binding.status != "revoked"
                        && binding.policy == BindingPolicy::SlotBound
                        && binding.slot_id.as_deref() == Some(candidate.usb_path.as_str())
                })
                .cloned()
                .collect::<Vec<_>>();
            if slot_candidates.len() > 1 {
                outcomes.push(ReconcileOutcome::Conflict {
                    discovery_id: candidate.discovery_id.clone(),
                    reason: "physical slot is assigned to multiple confirmed devices".into(),
                });
                continue;
            }
            let normalized = if slot_candidates.is_empty() {
                match candidate.fingerprint.clone().normalized() {
                    Ok(value) => value,
                    Err(error) => {
                        outcomes.push(ReconcileOutcome::Conflict {
                            discovery_id: candidate.discovery_id.clone(),
                            reason: error.to_string(),
                        });
                        continue;
                    }
                }
            } else {
                candidate.fingerprint.clone().normalized_for_slot()
            };
            let candidates: Vec<_> = if let Some(binding) = slot_candidates.into_iter().next() {
                if !binding.fingerprint.slot_hardware_compatible(&normalized) {
                    transaction.execute(
                        "UPDATE device_bindings SET status='identity_conflict', worker_generation=worker_generation+1, updated_at=?2 WHERE device_id=?1",
                        params![binding.device_id, Utc::now().to_rfc3339()],
                    )?;
                    outcomes.push(ReconcileOutcome::Conflict {
                        discovery_id: candidate.discovery_id.clone(),
                        reason: format!(
                            "hardware in slot {} differs from the confirmed module",
                            candidate.usb_path
                        ),
                    });
                    matched.insert(binding.device_id);
                    continue;
                }
                vec![binding]
            } else {
                all.into_iter()
                    .filter(|binding| {
                        binding.status != "revoked"
                            && binding.policy == BindingPolicy::HardwareBound
                            && binding.fingerprint.matches(&normalized)
                    })
                    .collect()
            };
            if candidates.len() > 1 {
                outcomes.push(ReconcileOutcome::Conflict {
                    discovery_id: candidate.discovery_id.clone(),
                    reason: "identity matches multiple confirmed devices".into(),
                });
                continue;
            }
            let Some(mut binding) = candidates.into_iter().next() else {
                outcomes.push(ReconcileOutcome::Pending {
                    discovery_id: candidate.discovery_id.clone(),
                });
                continue;
            };
            if !matched.insert(binding.device_id.clone()) {
                outcomes.push(ReconcileOutcome::Conflict {
                    discovery_id: candidate.discovery_id.clone(),
                    reason: "same confirmed identity appeared more than once".into(),
                });
                continue;
            }
            let changed = binding.current_usb_path != candidate.usb_path
                || binding.control_paths != candidate.control_paths;
            if changed {
                binding.worker_generation += 1;
            }
            transaction.execute("UPDATE device_bindings SET current_usb_path=?2, control_paths_json=?3,
                backend_json=?4, capabilities_json=?5, worker_generation=?6, status='ready', updated_at=?7,
                device_kind=?8, access_method=?9, capability_manifest_json=?10 WHERE device_id=?1",
                params![binding.device_id, candidate.usb_path, serde_json::to_string(&candidate.control_paths)?,
                    serde_json::to_string(&candidate.backend)?, serde_json::to_string(&candidate.capabilities)?,
                    binding.worker_generation, Utc::now().to_rfc3339(), candidate.device_kind.as_str(),
                    candidate.access_method.as_str(), serde_json::to_string(&candidate.capability_manifest)?])?;
            if changed {
                transaction.execute("INSERT INTO connection_history (device_id, usb_path, worker_generation, event, created_at)
                VALUES (?1,?2,?3,'path_changed',?4)", params![binding.device_id, candidate.usb_path, binding.worker_generation, Utc::now().to_rfc3339()])?;
            }
            outcomes.push(ReconcileOutcome::Ready {
                device_id: binding.device_id,
                generation: binding.worker_generation,
                path_changed: changed,
            });
        }
        for binding in load_bindings(&transaction)? {
            if binding.status != "revoked" && !matched.contains(&binding.device_id) {
                if binding.status != "offline" && binding.status != "identity_conflict" {
                    transaction.execute("UPDATE device_bindings SET status='offline', worker_generation=worker_generation+1, updated_at=?2 WHERE device_id=?1", params![binding.device_id, Utc::now().to_rfc3339()])?;
                }
                outcomes.push(ReconcileOutcome::Offline {
                    device_id: binding.device_id,
                });
            }
        }
        transaction.commit()?;
        Ok(outcomes)
    }

    pub fn acquire_lease(
        &self,
        device_id: &str,
        generation: i64,
        owner: &str,
        ttl_seconds: i64,
    ) -> DeviceResult<ExecutionLease> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| DeviceError::Backend("binding lock poisoned".into()))?;
        let transaction = connection.transaction()?;
        let (key, current_generation, status, control_mode): (String, i64, String, String) = transaction.query_row("SELECT fingerprint_key, worker_generation, status, control_mode FROM device_bindings WHERE device_id=?1", [device_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))).optional()?.ok_or(DeviceError::BindingNotFound)?;
        if status != "ready" || current_generation != generation || control_mode != "control" {
            return Err(DeviceError::StaleWorker);
        }
        let now = Utc::now();
        let held: Option<(String, String)> = transaction
            .query_row(
                "SELECT owner, expires_at FROM ownership_leases WHERE fingerprint_key=?1",
                [&key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if held
            .is_some_and(|(held_owner, expires)| held_owner != owner && expires > now.to_rfc3339())
        {
            return Err(DeviceError::LeaseUnavailable);
        }
        let expires_at = now + Duration::seconds(ttl_seconds.clamp(5, 300));
        transaction.execute("INSERT INTO ownership_leases (fingerprint_key, owner, worker_generation, expires_at) VALUES (?1,?2,?3,?4)
            ON CONFLICT(fingerprint_key) DO UPDATE SET owner=excluded.owner, worker_generation=excluded.worker_generation, expires_at=excluded.expires_at",
            params![key, owner, generation, expires_at.to_rfc3339()])?;
        transaction.commit()?;
        Ok(ExecutionLease {
            device_id: device_id.into(),
            fingerprint_key: key,
            owner: owner.into(),
            worker_generation: generation,
            expires_at: expires_at.to_rfc3339(),
        })
    }

    pub fn validate_lease(&self, lease: &ExecutionLease) -> DeviceResult<DeviceBinding> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| DeviceError::Backend("binding lock poisoned".into()))?;
        let valid: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM ownership_leases l JOIN device_bindings b ON b.fingerprint_key=l.fingerprint_key
            WHERE b.device_id=?1 AND l.fingerprint_key=?2 AND l.owner=?3 AND l.worker_generation=?4
            AND b.worker_generation=?4 AND b.status='ready' AND b.control_mode='control'
            AND l.expires_at>?5)", params![lease.device_id, lease.fingerprint_key, lease.owner, lease.worker_generation, Utc::now().to_rfc3339()], |row| row.get(0))?;
        if !valid {
            return Err(DeviceError::StaleWorker);
        }
        load_binding(&connection, &lease.device_id)?.ok_or(DeviceError::BindingNotFound)
    }
}

#[derive(Debug, Clone)]
pub struct ExecutionLease {
    pub device_id: String,
    pub fingerprint_key: String,
    pub owner: String,
    pub worker_generation: i64,
    pub expires_at: String,
}

fn parse<T: for<'de> Deserialize<'de>>(
    row: &rusqlite::Row<'_>,
    index: usize,
) -> rusqlite::Result<T> {
    let value: String = row.get(index)?;
    serde_json::from_str(&value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}
fn load_bindings(connection: &Connection) -> DeviceResult<Vec<DeviceBinding>> {
    let mut statement = connection.prepare("SELECT device_id, fingerprint_json, policy, slot_id, current_usb_path, control_paths_json, backend_json, capabilities_json, binding_version, worker_generation, status, device_kind, access_method, capability_manifest_json, control_mode FROM device_bindings")?;
    let bindings = statement
        .query_map([], |row| {
            Ok(DeviceBinding {
                device_id: row.get(0)?,
                fingerprint: parse(row, 1)?,
                policy: parse(row, 2)?,
                slot_id: row.get(3)?,
                current_usb_path: row.get(4)?,
                control_paths: parse(row, 5)?,
                backend: parse(row, 6)?,
                capabilities: parse(row, 7)?,
                binding_version: row.get(8)?,
                worker_generation: row.get(9)?,
                status: row.get(10)?,
                device_kind: parse_device_kind(row.get::<_, String>(11)?),
                access_method: parse_access_method(row.get::<_, String>(12)?),
                capability_manifest: parse_manifest(row, 13, 7)?,
                control_mode: parse_control_mode(row.get::<_, String>(14)?),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(bindings)
}

fn parse_device_kind(value: String) -> DeviceKind {
    match value.as_str() {
        "system_device" => DeviceKind::SystemDevice,
        "modem" => DeviceKind::Modem,
        _ => DeviceKind::Unknown,
    }
}

fn parse_access_method(value: String) -> AccessMethod {
    match value.as_str() {
        "network" => AccessMethod::Network,
        "local_system" => AccessMethod::LocalSystem,
        _ => AccessMethod::HostDirect,
    }
}

fn parse_control_mode(value: String) -> BindingControlMode {
    if value == "observed_only" {
        BindingControlMode::ObservedOnly
    } else {
        BindingControlMode::Control
    }
}

fn control_mode_label(value: BindingControlMode) -> &'static str {
    match value {
        BindingControlMode::Control => "control",
        BindingControlMode::ObservedOnly => "observed_only",
    }
}

fn parse_manifest(
    row: &rusqlite::Row<'_>,
    manifest_index: usize,
    legacy_index: usize,
) -> rusqlite::Result<CapabilityManifest> {
    let value: String = row.get(manifest_index)?;
    if let Ok(manifest) = serde_json::from_str::<CapabilityManifest>(&value) {
        if manifest.schema_version > 0 {
            return Ok(manifest.normalized());
        }
    }
    let legacy: Vec<String> = parse(row, legacy_index)?;
    Ok(CapabilityManifest::from_legacy(&legacy))
}

fn ensure_column(
    connection: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> DeviceResult<()> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    if !columns.iter().any(|value| value == column) {
        connection.execute(
            &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
            [],
        )?;
    }
    Ok(())
}
fn load_binding(connection: &Connection, device_id: &str) -> DeviceResult<Option<DeviceBinding>> {
    Ok(load_bindings(connection)?
        .into_iter()
        .find(|value| value.device_id == device_id))
}

pub trait DeviceBackend: Send + Sync {
    fn discover(&self) -> DeviceResult<Vec<DiscoveredDevice>>;
    fn discover_bound(&self, bindings: &[DeviceBinding]) -> DeviceResult<Vec<DiscoveredDevice>> {
        let discovered = self.discover()?;
        Ok(discovered
            .into_iter()
            .filter(|candidate| {
                bindings.iter().any(|binding| {
                    binding.fingerprint.matches(&candidate.fingerprint)
                        || binding.slot_id.as_deref() == Some(candidate.usb_path.as_str())
                })
            })
            .collect())
    }
    fn execute(
        &self,
        binding: &DeviceBinding,
        command_type: &str,
        payload: &serde_json::Value,
    ) -> DeviceResult<serde_json::Value>;
    fn feature_snapshot(&self, _binding: &DeviceBinding) -> DeviceResult<DeviceFeatureSnapshot> {
        Ok(DeviceFeatureSnapshot::default())
    }
}

#[derive(Default)]
pub struct MockBackend {
    discoveries: Mutex<Vec<DiscoveredDevice>>,
    results: Mutex<HashMap<String, serde_json::Value>>,
}
impl MockBackend {
    pub fn set_discoveries(&self, values: Vec<DiscoveredDevice>) {
        *self.discoveries.lock().unwrap() = values;
    }
    pub fn set_result(&self, command: &str, value: serde_json::Value) {
        self.results.lock().unwrap().insert(command.into(), value);
    }
}
impl DeviceBackend for MockBackend {
    fn discover(&self) -> DeviceResult<Vec<DiscoveredDevice>> {
        Ok(self.discoveries.lock().unwrap().clone())
    }
    fn execute(
        &self,
        _binding: &DeviceBinding,
        command_type: &str,
        _payload: &serde_json::Value,
    ) -> DeviceResult<serde_json::Value> {
        self.results
            .lock()
            .unwrap()
            .get(command_type)
            .cloned()
            .ok_or_else(|| DeviceError::Unsupported(command_type.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fingerprint(imei: &str, serial: &str) -> HardwareFingerprint {
        HardwareFingerprint {
            imei: Some(imei.into()),
            usb_serial: Some(serial.into()),
            vendor_id: Some("2c7c".into()),
            product_id: Some("0125".into()),
        }
    }
    fn binding(id: &str, imei: &str, serial: &str, path: &str) -> DeviceBinding {
        let capabilities = vec!["sms".into(), "network".into()];
        DeviceBinding {
            device_id: id.into(),
            fingerprint: fingerprint(imei, serial),
            policy: BindingPolicy::HardwareBound,
            control_mode: BindingControlMode::Control,
            slot_id: None,
            current_usb_path: path.into(),
            control_paths: vec![format!("{path}/ttyUSB2")],
            backend: BackendKind::DirectAt,
            capability_manifest: CapabilityManifest::from_legacy(&capabilities),
            capabilities,
            device_kind: DeviceKind::Modem,
            access_method: AccessMethod::HostDirect,
            binding_version: 1,
            worker_generation: 1,
            status: "ready".into(),
        }
    }
    fn discovered(id: &str, imei: &str, serial: &str, path: &str) -> DiscoveredDevice {
        let capabilities = vec!["sms".into(), "network".into()];
        DiscoveredDevice {
            discovery_id: id.into(),
            fingerprint: fingerprint(imei, serial),
            usb_path: path.into(),
            control_paths: vec![format!("{path}/ttyUSB2")],
            network_interfaces: vec![],
            simadmin_urls: vec![],
            backend: BackendKind::DirectAt,
            capability_manifest: CapabilityManifest::from_legacy(&capabilities),
            capabilities,
            device_kind: DeviceKind::Modem,
            access_method: AccessMethod::HostDirect,
        }
    }

    #[test]
    fn devices_follow_hardware_when_usb_ports_are_swapped() {
        let directory = tempfile::tempdir().unwrap();
        let store = BindingStore::open(directory.path().join("bindings.db")).unwrap();
        store
            .replace_confirmed_bindings(&[
                binding("a", "860000000000001", "A", "1-1"),
                binding("b", "860000000000002", "B", "1-2"),
            ])
            .unwrap();
        let result = store
            .reconcile(&[
                discovered("x", "860000000000001", "A", "1-2"),
                discovered("y", "860000000000002", "B", "1-1"),
            ])
            .unwrap();
        assert!(result.contains(&ReconcileOutcome::Ready {
            device_id: "a".into(),
            generation: 2,
            path_changed: true
        }));
        assert!(result.contains(&ReconcileOutcome::Ready {
            device_id: "b".into(),
            generation: 2,
            path_changed: true
        }));
        let lease = store.acquire_lease("a", 2, "host-1", 30).unwrap();
        assert_eq!(
            store.validate_lease(&lease).unwrap().current_usb_path,
            "1-2"
        );
        assert!(matches!(
            store.acquire_lease("a", 1, "host-1", 30),
            Err(DeviceError::StaleWorker)
        ));
    }

    #[test]
    fn observed_only_binding_never_grants_an_execution_lease() {
        let directory = tempfile::tempdir().unwrap();
        let store = BindingStore::open(directory.path().join("bindings.db")).unwrap();
        let mut observed = binding("a", "860000000000001", "A", "1-1");
        observed.control_mode = BindingControlMode::ObservedOnly;
        store.replace_confirmed_bindings(&[observed]).unwrap();
        let result = store
            .reconcile(&[discovered("x", "860000000000001", "A", "1-1")])
            .unwrap();
        let generation = match &result[0] {
            ReconcileOutcome::Ready { generation, .. } => *generation,
            outcome => panic!("unexpected reconcile outcome: {outcome:?}"),
        };

        assert!(matches!(
            store.acquire_lease("a", generation, "host-1", 30),
            Err(DeviceError::StaleWorker)
        ));
    }

    #[test]
    fn duplicate_identity_is_conflict_and_never_auto_bound() {
        let directory = tempfile::tempdir().unwrap();
        let store = BindingStore::open(directory.path().join("bindings.db")).unwrap();
        store
            .replace_confirmed_bindings(&[binding("a", "860000000000001", "A", "1-1")])
            .unwrap();
        let result = store
            .reconcile(&[
                discovered("x", "860000000000001", "A", "1-1"),
                discovered("y", "860000000000001", "A", "1-2"),
            ])
            .unwrap();
        assert!(matches!(result[1], ReconcileOutcome::Conflict { .. }));
    }

    #[test]
    fn slot_bound_device_rejects_replaced_hardware() {
        let directory = tempfile::tempdir().unwrap();
        let store = BindingStore::open(directory.path().join("bindings.db")).unwrap();
        let mut value = binding("slot-a", "860000000000001", "A", "1-1");
        value.policy = BindingPolicy::SlotBound;
        value.slot_id = Some("1-1".into());
        store.replace_confirmed_bindings(&[value]).unwrap();

        let result = store
            .reconcile(&[discovered("replacement", "860000000000002", "B", "1-1")])
            .unwrap();
        assert!(matches!(result[0], ReconcileOutcome::Conflict { .. }));
        assert_eq!(store.bindings().unwrap()[0].status, "identity_conflict");
    }

    #[test]
    fn repeated_offline_scan_only_advances_generation_once() {
        let directory = tempfile::tempdir().unwrap();
        let store = BindingStore::open(directory.path().join("bindings.db")).unwrap();
        store
            .replace_confirmed_bindings(&[binding("a", "860000000000001", "A", "1-1")])
            .unwrap();
        store.reconcile(&[]).unwrap();
        let first = store.bindings().unwrap()[0].worker_generation;
        store.reconcile(&[]).unwrap();
        let second = store.bindings().unwrap()[0].worker_generation;
        assert_eq!(first, second);
    }
}
