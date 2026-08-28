use std::{
    env,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    ApduArbiter, ApduError, ApduOperation, ProcessExecutor, ProcessOutput, SystemProcessExecutor,
};

pub const DEFAULT_LPAC_PROGRAM: &str = "lpac";
const DEFAULT_READ_TIMEOUT: Duration = Duration::from_secs(20);
const DEFAULT_MUTATION_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LpacApduBackend {
    Qmi { device: String, slot: u8 },
    Mbim { device: String, slot: u8 },
    AtCsim { device: String },
}

impl LpacApduBackend {
    pub fn driver_name(&self) -> &'static str {
        match self {
            Self::Qmi { .. } => "qmi",
            Self::Mbim { .. } => "mbim",
            Self::AtCsim { .. } => "at_csim",
        }
    }

    fn environment(&self) -> Vec<(String, String)> {
        let mut values = vec![
            ("LPAC_APDU".to_owned(), self.driver_name().to_owned()),
            ("LPAC_HTTP".to_owned(), "curl".to_owned()),
        ];
        match self {
            Self::Qmi { device, slot } => {
                values.push(("LPAC_APDU_QMI_DEVICE".to_owned(), device.clone()));
                values.push(("LPAC_APDU_QMI_UIM_SLOT".to_owned(), slot.to_string()));
            }
            Self::Mbim { device, slot } => {
                values.push(("LPAC_APDU_MBIM_DEVICE".to_owned(), device.clone()));
                values.push(("LPAC_APDU_MBIM_UIM_SLOT".to_owned(), slot.to_string()));
                values.push(("LPAC_APDU_MBIM_USE_PROXY".to_owned(), "true".to_owned()));
                values.push((
                    "LPAC_APDU_MBIM_SKIP_SLOT_MAPPING".to_owned(),
                    "true".to_owned(),
                ));
            }
            Self::AtCsim { device } => {
                values.push(("LPAC_APDU_AT_DEVICE".to_owned(), device.clone()));
            }
        }
        values
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LpacError {
    #[error("lpac is unavailable: {0}")]
    Unavailable(String),
    #[error("lpac command failed: {0}")]
    Command(String),
    #[error("lpac response is invalid: {0}")]
    InvalidResponse(String),
    #[error(transparent)]
    Apdu(#[from] ApduError),
}

pub type LpacResult<T> = Result<T, LpacError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EuiccProfile {
    pub iccid: String,
    pub name: String,
    pub provider: String,
    pub state: String,
    pub profile_class: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub isdp_aid: Option<String>,
    #[serde(default)]
    pub raw: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LpacMutationResult {
    pub action: String,
    pub message: String,
    #[serde(default)]
    pub data: Value,
}

#[derive(Clone)]
pub struct LpacProfileService {
    program: PathBuf,
    backend: LpacApduBackend,
    arbiter: ApduArbiter,
    executor: Arc<dyn ProcessExecutor>,
}

impl LpacProfileService {
    pub fn new(
        device_id: impl Into<String>,
        program: impl Into<PathBuf>,
        backend: LpacApduBackend,
    ) -> Self {
        let device_id = device_id.into();
        Self::with_executor(
            program,
            backend,
            ApduArbiter::new(device_id),
            Arc::new(SystemProcessExecutor),
        )
    }

    pub fn with_arbiter(
        program: impl Into<PathBuf>,
        backend: LpacApduBackend,
        arbiter: ApduArbiter,
    ) -> Self {
        Self::with_executor(program, backend, arbiter, Arc::new(SystemProcessExecutor))
    }

    pub fn with_executor(
        program: impl Into<PathBuf>,
        backend: LpacApduBackend,
        arbiter: ApduArbiter,
        executor: Arc<dyn ProcessExecutor>,
    ) -> Self {
        Self {
            program: program.into(),
            backend,
            arbiter,
            executor,
        }
    }

    pub fn driver_available(&self, timeout: Duration) -> LpacResult<bool> {
        let output = self.execute_process(&["driver", "list"], timeout)?;
        let value = parse_last_json(&output.stdout)?;
        let root = response_data(&value);
        let apdu = root
            .get("LPAC_APDU")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .any(|driver| driver == self.backend.driver_name());
        let http = root
            .get("LPAC_HTTP")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .any(|driver| driver == "curl");
        Ok(apdu && http)
    }

    pub fn profiles(&self) -> LpacResult<Vec<EuiccProfile>> {
        self.profiles_with_timeout(DEFAULT_READ_TIMEOUT)
    }

    pub fn profiles_with_timeout(&self, timeout: Duration) -> LpacResult<Vec<EuiccProfile>> {
        self.arbiter.execute(ApduOperation::ReadOnly, || {
            let response = self.run("profiles", &["profile", "list"], timeout)?;
            let profiles = response
                .data
                .get("profiles")
                .or_else(|| response.data.get("profileInfo"))
                .or_else(|| response.data.get("profile_info"))
                .unwrap_or(&response.data)
                .as_array()
                .map(|items| items.iter().map(normalize_profile).collect())
                .unwrap_or_default();
            Ok(profiles)
        })
    }

    pub fn enable_profile(
        &self,
        identifier: &str,
        refresh: bool,
    ) -> LpacResult<LpacMutationResult> {
        let identifier = validate_identifier(identifier)?;
        self.mutate(
            "enable",
            &[
                "profile",
                "enable",
                identifier,
                if refresh { "1" } else { "0" },
            ],
        )
    }

    pub fn disable_profile(
        &self,
        identifier: &str,
        refresh: bool,
    ) -> LpacResult<LpacMutationResult> {
        let identifier = validate_identifier(identifier)?;
        self.mutate(
            "disable",
            &[
                "profile",
                "disable",
                identifier,
                if refresh { "1" } else { "0" },
            ],
        )
    }

    pub fn delete_profile(&self, identifier: &str) -> LpacResult<LpacMutationResult> {
        let identifier = validate_identifier(identifier)?;
        self.mutate("delete", &["profile", "delete", identifier])
    }

    fn mutate(&self, action: &str, arguments: &[&str]) -> LpacResult<LpacMutationResult> {
        self.arbiter.execute(ApduOperation::ProfileMutation, || {
            self.run(action, arguments, DEFAULT_MUTATION_TIMEOUT)
        })
    }

    fn run(
        &self,
        action: &str,
        arguments: &[&str],
        timeout: Duration,
    ) -> LpacResult<LpacMutationResult> {
        let output = self.execute_process(arguments, timeout)?;
        let value = parse_last_json(&output.stdout)?;
        let payload = value.get("payload").unwrap_or(&value);
        let code = payload
            .get("code")
            .and_then(Value::as_i64)
            .unwrap_or(if output.success { 0 } else { 1 });
        let message = string_from(payload, &["message", "msg", "error"])
            .or_else(|| (!output.stderr.trim().is_empty()).then(|| output.stderr.trim().to_owned()))
            .unwrap_or_else(|| {
                if code == 0 {
                    "success"
                } else {
                    "lpac command failed"
                }
                .into()
            });
        if code != 0 || !output.success {
            return Err(LpacError::Command(message));
        }
        Ok(LpacMutationResult {
            action: action.to_owned(),
            message,
            data: payload.get("data").cloned().unwrap_or(Value::Null),
        })
    }

    fn execute_process(&self, arguments: &[&str], timeout: Duration) -> LpacResult<ProcessOutput> {
        let arguments = arguments
            .iter()
            .map(|value| (*value).to_owned())
            .collect::<Vec<_>>();
        let mut environment = self.backend.environment();
        add_lpac_library_path(&self.program, &mut environment);
        self.executor
            .execute(&self.program, &arguments, &environment, timeout)
            .map_err(|error| {
                if error.contains("failed to start") {
                    LpacError::Unavailable(error)
                } else {
                    LpacError::Command(error)
                }
            })
    }
}

fn add_lpac_library_path(program: &Path, environment: &mut Vec<(String, String)>) {
    let Some(parent) = program.parent().filter(|path| !path.as_os_str().is_empty()) else {
        return;
    };
    let library = parent.join("lib");
    if !library.is_dir() {
        return;
    }
    let mut value = library.to_string_lossy().into_owned();
    if let Some(existing) = env::var_os("LD_LIBRARY_PATH") {
        value.push(':');
        value.push_str(&existing.to_string_lossy());
    }
    environment.push(("LD_LIBRARY_PATH".to_owned(), value));
}

fn validate_identifier(identifier: &str) -> LpacResult<&str> {
    let identifier = identifier.trim();
    if identifier.is_empty()
        || identifier.len() > 128
        || !identifier
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'-' | b'_'))
    {
        return Err(LpacError::Command("invalid eSIM Profile identifier".into()));
    }
    Ok(identifier)
}

fn parse_last_json(output: &str) -> LpacResult<Value> {
    output
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
        .or_else(|| serde_json::from_str::<Value>(output.trim()).ok())
        .ok_or_else(|| LpacError::InvalidResponse("no JSON object in stdout".into()))
}

fn response_data(value: &Value) -> &Value {
    let payload = value.get("payload").unwrap_or(value);
    payload.get("data").unwrap_or(payload)
}

fn string_value(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => {
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_owned())
        }
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn string_from(value: &Value, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| value.get(*name).and_then(string_value))
}

fn normalize_profile(value: &Value) -> EuiccProfile {
    let iccid = string_from(value, &["iccid", "ICCID", "id"])
        .unwrap_or_default()
        .chars()
        .filter(char::is_ascii_digit)
        .collect();
    EuiccProfile {
        iccid,
        name: string_from(
            value,
            &[
                "profileNickname",
                "profile_nickname",
                "nickname",
                "name",
                "profileName",
            ],
        )
        .unwrap_or_default(),
        provider: string_from(
            value,
            &[
                "serviceProviderName",
                "service_provider_name",
                "provider",
                "carrier",
                "operatorName",
            ],
        )
        .unwrap_or_default(),
        state: profile_state(value),
        profile_class: profile_class(value),
        isdp_aid: string_from(value, &["isdpAid", "isdp_aid", "aid"]),
        raw: value.clone(),
    }
}

fn profile_state(value: &Value) -> String {
    let value = ["state", "status", "profileState", "profile_state"]
        .iter()
        .find_map(|name| value.get(*name));
    match value {
        Some(Value::Number(value)) if value.as_i64() == Some(1) => "enabled".into(),
        Some(Value::Number(value)) if value.as_i64() == Some(0) => "disabled".into(),
        Some(Value::Bool(true)) => "enabled".into(),
        Some(Value::Bool(false)) => "disabled".into(),
        Some(value) => string_value(value)
            .map(|value| match value.to_ascii_lowercase().as_str() {
                "1" | "active" | "enabled" => "enabled".into(),
                "0" | "inactive" | "disabled" => "disabled".into(),
                _ => value.to_ascii_lowercase(),
            })
            .unwrap_or_else(|| "unknown".into()),
        None => "unknown".into(),
    }
}

fn profile_class(value: &Value) -> String {
    let value = ["class", "profile_class", "profileClass"]
        .iter()
        .find_map(|name| value.get(*name));
    match value {
        Some(Value::Number(value)) => match value.as_i64() {
            Some(0) => "test".into(),
            Some(1) => "provisioning".into(),
            Some(2) => "operational".into(),
            _ => value.to_string(),
        },
        Some(value) => string_value(value).unwrap_or_default(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Debug, Clone)]
    struct Invocation {
        arguments: Vec<String>,
        environment: Vec<(String, String)>,
    }

    struct MockExecutor {
        outputs: Mutex<Vec<ProcessOutput>>,
        invocations: Mutex<Vec<Invocation>>,
    }

    impl MockExecutor {
        fn new(outputs: Vec<ProcessOutput>) -> Self {
            Self {
                outputs: Mutex::new(outputs.into_iter().rev().collect()),
                invocations: Mutex::new(Vec::new()),
            }
        }
    }

    impl ProcessExecutor for MockExecutor {
        fn execute(
            &self,
            _program: &Path,
            arguments: &[String],
            environment: &[(String, String)],
            _timeout: Duration,
        ) -> Result<ProcessOutput, String> {
            self.invocations.lock().unwrap().push(Invocation {
                arguments: arguments.to_vec(),
                environment: environment.to_vec(),
            });
            self.outputs
                .lock()
                .unwrap()
                .pop()
                .ok_or_else(|| "missing mock output".into())
        }
    }

    fn output(json: &str) -> ProcessOutput {
        ProcessOutput {
            success: true,
            exit_code: Some(0),
            stdout: json.to_owned(),
            stderr: String::new(),
        }
    }

    fn service(
        backend: LpacApduBackend,
        outputs: Vec<ProcessOutput>,
    ) -> (LpacProfileService, Arc<MockExecutor>) {
        let executor = Arc::new(MockExecutor::new(outputs));
        (
            LpacProfileService::with_executor(
                "lpac-test",
                backend,
                ApduArbiter::new("device-1"),
                executor.clone(),
            ),
            executor,
        )
    }

    #[test]
    fn driver_probe_is_backend_specific() {
        let (service, executor) = service(
            LpacApduBackend::Mbim {
                device: "/dev/cdc-wdm2".into(),
                slot: 1,
            },
            vec![output(
                r#"{"type":"lpa","payload":{"code":0,"data":{"LPAC_APDU":["qmi","mbim"],"LPAC_HTTP":["curl"]}}}"#,
            )],
        );
        assert!(service.driver_available(Duration::from_secs(2)).unwrap());
        let invocations = executor.invocations.lock().unwrap();
        assert_eq!(invocations[0].arguments, ["driver", "list"]);
        assert!(invocations[0]
            .environment
            .contains(&("LPAC_APDU_MBIM_DEVICE".into(), "/dev/cdc-wdm2".into())));
        assert!(invocations[0]
            .environment
            .contains(&("LPAC_APDU_MBIM_SKIP_SLOT_MAPPING".into(), "true".into())));
    }

    #[test]
    fn profile_list_is_normalized_for_the_shared_panel() {
        let (service, _) = service(
            LpacApduBackend::Qmi {
                device: "/dev/cdc-wdm0".into(),
                slot: 1,
            },
            vec![output(
                r#"{"type":"lpa","payload":{"code":0,"data":[{"iccid":"8986001234567890123F","profileNickname":"联通","serviceProviderName":"China Unicom","profileState":1,"profileClass":2}]}}"#,
            )],
        );
        let profiles = service.profiles().unwrap();
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].iccid, "8986001234567890123");
        assert_eq!(profiles[0].name, "联通");
        assert_eq!(profiles[0].state, "enabled");
        assert_eq!(profiles[0].profile_class, "operational");
    }

    #[test]
    fn profile_mutations_use_explicit_arguments() {
        let (service, executor) = service(
            LpacApduBackend::AtCsim {
                device: "/dev/ttyUSB2".into(),
            },
            vec![
                output(r#"{"type":"lpa","payload":{"code":0,"message":"ok"}}"#),
                output(r#"{"type":"lpa","payload":{"code":0,"message":"ok"}}"#),
                output(r#"{"type":"lpa","payload":{"code":0,"message":"ok"}}"#),
            ],
        );
        service.enable_profile("8986001", true).unwrap();
        service.disable_profile("8986001", true).unwrap();
        service.delete_profile("8986001").unwrap();
        let invocations = executor.invocations.lock().unwrap();
        assert_eq!(
            invocations[0].arguments,
            ["profile", "enable", "8986001", "1"]
        );
        assert_eq!(
            invocations[1].arguments,
            ["profile", "disable", "8986001", "1"]
        );
        assert_eq!(invocations[2].arguments, ["profile", "delete", "8986001"]);
    }
}
