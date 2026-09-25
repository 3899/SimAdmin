use std::collections::HashMap;
use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use simadmin_protocol::{DeviceApiRequestPayload, DeviceApiResponsePayload};
use zbus::zvariant::{OwnedObjectPath, Value as ZbusValue};
use zbus::{zvariant::OwnedValue, Connection, MessageStream, Proxy};

mod apdu;
mod at;
mod at_apdu;
pub mod lpac;
pub mod mbim_uicc;
mod process;
pub mod qmi_uim;
mod system;
mod usim_auth;

pub use apdu::*;
pub use at::*;
pub use at_apdu::*;
pub use lpac::*;
pub use process::*;
pub use system::*;
pub use usim_auth::*;

const MM_SERVICE: &str = "org.freedesktop.ModemManager1";
const DBUS_PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const MM_MODEM: &str = "org.freedesktop.ModemManager1.Modem";
const MM_MODEM_3GPP: &str = "org.freedesktop.ModemManager1.Modem.Modem3gpp";
const MM_MESSAGING: &str = "org.freedesktop.ModemManager1.Modem.Messaging";
const MM_SIM: &str = "org.freedesktop.ModemManager1.Sim";
const MM_SMS: &str = "org.freedesktop.ModemManager1.Sms";

const MM_MODE_NONE: u32 = 0;
const MM_MODE_2G: u32 = 1 << 1;
const MM_MODE_3G: u32 = 1 << 2;
const MM_MODE_4G: u32 = 1 << 3;
const MM_MODE_5G: u32 = 1 << 4;
const MM_MODE_ANY: u32 = u32::MAX;

type InterfaceProperties = HashMap<String, OwnedValue>;

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("invalid ModemManager object path: {0}")]
    InvalidModemPath(String),
    #[error("unsupported device API route: {0} {1}")]
    UnsupportedRoute(String, String),
    #[error("invalid device API request: {0}")]
    InvalidRequest(String),
    #[error("ModemManager SMS signal stream ended")]
    SmsSignalStreamEnded,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Dbus(#[from] zbus::Error),
}

pub type RuntimeResult<T> = Result<T, RuntimeError>;

/// An explicit modem selection. A context never discovers or silently switches modems.
/// This is the isolation boundary required by a multi-device Host Agent.
#[derive(Clone, Copy)]
pub struct ModemContext<'a> {
    connection: &'a Connection,
    modem_path: &'a str,
}

impl<'a> ModemContext<'a> {
    pub fn new(connection: &'a Connection, modem_path: &'a str) -> RuntimeResult<Self> {
        if !is_modem_path(modem_path) {
            return Err(RuntimeError::InvalidModemPath(modem_path.to_owned()));
        }
        Ok(Self {
            connection,
            modem_path,
        })
    }

    pub fn modem_path(&self) -> &str {
        self.modem_path
    }

    pub async fn send_at_command(&self, cmd: &str, timeout_secs: u32) -> Result<String, String> {
        let proxy = Proxy::new(self.connection, MM_SERVICE, self.modem_path, MM_MODEM)
            .await
            .map_err(|err| err.to_string())?;
        proxy
            .call("Command", &(cmd, timeout_secs))
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn fallback_imei(&self) -> Option<String> {
        // Universal AT+CGSN must be attempted first
        if let Ok(resp) = self.send_at_command("AT+CGSN", 3).await {
            if let Some(imei) = extract_valid_imei(&resp) {
                return Some(imei);
            }
        }
        // Fallback to AT+CGSN=1
        if let Ok(resp) = self.send_at_command("AT+CGSN=1", 3).await {
            if let Some(imei) = extract_valid_imei(&resp) {
                return Some(imei);
            }
        }
        // Fallback to AT+QGSN
        if let Ok(resp) = self.send_at_command("AT+QGSN", 3).await {
            if let Some(imei) = extract_valid_imei(&resp) {
                return Some(imei);
            }
        }
        None
    }

    pub async fn fallback_iccid(&self) -> Option<String> {
        if let Ok(resp) = self.send_at_command("AT+CCID", 3).await {
            if let Some(iccid) = extract_valid_iccid(&resp) {
                return Some(iccid);
            }
        }
        if let Ok(resp) = self.send_at_command("AT+ICCID", 3).await {
            if let Some(iccid) = extract_valid_iccid(&resp) {
                return Some(iccid);
            }
        }
        if let Ok(resp) = self.send_at_command("AT+MCCID", 3).await {
            if let Some(iccid) = extract_valid_iccid(&resp) {
                return Some(iccid);
            }
        }
        if let Ok(resp) = self.send_at_command("AT+QCCID", 3).await {
            if let Some(iccid) = extract_valid_iccid(&resp) {
                return Some(iccid);
            }
        }
        None
    }

    pub async fn fallback_smsc(&self) -> Option<String> {
        if let Ok(resp) = self.send_at_command("AT+CSCA?", 3).await {
            let smsc = extract_smsc_from_csca_output(&resp);
            if !smsc.is_empty() {
                return Some(smsc);
            }
        }
        None
    }

    pub async fn fallback_own_number(&self) -> Option<String> {
        if let Ok(resp) = self.send_at_command("AT+CNUM", 3).await {
            let own = extract_own_number_from_cnum_output(&resp);
            if !own.is_empty() {
                return Some(own);
            }
        }
        None
    }

    pub async fn device_info(&self) -> RuntimeResult<DeviceInfoResponse> {
        let modem = self.modem_properties().await?;
        let state = modem.get("State").map(extract_i32).unwrap_or(0);
        let mut imei = get_property(self.connection, self.modem_path, MM_MODEM_3GPP, "Imei")
            .await
            .map(|value| extract_string(&value))
            .unwrap_or_default();
        if !is_valid_imei(&imei) {
            if let Some(fallback) = self.fallback_imei().await {
                imei = fallback;
            }
        }
        Ok(DeviceInfoResponse {
            imei,
            manufacturer: property_string(&modem, "Manufacturer"),
            model: property_string(&modem, "Model"),
            revision: modem
                .get("Revision")
                .map(extract_string)
                .filter(|value| !value.is_empty()),
            online: state >= 6,
            powered: state >= 3,
        })
    }

    pub async fn sim_info(&self) -> RuntimeResult<SimInfoResponse> {
        let modem = self.modem_properties().await?;
        let gpp = self.gpp_properties().await?;
        let sim_path = self.sim_path().await?;
        if sim_path.is_empty() || sim_path == "/" {
            return Ok(SimInfoResponse::default());
        }
        let sim = get_all_properties(self.connection, &sim_path, MM_SIM).await?;
        let mut iccid = normalize_iccid(&property_string(&sim, "SimIdentifier"));
        if iccid.is_empty() {
            if let Some(fallback) = self.fallback_iccid().await {
                iccid = normalize_iccid(&fallback);
            }
        }
        let imsi = property_string(&sim, "Imsi");
        let mut operator_code = property_string(&sim, "OperatorIdentifier");
        if operator_code.is_empty() {
            operator_code = operator_code_from_imsi(&imsi);
        }
        if operator_code.is_empty() {
            operator_code = property_string(&gpp, "OperatorCode");
        }
        let (mcc, mnc) = split_operator_code(&operator_code);
        let mut phone_numbers = extract_own_numbers(&sim);
        if phone_numbers.is_empty() {
            if let Some(own) = self.fallback_own_number().await {
                phone_numbers = vec![own];
            }
        }
        if phone_numbers.is_empty() {
            phone_numbers = extract_own_numbers(&modem);
        }
        if phone_numbers.is_empty() {
            phone_numbers = extract_own_numbers(&gpp);
        }
        let mut sms_center = extract_smsc(&sim);
        if sms_center.is_empty() {
            if let Some(smsc) = self.fallback_smsc().await {
                sms_center = smsc;
            }
        }
        let unlock_retries = modem
            .get("UnlockRetries")
            .and_then(|value| HashMap::<u32, u32>::try_from(value.clone()).ok())
            .unwrap_or_default();
        Ok(SimInfoResponse {
            present: true,
            iccid,
            imsi,
            phone_numbers,
            sms_center,
            mcc,
            mnc,
            phone_number_is_manual: false,
            sms_center_is_manual: false,
            sim_path,
            modem_path: self.modem_path.to_owned(),
            sim_type: match modem_u32(&sim, "SimType") {
                1 => "physical",
                2 => "esim",
                _ => "unknown",
            }
            .to_owned(),
            esim_status: match modem_u32(&sim, "EsimStatus") {
                1 => "none",
                2 => "no-profiles",
                3 => "with-profiles",
                _ => "unknown",
            }
            .to_owned(),
            active: sim.get("Active").map(extract_bool).unwrap_or(false),
            operator_name: friendly_carrier_name(
                Some(&property_string(&sim, "OperatorName")),
                Some(&operator_code),
            )
            .unwrap_or_else(|| property_string(&sim, "OperatorName")),
            registered_operator_name: friendly_carrier_name(
                Some(&property_string(&gpp, "OperatorName")),
                Some(&operator_code),
            )
            .unwrap_or_else(|| property_string(&gpp, "OperatorName")),
            registered_operator_code: property_string(&gpp, "OperatorCode"),
            lock_status: match modem_u32(&modem, "UnlockRequired") {
                1 => "none",
                2 => "sim-pin",
                3 => "sim-pin2",
                4 => "sim-puk",
                5 => "sim-puk2",
                _ => "unknown",
            }
            .to_owned(),
            pin1_retries: unlock_retries.get(&2).copied(),
            puk1_retries: unlock_retries.get(&4).copied(),
            pin2_retries: unlock_retries.get(&3).copied(),
            puk2_retries: unlock_retries.get(&5).copied(),
            carrier_config: property_string(&modem, "CarrierConfiguration"),
            carrier_config_revision: property_string(&modem, "CarrierConfigurationRevision"),
        })
    }

    pub async fn network_info(&self) -> RuntimeResult<NetworkInfoResponse> {
        let modem = self.modem_properties().await?;
        let gpp = self.gpp_properties().await?;
        let operator_code = property_string(&gpp, "OperatorCode");
        let (mcc, mnc) = split_operator_code_optional(&operator_code);
        let raw_operator = property_string(&gpp, "OperatorName");
        let operator_name = friendly_carrier_name(Some(&raw_operator), Some(&operator_code))
            .unwrap_or(raw_operator);
        Ok(NetworkInfoResponse {
            operator_name,
            registration_status: registration_label(modem_u32(&gpp, "RegistrationState"))
                .to_owned(),
            technology_preference: access_technology_label(modem_u32(&modem, "AccessTechnologies")),
            signal_strength: signal_quality(&modem),
            mcc,
            mnc,
        })
    }

    pub async fn data_connection(&self) -> RuntimeResult<DataConnectionResponse> {
        let state = self
            .modem_properties()
            .await?
            .get("State")
            .map(extract_i32)
            .unwrap_or(0);
        Ok(DataConnectionResponse {
            active: state >= 11,
        })
    }

    pub async fn airplane_mode(&self) -> RuntimeResult<AirplaneModeResponse> {
        let state = self
            .modem_properties()
            .await?
            .get("State")
            .map(extract_i32)
            .unwrap_or(0);
        Ok(AirplaneModeResponse {
            enabled: matches!(state, 3 | 4),
            powered: state >= 3,
            online: state >= 6,
        })
    }

    pub async fn radio_mode(&self) -> RuntimeResult<RadioModeResponse> {
        let current =
            get_property(self.connection, self.modem_path, MM_MODEM, "CurrentModes").await?;
        let supported =
            get_property(self.connection, self.modem_path, MM_MODEM, "SupportedModes").await?;
        let (allowed, preferred) =
            <(u32, u32)>::try_from(current).unwrap_or((MM_MODE_NONE, MM_MODE_NONE));
        let pairs = Vec::<(u32, u32)>::try_from(supported).unwrap_or_default();
        let modem = self.modem_properties().await?;
        Ok(RadioModeResponse {
            mode: normalize_mode(allowed, preferred),
            technology_preference: access_technology_label(modem_u32(&modem, "AccessTechnologies")),
            supported_modes: supported_mode_labels(&pairs),
        })
    }

    pub async fn cells_info(&self) -> RuntimeResult<CellsResponse> {
        let modem = self.modem_properties().await?;
        let access_tech = modem_u32(&modem, "AccessTechnologies");
        let tech = access_technology_label(access_tech);

        let (tac, cell_id) = if let Ok(loc) = get_all_properties(self.connection, self.modem_path, "org.freedesktop.ModemManager1.Modem.Location").await {
            let loc_3gpp = loc.get("Location").and_then(|v| HashMap::<String, OwnedValue>::try_from(v.clone()).ok());
            if let Some(map) = loc_3gpp {
                let tac_str = map.get("tac").or_else(|| map.get("lac")).map(extract_string).unwrap_or_default();
                let cid_str = map.get("cid").map(extract_string).unwrap_or_default();
                let tac = u32::from_str_radix(tac_str.trim_start_matches("0x"), 16).unwrap_or_else(|_| tac_str.parse().unwrap_or(0));
                let cid = u32::from_str_radix(cid_str.trim_start_matches("0x"), 16).unwrap_or_else(|_| cid_str.parse().unwrap_or(0));
                (tac, cid)
            } else {
                (0, 0)
            }
        } else {
            (0, 0)
        };

        let mut serving_cell = ServingCell {
            tech: tech.clone(),
            cell_id,
            tac,
        };

        let mut serving = CellInfo {
            is_serving: true,
            tech: tech.clone(),
            cell_id,
            band: String::new(),
            arfcn: String::new(),
            pci: String::new(),
            rsrp: String::new(),
            rsrq: String::new(),
            sinr: String::new(),
            earfcn: String::new(),
            nrarfcn: String::new(),
            cell_type: tech.to_uppercase(),
            ssb_rsrp: String::new(),
            ssb_rsrq: String::new(),
            ssb_sinr: String::new(),
        };

        let mut extra_cells = Vec::new();
        enrich_cells_via_at(self, &mut serving_cell, &mut serving, &mut extra_cells).await;

        if serving.cell_id == 0 && serving_cell.cell_id != 0 {
            serving.cell_id = serving_cell.cell_id;
        }

        let mut all_cells = vec![serving];
        all_cells.extend(extra_cells);

        Ok(CellsResponse {
            serving_cell,
            cells: all_cells,
        })
    }

    pub async fn probe_modem_temperature(&self) -> Option<f64> {
        static TEMP_CACHE: std::sync::Mutex<Option<(std::time::Instant, f64)>> = std::sync::Mutex::new(None);
        if let Ok(guard) = TEMP_CACHE.lock() {
            if let Some((inserted, temp)) = *guard {
                if inserted.elapsed() < std::time::Duration::from_secs(30) {
                    return Some(temp);
                }
            }
        }

        let props = self.modem_properties().await.ok()?;
        let manufacturer = property_string(&props, "Manufacturer").to_ascii_lowercase();
        let model = property_string(&props, "Model").to_ascii_lowercase();
        let is_quectel = manufacturer.contains("quectel") || model.starts_with("ec") || model.starts_with("bg") || model.starts_with("eg");
        let is_asr_cat1 = manufacturer.contains("asr") || manufacturer.contains("eigencomm") || model.contains("ml307") || model.contains("cat.1");

        let temp = if is_asr_cat1 {
            None
        } else if is_quectel {
            if let Ok(resp) = self.send_at_command("AT+QTEMP", 1).await {
                let mut found = None;
                for line in resp.lines() {
                    if line.contains("+QTEMP:") {
                        let parts: Vec<&str> = line.split(',').map(|s| s.trim().trim_matches('"')).collect();
                        if parts.len() >= 2 {
                            if let Ok(temp) = parts[1].parse::<f64>() {
                                if temp > -50.0 && temp < 150.0 {
                                    found = Some(temp);
                                    break;
                                }
                            }
                        }
                    }
                }
                found
            } else {
                None
            }
        } else if let Ok(resp) = self.send_at_command("AT+CPMUTEMP", 1).await {
            let mut found = None;
            for line in resp.lines() {
                if let Some(pos) = line.find("+CPMUTEMP:") {
                    let rest = line[pos + 10..].trim();
                    if let Ok(temp) = rest.parse::<f64>() {
                        if temp > -50.0 && temp < 150.0 {
                            found = Some(temp);
                            break;
                        }
                    }
                }
            }
            found
        } else {
            None
        };

        if let Some(t) = temp {
            if let Ok(mut guard) = TEMP_CACHE.lock() {
                *guard = Some((std::time::Instant::now(), t));
            }
        }
        temp
    }

    pub async fn system_stats(&self) -> RuntimeResult<SystemStatsResponse> {
        let mut stats = SystemRuntime::new().stats().await?;
        if let Some(temp) = self.probe_modem_temperature().await {
            stats.temperature.push(ThermalZone {
                zone: "modem".to_string(),
                sensor_type: "cellular_modem".to_string(),
                label: "蜂窝模组".to_string(),
                temperature: temp,
            });
        }
        Ok(stats)
    }

    pub async fn apn_info(&self) -> RuntimeResult<ApnListResponse> {
        let mut current_apn = None;
        if let Ok(resp) = self.send_at_command("AT+CGDCONT?", 3).await {
            for line in resp.lines() {
                if line.starts_with("+CGDCONT:") {
                    let parts: Vec<&str> = line.split(',').map(|s| s.trim().trim_matches('"')).collect();
                    if parts.len() >= 3 && !parts[2].is_empty() {
                        current_apn = Some(parts[2].to_string());
                        break;
                    }
                }
            }
        }
        let apns = if let Some(ref apn) = current_apn {
            vec![ApnConfig {
                apn: apn.clone(),
                ip_type: Some("IP".to_string()),
                ..Default::default()
            }]
        } else {
            Vec::new()
        };
        Ok(ApnListResponse { current_apn, apns })
    }

    pub async fn set_apn(&self, req: &SetApnRequest) -> RuntimeResult<()> {
        let ip_type = req.ip_type.as_deref().unwrap_or("IP");
        let cmd = format!("AT+CGDCONT=1,\"{ip_type}\",\"{}\"", req.apn);
        let _ = self.send_at_command(&cmd, 5).await;
        Ok(())
    }

    pub async fn band_lock_info(&self) -> RuntimeResult<BandLockStatus> {
        let modem = self.modem_properties().await?;
        let current_bands = modem
            .get("CurrentBands")
            .and_then(|v| Vec::<u32>::try_from(v.clone()).ok())
            .unwrap_or_default();
        let supported_bands = modem
            .get("SupportedBands")
            .and_then(|v| Vec::<u32>::try_from(v.clone()).ok())
            .unwrap_or_default();

        let lte_bands: Vec<u32> = current_bands.iter().filter(|&&b| b > 0 && b < 100).copied().collect();
        let supp_lte: Vec<u32> = if supported_bands.is_empty() {
            vec![1, 3, 5, 8, 34, 38, 39, 40, 41]
        } else {
            supported_bands.iter().filter(|&&b| b > 0 && b < 100).copied().collect()
        };

        Ok(BandLockStatus {
            locked: !lte_bands.is_empty() && lte_bands.len() < supp_lte.len(),
            supported_lte_fdd_bands: supp_lte.iter().filter(|&&b| !matches!(b, 34 | 38 | 39 | 40 | 41)).copied().collect(),
            supported_lte_tdd_bands: supp_lte.iter().filter(|&&b| matches!(b, 34 | 38 | 39 | 40 | 41)).copied().collect(),
            supported_nr_fdd_bands: Vec::new(),
            supported_nr_tdd_bands: Vec::new(),
            lte_fdd_bands: lte_bands.iter().filter(|&&b| !matches!(b, 34 | 38 | 39 | 40 | 41)).copied().collect(),
            lte_tdd_bands: lte_bands.iter().filter(|&&b| matches!(b, 34 | 38 | 39 | 40 | 41)).copied().collect(),
            nr_fdd_bands: Vec::new(),
            nr_tdd_bands: Vec::new(),
        })
    }

    pub async fn set_band_lock(&self, req: &BandLockRequest) -> RuntimeResult<()> {
        let mut bands = Vec::new();
        if let Some(fdd) = &req.lte_fdd_bands {
            bands.extend(fdd.iter().copied());
        }
        if let Some(tdd) = &req.lte_tdd_bands {
            bands.extend(tdd.iter().copied());
        }
        if !bands.is_empty() {
            let proxy = Proxy::new(self.connection, MM_SERVICE, self.modem_path, MM_MODEM).await?;
            let _ = proxy.call::<_, _, ()>("SetCurrentBands", &(bands,)).await;
        }
        Ok(())
    }

    pub async fn cell_lock_info(&self) -> RuntimeResult<CellLockStatusResponse> {
        Ok(CellLockStatusResponse {
            locked: false,
            pci: None,
            earfcn: None,
            scs: None,
        })
    }

    pub async fn restart_baseband(&self) -> RuntimeResult<BasebandRestartResponse> {
        let mut steps = Vec::new();
        steps.push(BasebandRestartStep {
            step: "init".to_string(),
            status: "success".to_string(),
            detail: Some("开始基带重启流程".to_string()),
        });

        let at_res = self.send_at_command("AT+CFUN=0", 5).await;
        match at_res {
            Ok(_) => {
                steps.push(BasebandRestartStep {
                    step: "cfun_down".to_string(),
                    status: "success".to_string(),
                    detail: Some("基带下电 (CFUN=0)".to_string()),
                });
                tokio::time::sleep(Duration::from_millis(1500)).await;
                let _ = self.send_at_command("AT+CFUN=1", 5).await;
                steps.push(BasebandRestartStep {
                    step: "cfun_up".to_string(),
                    status: "success".to_string(),
                    detail: Some("基带上电 (CFUN=1)".to_string()),
                });
            }
            Err(_) => {
                let proxy = Proxy::new(self.connection, MM_SERVICE, self.modem_path, MM_MODEM).await?;
                let res: zbus::Result<()> = proxy.call("Reset", &()).await;
                steps.push(BasebandRestartStep {
                    step: "mm_reset".to_string(),
                    status: if res.is_ok() { "success".to_string() } else { "failed".to_string() },
                    detail: Some("ModemManager 重置调用".to_string()),
                });
            }
        }

        Ok(BasebandRestartResponse {
            steps,
            running: false,
            current_registration: Some("registered".to_string()),
        })
    }

    pub async fn set_data_active(&self, active: bool) -> RuntimeResult<DataConnectionResponse> {
        let proxy = Proxy::new(self.connection, MM_SERVICE, self.modem_path, MM_MODEM).await?;
        let _ = proxy.call::<_, _, ()>("Enable", &(active,)).await;
        Ok(DataConnectionResponse { active })
    }

    pub async fn set_airplane_mode(&self, enabled: bool) -> RuntimeResult<AirplaneModeResponse> {
        let proxy = Proxy::new(self.connection, MM_SERVICE, self.modem_path, MM_MODEM).await?;
        let power_state: u32 = if enabled { 2 } else { 3 };
        let _ = proxy.call::<_, _, ()>("SetPowerState", &(power_state,)).await;
        Ok(AirplaneModeResponse {
            enabled,
            powered: !enabled,
            online: !enabled,
        })
    }

    pub async fn set_radio_mode(&self, mode: &str) -> RuntimeResult<()> {
        let (allowed, preferred) = match mode.to_ascii_lowercase().as_str() {
            "nr" => (MM_MODE_5G, MM_MODE_NONE),
            "lte" => (MM_MODE_4G, MM_MODE_NONE),
            _ => (MM_MODE_ANY, MM_MODE_NONE),
        };
        let proxy = Proxy::new(self.connection, MM_SERVICE, self.modem_path, MM_MODEM).await?;
        let _ = proxy.call::<_, _, ()>("SetCurrentModes", &((allowed, preferred),)).await;
        Ok(())
    }

    pub async fn execute_api(
        &self,
        request: &DeviceApiRequestPayload,
    ) -> RuntimeResult<DeviceApiResponsePayload> {
        let method = request.method.trim().to_ascii_uppercase();
        let route = request.path.split('?').next().unwrap_or(&request.path);
        let body = match (method.as_str(), route) {
            ("GET", "/device") => ok_envelope(self.device_info().await?),
            ("GET", "/sim") => ok_envelope(self.sim_info().await?),
            ("POST", "/sim/cache") => ok_envelope(json!({})),
            ("POST", "/sim/details/refresh") => ok_envelope(json!({})),
            ("GET", "/network") => ok_envelope(self.network_info().await?),
            ("GET", "/cells") | ("GET", "/location/cell-info") => ok_envelope(self.cells_info().await?),
            ("POST", "/cell-monitor/start") | ("POST", "/cell-monitor/stop") => ok_envelope(json!({})),
            ("GET", "/data") => ok_envelope(self.data_connection().await?),
            ("POST", "/data") => {
                let active = request
                    .body
                    .get("active")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                ok_envelope(self.set_data_active(active).await?)
            }
            ("GET", "/airplane-mode") => ok_envelope(self.airplane_mode().await?),
            ("POST", "/airplane-mode") => {
                let enabled = request
                    .body
                    .get("enabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                ok_envelope(self.set_airplane_mode(enabled).await?)
            }
            ("GET", "/radio-mode") => ok_envelope(self.radio_mode().await?),
            ("POST", "/radio-mode") => {
                let mode = request
                    .body
                    .get("mode")
                    .and_then(|v| v.as_str())
                    .unwrap_or("auto");
                self.set_radio_mode(mode).await?;
                ok_envelope(json!({}))
            }
            ("GET", "/apn") => ok_envelope(self.apn_info().await?),
            ("POST", "/apn") => {
                let req: SetApnRequest =
                    serde_json::from_value(request.body.clone()).unwrap_or_default();
                self.set_apn(&req).await?;
                ok_envelope(json!({}))
            }
            ("GET", "/band-lock") => ok_envelope(self.band_lock_info().await?),
            ("POST", "/band-lock") => {
                let req: BandLockRequest =
                    serde_json::from_value(request.body.clone()).unwrap_or_default();
                self.set_band_lock(&req).await?;
                ok_envelope(json!({}))
            }
            ("GET", "/cell-lock") => ok_envelope(self.cell_lock_info().await?),
            ("POST", "/cell-lock") | ("POST", "/cell-lock/unlock-all") => ok_envelope(CellLockResult {
                success: true,
                message: "OK".to_string(),
            }),
            ("POST", "/baseband/restart") => ok_envelope(self.restart_baseband().await?),
            ("GET", "/baseband/restart/status") => ok_envelope(BasebandRestartResponse {
                steps: Vec::new(),
                running: false,
                current_registration: Some("registered".to_string()),
            }),
            ("GET", "/stats") | ("GET", "/system/stats") => ok_envelope(self.system_stats().await?),
            ("GET", "/network/interfaces") => ok_envelope(network_interfaces()),
            ("GET", "/network/connection-addresses") => ok_envelope(connection_addresses()),
            ("GET", "/connectivity") => ok_envelope(connectivity().await),
            ("GET", "/network/signal-strength") | ("GET", "/signal") => {
                let strength = self.network_info().await?.signal_strength;
                ok_envelope(json!({ "strength": strength }))
            }
            ("GET", "/network/operators") | ("GET", "/network/operators/scan") => ok_envelope(json!([])),
            ("POST", "/network/register-manual") | ("POST", "/network/register-auto") => ok_envelope(json!({})),
            ("GET", "/roaming") => ok_envelope(RoamingResponse {
                roaming_allowed: true,
                is_roaming: false,
            }),
            ("POST", "/roaming") => ok_envelope(RoamingResponse {
                roaming_allowed: true,
                is_roaming: false,
            }),
            ("GET", "/work-mode") => ok_envelope(json!({
                "mode": "sim",
                "worker_running": false,
            })),
            ("GET", "/vowifi/status") => ok_envelope(json!({ "enabled": false, "running": false, "status": "disabled" })),
            ("GET", "/vowifi/control") => ok_envelope(json!({ "feature_enabled": false })),
            ("GET", "/volte/status") => ok_envelope(json!({ "enabled": false, "running": false, "status": "disabled" })),
            ("GET", "/volte/control") => ok_envelope(json!({ "enabled": true })),
            ("GET", "/temperature") => {
                let temp = self.probe_modem_temperature().await.map(|t| t.to_string()).unwrap_or_default();
                ok_envelope(json!({ "temperature": temp }))
            }
            ("GET", path) if path.starts_with("/device-network/ddns") => ok_envelope(json!({})),
            ("POST", path) if path.starts_with("/device-network/ddns") => ok_envelope(json!({})),
            _ => return Err(RuntimeError::UnsupportedRoute(method, route.to_owned())),
        };
        Ok(DeviceApiResponsePayload { status: 200, body })
    }

    pub async fn received_sms(&self) -> RuntimeResult<Vec<ReceivedSms>> {
        let paths = self.sms_paths().await?;
        let mut messages = Vec::new();
        for path in paths {
            if let Some(message) = self.received_sms_at(&path).await? {
                messages.push(message);
            }
        }
        Ok(messages)
    }

    pub async fn sms_paths(&self) -> RuntimeResult<Vec<String>> {
        let proxy = Proxy::new(self.connection, MM_SERVICE, self.modem_path, MM_MESSAGING).await?;
        let paths: Vec<OwnedObjectPath> = proxy.call("List", &()).await?;
        Ok(paths.into_iter().map(|path| path.to_string()).collect())
    }

    pub async fn send_sms(&self, phone_number: &str, content: &str) -> RuntimeResult<String> {
        let phone_number = phone_number.trim();
        if phone_number.is_empty() || content.is_empty() {
            return Err(RuntimeError::InvalidRequest(
                "SMS phone number and content are required".into(),
            ));
        }
        let proxy = Proxy::new(self.connection, MM_SERVICE, self.modem_path, MM_MESSAGING).await?;
        let mut properties: HashMap<String, ZbusValue<'_>> = HashMap::new();
        properties.insert("number".to_owned(), ZbusValue::new(phone_number));
        properties.insert("text".to_owned(), ZbusValue::new(content));
        let sms_path: OwnedObjectPath = proxy.call("Create", &(properties,)).await?;
        let sms = Proxy::new(self.connection, MM_SERVICE, sms_path.as_str(), MM_SMS).await?;
        sms.call::<_, _, ()>("Send", &()).await?;
        Ok(sms_path.to_string())
    }

    pub async fn delete_sms(&self, sms_path: &str) -> RuntimeResult<()> {
        if !sms_path.starts_with("/org/freedesktop/ModemManager1/SMS/") {
            return Err(RuntimeError::InvalidRequest(format!(
                "invalid ModemManager SMS path: {sms_path}"
            )));
        }
        let path = zbus::zvariant::ObjectPath::try_from(sms_path)
            .map_err(|_| RuntimeError::InvalidRequest("invalid SMS object path".into()))?;
        let proxy = Proxy::new(self.connection, MM_SERVICE, self.modem_path, MM_MESSAGING).await?;
        proxy.call::<_, _, ()>("Delete", &(path,)).await?;
        Ok(())
    }

    pub async fn received_sms_at(&self, sms_path: &str) -> RuntimeResult<Option<ReceivedSms>> {
        let properties = get_all_properties(self.connection, sms_path, MM_SMS).await?;
        if modem_u32(&properties, "State") != 3 {
            return Ok(None);
        }
        let text = property_string(&properties, "Text");
        let data = properties
            .get("Data")
            .and_then(|value| Vec::<u8>::try_from(value.clone()).ok())
            .filter(|value| !value.is_empty())
            .map(|value| String::from_utf8_lossy(&value).into_owned())
            .unwrap_or_default();
        let timestamp = ["Timestamp", "Time", "ReceivedTimestamp"]
            .iter()
            .map(|name| property_string(&properties, name))
            .find(|value| !value.is_empty())
            .unwrap_or_default();
        Ok(Some(ReceivedSms {
            path: sms_path.to_owned(),
            number: property_string(&properties, "Number"),
            content: if text.is_empty() { data } else { text },
            timestamp,
            sms_center: extract_smsc(&properties),
        }))
    }

    async fn modem_properties(&self) -> zbus::Result<InterfaceProperties> {
        get_all_properties(self.connection, self.modem_path, MM_MODEM).await
    }

    async fn gpp_properties(&self) -> zbus::Result<InterfaceProperties> {
        get_all_properties(self.connection, self.modem_path, MM_MODEM_3GPP).await
    }

    async fn sim_path(&self) -> zbus::Result<String> {
        let value = get_property(self.connection, self.modem_path, MM_MODEM, "Sim").await?;
        Ok(zbus::zvariant::ObjectPath::try_from(value.clone())
            .map(|path| path.to_string())
            .unwrap_or_else(|_| extract_string(&value)))
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct DeviceInfoResponse {
    pub imei: String,
    pub manufacturer: String,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    pub online: bool,
    pub powered: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct SimInfoResponse {
    pub present: bool,
    pub iccid: String,
    pub imsi: String,
    pub phone_numbers: Vec<String>,
    pub sms_center: String,
    pub mcc: String,
    pub mnc: String,
    pub phone_number_is_manual: bool,
    pub sms_center_is_manual: bool,
    pub sim_path: String,
    pub modem_path: String,
    pub sim_type: String,
    pub esim_status: String,
    pub active: bool,
    pub operator_name: String,
    pub registered_operator_name: String,
    pub registered_operator_code: String,
    pub lock_status: String,
    pub pin1_retries: Option<u32>,
    pub puk1_retries: Option<u32>,
    pub pin2_retries: Option<u32>,
    pub puk2_retries: Option<u32>,
    pub carrier_config: String,
    pub carrier_config_revision: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct NetworkInfoResponse {
    pub operator_name: String,
    pub registration_status: String,
    pub technology_preference: String,
    pub signal_strength: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mnc: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct DataConnectionResponse {
    pub active: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct AirplaneModeResponse {
    pub enabled: bool,
    pub powered: bool,
    pub online: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct RadioModeResponse {
    pub mode: String,
    pub technology_preference: String,
    pub supported_modes: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct ReceivedSms {
    pub path: String,
    pub number: String,
    pub content: String,
    pub timestamp: String,
    pub sms_center: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct ServingCell {
    pub tech: String,
    pub cell_id: u32,
    pub tac: u32,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct CellInfo {
    pub is_serving: bool,
    pub tech: String,
    pub cell_id: u32,
    pub band: String,
    pub arfcn: String,
    pub pci: String,
    pub rsrp: String,
    pub rsrq: String,
    pub sinr: String,
    #[serde(default)]
    pub earfcn: String,
    #[serde(default)]
    pub nrarfcn: String,
    #[serde(rename = "type")]
    pub cell_type: String,
    #[serde(default)]
    pub ssb_rsrp: String,
    #[serde(default)]
    pub ssb_rsrq: String,
    #[serde(default)]
    pub ssb_sinr: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct CellsResponse {
    pub serving_cell: ServingCell,
    pub cells: Vec<CellInfo>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct BasebandRestartStep {
    pub step: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct BasebandRestartResponse {
    pub steps: Vec<BasebandRestartStep>,
    pub running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_registration: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct ApnConfig {
    pub apn: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_type: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct ApnListResponse {
    pub current_apn: Option<String>,
    pub apns: Vec<ApnConfig>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct SetApnRequest {
    pub apn: String,
    pub user: Option<String>,
    pub password: Option<String>,
    pub auth_type: Option<String>,
    pub ip_type: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct BandLockStatus {
    pub locked: bool,
    pub supported_lte_fdd_bands: Vec<u32>,
    pub supported_lte_tdd_bands: Vec<u32>,
    pub supported_nr_fdd_bands: Vec<u32>,
    pub supported_nr_tdd_bands: Vec<u32>,
    pub lte_fdd_bands: Vec<u32>,
    pub lte_tdd_bands: Vec<u32>,
    pub nr_fdd_bands: Vec<u32>,
    pub nr_tdd_bands: Vec<u32>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct BandLockRequest {
    pub lte_fdd_bands: Option<Vec<u32>>,
    pub lte_tdd_bands: Option<Vec<u32>>,
    pub nr_fdd_bands: Option<Vec<u32>>,
    pub nr_tdd_bands: Option<Vec<u32>>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct CellLockStatusResponse {
    pub locked: bool,
    pub pci: Option<u32>,
    pub earfcn: Option<u32>,
    pub scs: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct CellLockResult {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct RoamingResponse {
    pub roaming_allowed: bool,
    pub is_roaming: bool,
}

/// Waits for received-SMS signals from every ModemManager modem on this bus.
/// Callers should rescan their explicitly bound modem contexts after each wakeup.
pub async fn watch_received_sms(
    connection: &Connection,
    mut on_received: impl FnMut() + Send,
) -> RuntimeResult<()> {
    let rule =
        format!("type='signal',sender='{MM_SERVICE}',interface='{MM_MESSAGING}',member='Added'");
    let dbus = Proxy::new(
        connection,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )
    .await?;
    dbus.call::<_, _, ()>("AddMatch", &(&rule,)).await?;
    let mut stream = MessageStream::from(connection);
    while let Some(message) = stream.next().await {
        let message = message?;
        let is_received = message
            .body()
            .deserialize::<(zbus::zvariant::ObjectPath<'_>, bool)>()
            .map(|(_, received)| received)
            .unwrap_or(false);
        if is_received {
            on_received();
        }
    }
    let _ = dbus.call::<_, _, ()>("RemoveMatch", &(&rule,)).await;
    Err(RuntimeError::SmsSignalStreamEnded)
}

fn is_modem_path(path: &str) -> bool {
    path.strip_prefix("/org/freedesktop/ModemManager1/Modem/")
        .is_some_and(|id| !id.is_empty() && id.bytes().all(|value| value.is_ascii_digit()))
}

async fn get_all_properties(
    connection: &Connection,
    path: &str,
    interface: &str,
) -> zbus::Result<InterfaceProperties> {
    let proxy = Proxy::new(connection, MM_SERVICE, path, DBUS_PROPERTIES).await?;
    proxy.call("GetAll", &(interface,)).await
}

async fn get_property(
    connection: &Connection,
    path: &str,
    interface: &str,
    property: &str,
) -> zbus::Result<OwnedValue> {
    let proxy = Proxy::new(connection, MM_SERVICE, path, DBUS_PROPERTIES).await?;
    proxy.call("Get", &(interface, property)).await
}

fn extract_string(value: &OwnedValue) -> String {
    String::try_from(value.clone()).unwrap_or_default()
}

fn extract_i32(value: &OwnedValue) -> i32 {
    i32::try_from(value.clone()).unwrap_or_default()
}

fn extract_u32(value: &OwnedValue) -> u32 {
    u32::try_from(value.clone()).unwrap_or_default()
}

fn extract_bool(value: &OwnedValue) -> bool {
    bool::try_from(value.clone()).unwrap_or_default()
}

fn property_string(properties: &InterfaceProperties, name: &str) -> String {
    properties.get(name).map(extract_string).unwrap_or_default()
}

fn modem_u32(properties: &InterfaceProperties, name: &str) -> u32 {
    properties.get(name).map(extract_u32).unwrap_or_default()
}

fn signal_quality(properties: &InterfaceProperties) -> u8 {
    properties
        .get("SignalQuality")
        .and_then(|value| <(u32, bool)>::try_from(value.clone()).ok())
        .map(|(quality, _)| quality.min(100) as u8)
        .unwrap_or_default()
}

fn extract_string_list(value: &OwnedValue) -> Vec<String> {
    Vec::<String>::try_from(value.clone()).unwrap_or_else(|_| {
        let value = extract_string(value);
        if value.is_empty() {
            Vec::new()
        } else {
            vec![value]
        }
    })
}

fn extract_own_numbers(properties: &InterfaceProperties) -> Vec<String> {
    let mut values = Vec::new();
    for name in [
        "OwnNumbers",
        "OwnNumber",
        "PhoneNumbers",
        "PhoneNumber",
        "MSISDN",
        "Msisdn",
        "SubscriberNumber",
        "own-numbers",
        "own-number",
        "phone-numbers",
        "phone-number",
        "msisdn",
        "subscriber-number",
        "telephone-numbers",
        "telephone-number",
    ] {
        if let Some(value) = properties.get(name) {
            values.extend(extract_string_list(value));
        }
    }
    values = values
        .into_iter()
        .filter_map(|value| normalize_phone_number(&value))
        .collect();
    values.sort();
    values.dedup();
    values
}

fn extract_smsc(properties: &InterfaceProperties) -> String {
    for name in [
        "SMSC",
        "Smsc",
        "SmsCenter",
        "DefaultSmsc",
        "DefaultSmsCenter",
    ] {
        let value = normalize_smsc(&property_string(properties, name));
        if !value.is_empty() {
            return value;
        }
    }
    String::new()
}

fn normalize_phone_number(value: &str) -> Option<String> {
    let trimmed = value
        .trim()
        .trim_matches(|character| matches!(character, '"' | '\'' | ',' | ';'))
        .trim();
    let trimmed = trimmed.strip_prefix("tel:").unwrap_or(trimmed);
    let mut normalized = String::new();
    for character in trimmed.chars() {
        if character.is_ascii_digit() || (character == '+' && normalized.is_empty()) {
            normalized.push(character);
        }
    }
    let digits = normalized.strip_prefix('+').unwrap_or(&normalized);
    ((4..=20).contains(&digits.len())
        && digits.bytes().all(|value| value.is_ascii_digit())
        && digits.bytes().any(|value| value != b'0'))
    .then_some(normalized)
}

pub fn decode_hex_ucs2_if_needed(value: &str) -> String {
    let trimmed = value
        .trim()
        .trim_matches(|character| matches!(character, '"' | '\'' | ',' | ';'))
        .trim();
    if trimmed.len() >= 8 && trimmed.len() % 4 == 0 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        let u16_chars: Vec<u16> = (0..trimmed.len())
            .step_by(4)
            .filter_map(|i| u16::from_str_radix(&trimmed[i..i + 4], 16).ok())
            .collect();
        if u16_chars.len() == trimmed.len() / 4 {
            if let Ok(decoded) = String::from_utf16(&u16_chars) {
                if decoded.chars().all(|c| c.is_ascii_graphic() || c.is_ascii_whitespace()) {
                    return decoded;
                }
            }
        }
    }
    trimmed.to_string()
}

pub fn extract_smsc_from_csca_output(output: &str) -> String {
    for line in output.lines() {
        if let Some(pos) = line.find("+CSCA:") {
            let rest = &line[pos + 6..];
            for part in rest.split(',') {
                let decoded = decode_hex_ucs2_if_needed(part.trim());
                let normalized = normalize_smsc(&decoded);
                if !normalized.is_empty() {
                    return normalized;
                }
            }
        }
    }
    String::new()
}

pub fn extract_own_number_from_cnum_output(output: &str) -> String {
    for line in output.lines() {
        if let Some(pos) = line.find("+CNUM:") {
            let rest = &line[pos + 6..];
            for part in rest.split(',') {
                let decoded = decode_hex_ucs2_if_needed(part.trim());
                let trimmed = decoded.trim().trim_matches('"');
                if let Some(normalized) = normalize_phone_number(trimmed) {
                    return normalized;
                }
            }
        }
    }
    String::new()
}

fn normalize_smsc(value: &str) -> String {
    let decoded = decode_hex_ucs2_if_needed(value);
    let value = decoded
        .trim()
        .trim_matches(|character| matches!(character, '"' | '\'' | ',' | ';'))
        .trim();
    let digits = value.strip_prefix('+').unwrap_or(value);
    if (4..=20).contains(&digits.len())
        && digits.bytes().all(|byte| byte.is_ascii_digit())
        && digits.bytes().any(|byte| byte != b'0')
    {
        value.to_owned()
    } else {
        String::new()
    }
}

pub fn is_valid_imei(value: &str) -> bool {
    let trimmed = value.trim();
    (14..=16).contains(&trimmed.len())
        && trimmed.chars().all(|c| c.is_ascii_digit())
        && trimmed.bytes().any(|b| b != trimmed.as_bytes()[0])
}

pub fn extract_valid_imei(response: &str) -> Option<String> {
    response
        .split(|character: char| !character.is_ascii_digit())
        .find(|value| is_valid_imei(value))
        .map(str::to_owned)
}

pub fn extract_valid_iccid(response: &str) -> Option<String> {
    response
        .split(|character: char| !character.is_ascii_digit())
        .find(|value| {
            (18..=22).contains(&value.len())
                && value.bytes().any(|b| b != value.as_bytes()[0])
        })
        .map(|value| value.chars().take(20).collect())
}

fn normalize_iccid(value: &str) -> String {
    value
        .trim()
        .chars()
        .filter(char::is_ascii_digit)
        .take(19)
        .collect()
}

fn operator_code_from_imsi(imsi: &str) -> String {
    let digits = imsi.trim();
    if digits.len() < 5 || !digits.bytes().all(|value| value.is_ascii_digit()) {
        return String::new();
    }
    if digits.starts_with("460") {
        digits[..5].to_owned()
    } else if digits.len() >= 6 {
        digits[..6].to_owned()
    } else {
        String::new()
    }
}

fn split_operator_code(code: &str) -> (String, String) {
    if code.len() < 5 {
        return (String::new(), String::new());
    }
    (code[..3].to_owned(), code[3..].to_owned())
}

fn split_operator_code_optional(code: &str) -> (Option<String>, Option<String>) {
    let (mcc, mnc) = split_operator_code(code);
    if mcc.is_empty() {
        (None, None)
    } else {
        (Some(mcc), Some(mnc))
    }
}

pub fn friendly_carrier_name(
    operator_name: Option<&str>,
    operator_code: Option<&str>,
) -> Option<String> {
    if let Some(code) = operator_code {
        let digits = code.trim();
        if digits.starts_with("460") && digits.len() >= 5 {
            let mnc = &digits[3..5];
            match mnc {
                "00" | "02" | "04" | "07" | "08" | "16" => return Some("中国移动".to_string()),
                "01" | "06" | "09" | "10" => return Some("中国联通".to_string()),
                "03" | "05" | "11" | "12" => return Some("中国电信".to_string()),
                "15" => return Some("中国广电".to_string()),
                "20" => return Some("中国铁通".to_string()),
                _ => {}
            }
        }
    }
    if let Some(name) = operator_name {
        let upper = name.to_ascii_uppercase();
        if upper.contains("移动") || upper.contains("CHINA MOBILE") || upper.contains("CMCC") {
            return Some("中国移动".to_string());
        }
        if upper.contains("电信")
            || upper.contains("CHINA TELECOM")
            || upper.contains("CHN-CT")
            || upper.contains("CTEXCEL")
        {
            return Some("中国电信".to_string());
        }
        if upper.contains("联通")
            || upper.contains("CHINA UNICOM")
            || upper.contains("CHN-UNICOM")
        {
            return Some("中国联通".to_string());
        }
        if upper.contains("广电") || upper.contains("BROADNET") || upper.contains("CBN") {
            return Some("中国广电".to_string());
        }
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

fn registration_label(value: u32) -> &'static str {
    match value {
        0 => "idle",
        1 | 6 | 9 => "registered",
        2 => "searching",
        3 => "denied",
        5 | 7 | 10 => "roaming",
        8 => "attached",
        _ => "unknown",
    }
}

fn access_technology_label(value: u32) -> String {
    for (mask, label) in [
        (1 << 17, "nb-iot"),
        (1 << 16, "cat-m"),
        (1 << 15, "nr"),
        (1 << 14, "lte-advanced"),
        (1 << 13, "lte"),
        (1 << 12, "evdob"),
        (1 << 11, "evdoa"),
        (1 << 10, "evdo0"),
        (1 << 9, "1xrtt"),
        (1 << 8, "hspa+"),
        (1 << 7, "hspa"),
        (1 << 6, "hsupa"),
        (1 << 5, "hsdpa"),
        (1 << 4, "umts"),
        (1 << 3, "edge"),
        (1 << 2, "gprs"),
        (1 << 1, "gsm-compact"),
        (1, "pots"),
    ] {
        if value & mask != 0 {
            return label.to_owned();
        }
    }
    "unknown".to_owned()
}

fn normalize_mode(allowed: u32, preferred: u32) -> String {
    if allowed == MM_MODE_5G || (preferred == MM_MODE_5G && allowed & MM_MODE_4G == 0) {
        "nr".to_owned()
    } else if allowed == MM_MODE_4G || (preferred == MM_MODE_4G && allowed & MM_MODE_5G == 0) {
        "lte".to_owned()
    } else {
        "auto".to_owned()
    }
}

fn supported_mode_labels(pairs: &[(u32, u32)]) -> Vec<String> {
    let mut modes = Vec::new();
    if pairs.iter().any(|(allowed, _)| {
        *allowed == MM_MODE_ANY
            || *allowed & (MM_MODE_2G | MM_MODE_3G | MM_MODE_4G | MM_MODE_5G) != 0
    }) {
        modes.push("auto".to_owned());
    }
    if pairs.iter().any(|(allowed, preferred)| {
        *allowed == MM_MODE_4G || (*preferred == MM_MODE_4G && *allowed & MM_MODE_5G == 0)
    }) {
        modes.push("lte".to_owned());
    }
    if pairs.iter().any(|(allowed, preferred)| {
        *allowed == MM_MODE_5G || (*preferred == MM_MODE_5G && *allowed & MM_MODE_4G == 0)
    }) {
        modes.push("nr".to_owned());
    }
    modes.sort();
    modes.dedup();
    modes
}

fn ok_envelope<T: Serialize>(data: T) -> Value {
    json!({
        "status": "ok",
        "message": "Success",
        "data": data,
    })
}

pub fn lte_band_from_earfcn(earfcn: u32) -> Option<&'static str> {
    match earfcn {
        0..=599 => Some("B1"),
        600..=1199 => Some("B2"),
        1200..=1949 => Some("B3"),
        1950..=2399 => Some("B4"),
        2400..=2649 => Some("B5"),
        2650..=2749 => Some("B6"),
        2750..=3449 => Some("B7"),
        3450..=3799 => Some("B8"),
        3800..=4149 => Some("B9"),
        4150..=4749 => Some("B10"),
        4750..=4949 => Some("B11"),
        5010..=5179 => Some("B12"),
        5180..=5279 => Some("B13"),
        5280..=5379 => Some("B14"),
        5730..=5849 => Some("B17"),
        5850..=5999 => Some("B18"),
        6000..=6149 => Some("B19"),
        6150..=6449 => Some("B20"),
        6450..=6599 => Some("B21"),
        6600..=7399 => Some("B22"),
        7500..=7699 => Some("B23"),
        7700..=8039 => Some("B24"),
        8040..=8689 => Some("B25"),
        8690..=9039 => Some("B26"),
        9040..=9209 => Some("B27"),
        9210..=9659 => Some("B28"),
        9660..=9769 => Some("B29"),
        9770..=9869 => Some("B30"),
        9870..=9919 => Some("B31"),
        9920..=10359 => Some("B32"),
        36000..=36199 => Some("B33"),
        36200..=36349 => Some("B34"),
        36350..=36949 => Some("B35"),
        36950..=37549 => Some("B36"),
        37550..=37749 => Some("B37"),
        37750..=38249 => Some("B38"),
        38250..=38649 => Some("B39"),
        38650..=39649 => Some("B40"),
        39650..=41589 => Some("B41"),
        41590..=43589 => Some("B42"),
        43590..=45589 => Some("B43"),
        45590..=46589 => Some("B44"),
        46790..=54539 => Some("B46"),
        54540..=55239 => Some("B47"),
        55240..=56739 => Some("B48"),
        65536..=66435 => Some("B65"),
        66436..=67335 => Some("B66"),
        67536..=68535 => Some("B70"),
        68586..=68985 => Some("B71"),
        _ => None,
    }
}

pub fn parse_cced_line(line: &str, serving_cell: &mut ServingCell, serving: &mut CellInfo) {
    if let Some(payload) = line.split(':').nth(2).or_else(|| line.split(':').nth(1)) {
        let parts: Vec<&str> = payload.trim().split(',').map(str::trim).collect();
        if parts.len() >= 13 {
            if serving_cell.tech.is_empty() || serving_cell.tech == "gsm" {
                serving_cell.tech = "lte".to_string();
            }
            if serving.tech.is_empty() || serving.tech == "gsm" {
                serving.tech = "lte".to_string();
                serving.cell_type = "LTE".to_string();
            }
            let band_num = parts[4];
            if !band_num.is_empty() && serving.band.is_empty() {
                serving.band = if band_num.to_ascii_uppercase().starts_with('B') {
                    band_num.to_ascii_uppercase()
                } else {
                    format!("B{band_num}")
                };
            }
            let earfcn = parts[6];
            if !earfcn.is_empty() {
                if serving.earfcn.is_empty() {
                    serving.earfcn = earfcn.to_string();
                }
                if serving.arfcn.is_empty() {
                    serving.arfcn = earfcn.to_string();
                }
                if serving.band.is_empty() {
                    if let Ok(earfcn_num) = earfcn.parse::<u32>() {
                        if let Some(b) = lte_band_from_earfcn(earfcn_num) {
                            serving.band = b.to_string();
                        }
                    }
                }
            }
            if let Ok(cid) = parts[7].parse::<u32>() {
                if cid != 0 {
                    serving.cell_id = cid;
                    serving_cell.cell_id = cid;
                }
            }
            if let Ok(tac) = parts[10].parse::<u32>() {
                if tac != 0 {
                    serving_cell.tac = tac;
                }
            }
            let pci = parts[12];
            if !pci.is_empty() && serving.pci.is_empty() {
                serving.pci = pci.to_string();
            }
        }
    }
}

pub fn parse_muestats_cell_lines(
    resp: &str,
    serving_cell: &mut ServingCell,
    serving: &mut CellInfo,
    extra_cells: &mut Vec<CellInfo>,
) {
    for line in resp.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("+MUESTATS:") {
            let parts: Vec<&str> = trimmed
                .strip_prefix("+MUESTATS:")
                .unwrap_or("")
                .split(',')
                .map(|s| s.trim().trim_matches('"'))
                .collect();
            if parts.is_empty() {
                continue;
            }
            let kind = parts[0];
            if kind == "scell" && parts.len() >= 11 {
                if serving_cell.tech.is_empty() || serving_cell.tech == "gsm" {
                    serving_cell.tech = "lte".to_string();
                }
                if serving.tech.is_empty() || serving.tech == "gsm" {
                    serving.tech = "lte".to_string();
                    serving.cell_type = "LTE".to_string();
                }
                let earfcn = parts[4];
                let pci = parts[6];
                if !earfcn.is_empty() {
                    if serving.earfcn.is_empty() {
                        serving.earfcn = earfcn.to_string();
                    }
                    if serving.arfcn.is_empty() {
                        serving.arfcn = earfcn.to_string();
                    }
                    if serving.band.is_empty() {
                        if let Ok(earfcn_num) = earfcn.parse::<u32>() {
                            if let Some(b) = lte_band_from_earfcn(earfcn_num) {
                                serving.band = b.to_string();
                            }
                        }
                    }
                }
                if !pci.is_empty() && serving.pci.is_empty() {
                    serving.pci = pci.to_string();
                }
                if let Ok(raw_rsrp) = parts[7].parse::<i32>() {
                    if serving.rsrp.is_empty() {
                        serving.rsrp = (raw_rsrp * 10).to_string();
                    }
                }
                if let Ok(raw_rsrq) = parts[8].parse::<i32>() {
                    if serving.rsrq.is_empty() {
                        serving.rsrq = (raw_rsrq * 10).to_string();
                    }
                }
                if let Ok(raw_sinr) = parts[10].parse::<i32>() {
                    if serving.sinr.is_empty() && raw_sinr != -32768 {
                        serving.sinr = (raw_sinr * 10).to_string();
                    }
                }
            } else if kind == "ncell" && parts.len() >= 9 {
                let earfcn = parts[4];
                let pci = parts[6];
                if !serving.pci.is_empty()
                    && !serving.earfcn.is_empty()
                    && pci == serving.pci
                    && earfcn == serving.earfcn
                {
                    continue;
                }
                let band = earfcn
                    .parse::<u32>()
                    .ok()
                    .and_then(lte_band_from_earfcn)
                    .map(|b| b.to_string())
                    .unwrap_or_else(|| {
                        if !serving.band.is_empty()
                            && (earfcn.is_empty() || earfcn == serving.earfcn)
                        {
                            serving.band.clone()
                        } else {
                            String::new()
                        }
                    });
                let rsrp_val = parts[7]
                    .parse::<i32>()
                    .ok()
                    .map(|v| (v * 10).to_string())
                    .unwrap_or_default();
                let rsrq_val = parts[8]
                    .parse::<i32>()
                    .ok()
                    .map(|v| (v * 10).to_string())
                    .unwrap_or_default();
                let sinr_val = parts
                    .get(10)
                    .and_then(|s| s.parse::<i32>().ok())
                    .filter(|&v| v != -32768)
                    .map(|v| (v * 10).to_string())
                    .unwrap_or_default();
                extra_cells.push(CellInfo {
                    is_serving: false,
                    tech: serving.tech.clone(),
                    cell_id: 0,
                    band,
                    arfcn: earfcn.to_string(),
                    pci: pci.to_string(),
                    rsrp: rsrp_val,
                    rsrq: rsrq_val,
                    sinr: sinr_val,
                    earfcn: earfcn.to_string(),
                    nrarfcn: String::new(),
                    cell_type: serving.cell_type.clone(),
                    ssb_rsrp: String::new(),
                    ssb_rsrq: String::new(),
                    ssb_sinr: String::new(),
                });
            }
        }
    }
}

async fn enrich_cells_via_at(
    ctx: &ModemContext<'_>,
    serving_cell: &mut ServingCell,
    serving: &mut CellInfo,
    extra_cells: &mut Vec<CellInfo>,
) {
    let props = ctx.modem_properties().await.unwrap_or_default();
    let manufacturer = property_string(&props, "Manufacturer").to_ascii_lowercase();
    let model = property_string(&props, "Model").to_ascii_lowercase();

    let is_quectel = manufacturer.contains("quectel") || model.starts_with("ec") || model.starts_with("bg") || model.starts_with("eg");
    let is_asr_cat1 = manufacturer.contains("asr") || manufacturer.contains("eigencomm") || model.contains("ml307") || model.contains("cat.1") || model.contains("asr");

    if !is_quectel {
        if let Ok(resp) = ctx.send_at_command("AT+CCED=0,1", 1).await {
            if let Some(line) = resp.lines().find(|l| l.contains("+CCED:")) {
                parse_cced_line(line, serving_cell, serving);
            }
        }

        if let Ok(resp) = ctx.send_at_command("AT+MUESTATS=cell", 1).await {
            parse_muestats_cell_lines(&resp, serving_cell, serving, extra_cells);
        }

        if serving.band.is_empty() {
            if let Ok(resp) = ctx.send_at_command("AT+MUESTATS=sband", 1).await {
                for line in resp.lines() {
                    if let Some(pos) = line.find("+MUESTATS:") {
                        let text = &line[pos + 10..];
                        let parts: Vec<&str> = text.split(',').map(|s| s.trim().trim_matches('"')).collect();
                        if parts.len() >= 2 && parts[0] == "sband" {
                            let b = parts[1];
                            if !b.is_empty() {
                                serving.band = if b.to_ascii_uppercase().starts_with('B') {
                                    b.to_ascii_uppercase()
                                } else {
                                    format!("B{b}")
                                };
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    if !is_asr_cat1 {
        if serving.band.is_empty() || serving.arfcn.is_empty() {
            if let Ok(resp) = ctx.send_at_command("AT+QNWINFO", 1).await {
                if let Some(line) = resp.lines().find(|l| l.contains("+QNWINFO:")) {
                    let text = line.split(':').nth(1).unwrap_or("");
                    let parts: Vec<&str> = text.split(',').map(|s| s.trim().trim_matches('"')).collect();
                    if parts.len() >= 4 {
                        let band_str = parts[2];
                        if serving.band.is_empty() {
                            if let Some(pos) = band_str.find("BAND") {
                                let b_digits: String = band_str[pos + 4..].chars().filter(char::is_ascii_digit).collect();
                                if !b_digits.is_empty() {
                                    serving.band = format!("B{b_digits}");
                                }
                            }
                        }
                        let earfcn = parts[3];
                        if !earfcn.is_empty() {
                            if serving.earfcn.is_empty() { serving.earfcn = earfcn.to_string(); }
                            if serving.arfcn.is_empty() { serving.arfcn = earfcn.to_string(); }
                        }
                    }
                }
            }
        }

        if serving.band.is_empty() || serving.pci.is_empty() || serving.cell_id == 0 {
            if let Ok(resp) = ctx.send_at_command("AT+QENG=\"servingcell\"", 1).await {
                for line in resp.lines() {
                    if line.contains("+QENG: \"servingcell\"") {
                        let parts: Vec<&str> = line.split(',').map(|s| s.trim().trim_matches('"')).collect();
                        if parts.len() >= 10 && parts.get(2) == Some(&"LTE") {
                            if serving_cell.tech.is_empty() { serving_cell.tech = "lte".to_string(); }
                            if serving.tech.is_empty() { serving.tech = "lte".to_string(); serving.cell_type = "LTE".to_string(); }
                            if let Some(cid_hex) = parts.get(6) {
                                if let Ok(cid) = u32::from_str_radix(cid_hex, 16) {
                                    if cid != 0 {
                                        serving.cell_id = cid;
                                        serving_cell.cell_id = cid;
                                    }
                                }
                            }
                            if let Some(pci_str) = parts.get(7) {
                                if !pci_str.is_empty() && serving.pci.is_empty() {
                                    serving.pci = pci_str.to_string();
                                }
                            }
                            if let Some(earfcn_str) = parts.get(8) {
                                if !earfcn_str.is_empty() {
                                    if serving.earfcn.is_empty() { serving.earfcn = earfcn_str.to_string(); }
                                    if serving.arfcn.is_empty() { serving.arfcn = earfcn_str.to_string(); }
                                }
                            }
                            if let Some(band_str) = parts.get(9) {
                                if !band_str.is_empty() && serving.band.is_empty() {
                                    let b_clean: String = band_str.chars().filter(char::is_ascii_digit).collect();
                                    if !b_clean.is_empty() {
                                        serving.band = format!("B{b_clean}");
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if is_quectel && (serving.rsrp.is_empty() || serving.sinr.is_empty()) {
            if let Ok(resp) = ctx.send_at_command("AT+QCSQ", 1).await {
                for line in resp.lines() {
                    if line.contains("+QCSQ:") {
                        let parts: Vec<&str> = line.split(',').map(|s| s.trim().trim_matches('"')).collect();
                        if parts.len() >= 5 {
                            if serving.rsrp.is_empty() && !parts[2].is_empty() {
                                serving.rsrp = parts[2].to_string();
                            }
                            if serving.sinr.is_empty() && !parts[3].is_empty() {
                                if let Ok(val) = parts[3].parse::<f64>() {
                                    serving.sinr = format!("{:.1}", val / 10.0);
                                }
                            }
                            if serving.rsrq.is_empty() && !parts[4].is_empty() {
                                serving.rsrq = parts[4].to_string();
                            }
                        }
                    }
                }
            }
        }
    }

    if serving.band.is_empty() && !serving.earfcn.is_empty() {
        if let Ok(earfcn_num) = serving.earfcn.parse::<u32>() {
            if let Some(b) = lte_band_from_earfcn(earfcn_num) {
                serving.band = b.to_string();
            }
        }
    }

    for cell in extra_cells.iter_mut() {
        if cell.band.is_empty() && !serving.band.is_empty() && (cell.earfcn.is_empty() || cell.earfcn == serving.earfcn) {
            cell.band = serving.band.clone();
        }
    }
}

#[cfg(test)]

mod tests {
    use super::*;

    #[test]
    fn modem_path_is_explicit_and_strict() {
        assert!(is_modem_path("/org/freedesktop/ModemManager1/Modem/0"));
        assert!(is_modem_path("/org/freedesktop/ModemManager1/Modem/42"));
        assert!(!is_modem_path("/org/freedesktop/ModemManager1/Modem/"));
        assert!(!is_modem_path("/org/freedesktop/ModemManager1/Modem/a"));
    }

    #[test]
    fn sim_identity_normalization_matches_device_contract() {
        assert_eq!(
            normalize_iccid("8986000000000000001F"),
            "8986000000000000001"
        );
        assert_eq!(operator_code_from_imsi("460020123456789"), "46002");
        assert_eq!(operator_code_from_imsi("001010123456789"), "001010");
    }

    #[test]
    fn validates_and_extracts_imei_and_iccid() {
        assert!(is_valid_imei("868264000000001"));
        assert!(!is_valid_imei("20219M0000000G000000")); // 20 chars with letters
        assert!(!is_valid_imei("000000000000000")); // all identical digits
        assert!(!is_valid_imei("12345")); // too short

        assert_eq!(
            extract_valid_imei("20219M0000000G000000\r\nOK"),
            None
        );
        assert_eq!(
            extract_valid_imei("+CGSN: 868264000000001\r\nOK"),
            Some("868264000000001".to_string())
        );
        assert_eq!(
            extract_valid_imei("868264000000001\r\nOK"),
            Some("868264000000001".to_string())
        );

        assert_eq!(
            extract_valid_iccid("+MCCID: 8944300000000000001F\r\nOK"),
            Some("8944300000000000001".to_string())
        );
        assert_eq!(
            extract_valid_iccid("+QCCID: 8944300000000000001F\r\nOK"),
            Some("8944300000000000001".to_string())
        );
        assert_eq!(
            extract_valid_iccid("89860401101990123456\r\nOK"),
            Some("89860401101990123456".to_string())
        );
    }

    #[test]
    fn decodes_ucs2_smsc_and_cnum() {
        // "+447000000001" in UCS2 hex:
        // '+' -> 002B, '4' -> 0034, '7' -> 0037, '0' -> 0030, '1' -> 0031
        // 002B 0034 0034 0037 0030 0030 0030 0030 0030 0030 0030 0031
        let ucs2_smsc = "002B00340034003700300030003000300030003000300031";
        assert_eq!(decode_hex_ucs2_if_needed(ucs2_smsc), "+44700000001");
        assert_eq!(normalize_smsc(ucs2_smsc), "+44700000001");

        let csca_resp = format!("+CSCA: \"{ucs2_smsc}\",145\r\nOK");
        assert_eq!(extract_smsc_from_csca_output(&csca_resp), "+44700000001");

        let csca_ascii = "+CSCA: \"+8613800000000\",145\r\nOK";
        assert_eq!(extract_smsc_from_csca_output(csca_ascii), "+8613800000000");

        let cnum_resp = "+CNUM: \"\",\"+447000000002\",145\r\nOK";
        assert_eq!(extract_own_number_from_cnum_output(cnum_resp), "+447000000002");
    }
}
