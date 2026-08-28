use std::{path::PathBuf, sync::Arc, time::Duration};

use crate::{
    decode_hex, ApduError, ApduResult, ApduTransport, ProcessExecutor, SystemProcessExecutor,
};

pub use crate::{
    ProcessExecutor as MbimCommandExecutor, ProcessOutput as MbimCommandOutput,
    SystemProcessExecutor as SystemMbimCommandExecutor,
};

pub const DEFAULT_MBIMCLI_PROGRAM: &str = "mbimcli";
const DEFAULT_CHANNEL_GROUP: u32 = 1;
const DEFAULT_SELECT_P2: u32 = 0x0C;
#[derive(Clone)]
pub struct MbimCliApduTransport {
    program: PathBuf,
    device: PathBuf,
    timeout: Duration,
    channel_group: u32,
    executor: Arc<dyn ProcessExecutor>,
}

impl MbimCliApduTransport {
    pub fn new(device: impl Into<PathBuf>, timeout: Duration) -> Self {
        Self::with_executor(
            DEFAULT_MBIMCLI_PROGRAM,
            device,
            timeout,
            Arc::new(SystemProcessExecutor),
        )
    }

    pub fn with_executor(
        program: impl Into<PathBuf>,
        device: impl Into<PathBuf>,
        timeout: Duration,
        executor: Arc<dyn ProcessExecutor>,
    ) -> Self {
        Self {
            program: program.into(),
            device: device.into(),
            timeout,
            channel_group: DEFAULT_CHANNEL_GROUP,
            executor,
        }
    }

    fn execute(&self, action: String) -> ApduResult<String> {
        let arguments = vec![
            "-d".to_owned(),
            self.device.to_string_lossy().into_owned(),
            "--device-open-proxy".to_owned(),
            action,
        ];
        let output = self
            .executor
            .execute(&self.program, &arguments, &[], self.timeout)
            .map_err(ApduError::Transport)?;
        if !output.success {
            let detail = if output.stderr.trim().is_empty() {
                output.stdout.trim()
            } else {
                output.stderr.trim()
            };
            return Err(ApduError::Transport(format!(
                "mbimcli exited with {}: {}",
                output
                    .exit_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "unknown status".to_owned()),
                detail
            )));
        }
        Ok(output.stdout)
    }
}

impl ApduTransport for MbimCliApduTransport {
    type Channel = u32;

    fn open_logical_channel(&self, aid: &[u8]) -> ApduResult<Self::Channel> {
        if aid.is_empty() {
            return Err(ApduError::Transport(
                "MBIM UICC application ID is empty".into(),
            ));
        }
        let request = format!(
            "--ms-set-uicc-open-channel=application-id={},selectp2arg={},channel-group={}",
            encode_colon_hex(aid),
            DEFAULT_SELECT_P2,
            self.channel_group
        );
        let output = self.execute(request)?;
        ensure_channel_status(&output)?;
        parse_decimal_field(&output, "channel")
    }

    fn transmit(&self, channel: Self::Channel, command: &[u8]) -> ApduResult<Vec<u8>> {
        if command.is_empty() {
            return Err(ApduError::Transport("MBIM UICC APDU is empty".into()));
        }
        let request = format!(
            "--ms-set-uicc-apdu=channel={channel},secure-message=none,classbyte-type=extended,command={}",
            encode_colon_hex(command)
        );
        let output = self.execute(request)?;
        let status = parse_status_word(&output)?;
        let response = parse_text_field_allow_empty(&output, "response")?;
        let mut response = decode_hex(&response.replace(':', ""))?;
        response.extend_from_slice(&status);
        Ok(response)
    }

    fn close_logical_channel(&self, channel: Self::Channel) -> ApduResult<()> {
        let request = format!(
            "--ms-set-uicc-close-channel=channel={channel},channel-group={}",
            self.channel_group
        );
        let output = self.execute(request)?;
        ensure_channel_status(&output)
    }
}

fn encode_colon_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn ensure_channel_status(output: &str) -> ApduResult<()> {
    let status = parse_decimal_field(output, "status")?;
    if status == 0 || status_bytes(status)? == [0x90, 0x00] {
        Ok(())
    } else {
        Err(ApduError::Transport(format!(
            "MBIM UICC operation returned status {status}"
        )))
    }
}

fn parse_status_word(output: &str) -> ApduResult<[u8; 2]> {
    let status = parse_decimal_field(output, "status")?;
    status_bytes(status)
}

fn status_bytes(status: u32) -> ApduResult<[u8; 2]> {
    if status > u32::from(u16::MAX) {
        return Err(ApduError::InvalidResponse(format!(
            "invalid MBIM UICC status word {status}"
        )));
    }
    Ok([status as u8, (status >> 8) as u8])
}

fn parse_decimal_field(output: &str, field: &str) -> ApduResult<u32> {
    let value = parse_text_field(output, field)?;
    value.parse::<u32>().map_err(|error| {
        ApduError::InvalidResponse(format!(
            "invalid MBIM UICC {field} value {value:?}: {error}"
        ))
    })
}

fn parse_text_field(output: &str, field: &str) -> ApduResult<String> {
    let value = parse_text_field_allow_empty(output, field)?;
    if value.is_empty() {
        Err(ApduError::InvalidResponse(format!(
            "empty MBIM UICC {field} field in mbimcli output"
        )))
    } else {
        Ok(value)
    }
}

fn parse_text_field_allow_empty(output: &str, field: &str) -> ApduResult<String> {
    output
        .lines()
        .filter_map(|line| line.trim().split_once(':'))
        .find_map(|(name, value)| {
            name.trim()
                .eq_ignore_ascii_case(field)
                .then(|| value.trim().to_owned())
        })
        .ok_or_else(|| {
            ApduError::InvalidResponse(format!("missing MBIM UICC {field} field in mbimcli output"))
        })
}

#[cfg(test)]
mod tests {
    use std::{path::Path, sync::Mutex};

    use super::*;
    use crate::ProcessOutput;

    struct MockExecutor {
        outputs: Mutex<Vec<MbimCommandOutput>>,
        arguments: Mutex<Vec<Vec<String>>>,
    }

    impl MockExecutor {
        fn new(outputs: Vec<MbimCommandOutput>) -> Self {
            Self {
                outputs: Mutex::new(outputs.into_iter().rev().collect()),
                arguments: Mutex::new(Vec::new()),
            }
        }
    }

    impl ProcessExecutor for MockExecutor {
        fn execute(
            &self,
            _program: &Path,
            arguments: &[String],
            _environment: &[(String, String)],
            _timeout: Duration,
        ) -> Result<ProcessOutput, String> {
            self.arguments.lock().unwrap().push(arguments.to_vec());
            self.outputs
                .lock()
                .unwrap()
                .pop()
                .ok_or_else(|| "missing mock mbimcli output".to_owned())
        }
    }

    fn successful(stdout: &str) -> MbimCommandOutput {
        MbimCommandOutput {
            success: true,
            exit_code: Some(0),
            stdout: stdout.to_owned(),
            stderr: String::new(),
        }
    }

    #[test]
    fn mbim_transport_uses_separate_arguments_and_parses_responses() {
        let executor = Arc::new(MockExecutor::new(vec![
            successful(
                "Succesfully retrieved open channel info:\n\tstatus: 144\n\tchannel: 7\n\tresponse: \n",
            ),
            successful(
                "Succesfully retrieved UICC APDU response:\n\tstatus: 144\n\tresponse: BF:3E:00\n",
            ),
            successful("Succesfully retrieved close channel info:\n\tstatus: 0\n"),
        ]));
        let transport = MbimCliApduTransport::with_executor(
            "mbimcli-test",
            "/dev/cdc-wdm0",
            Duration::from_secs(2),
            executor.clone(),
        );

        let aid = [0xA0, 0x00, 0x00, 0x05, 0x59];
        let channel = transport.open_logical_channel(&aid).unwrap();
        assert_eq!(channel, 7);
        assert_eq!(
            transport
                .transmit(channel, &[0x80, 0xCA, 0xBF, 0x3E, 0x00])
                .unwrap(),
            vec![0xBF, 0x3E, 0x00, 0x90, 0x00]
        );
        transport.close_logical_channel(channel).unwrap();

        let arguments = executor.arguments.lock().unwrap();
        assert_eq!(
            arguments[0][..3],
            ["-d", "/dev/cdc-wdm0", "--device-open-proxy"]
        );
        assert_eq!(
            arguments[0][3],
            "--ms-set-uicc-open-channel=application-id=A0:00:00:05:59,selectp2arg=12,channel-group=1"
        );
        assert_eq!(
            arguments[1][3],
            "--ms-set-uicc-apdu=channel=7,secure-message=none,classbyte-type=extended,command=80:CA:BF:3E:00"
        );
        assert_eq!(
            arguments[2][3],
            "--ms-set-uicc-close-channel=channel=7,channel-group=1"
        );
    }

    #[test]
    fn mbim_transport_rejects_nonzero_service_status() {
        let executor = Arc::new(MockExecutor::new(vec![successful(
            "Succesfully retrieved open channel info:\nstatus: 5\nchannel: 1\nresponse:\n",
        )]));
        let transport = MbimCliApduTransport::with_executor(
            "mbimcli-test",
            "/dev/cdc-wdm0",
            Duration::from_secs(2),
            executor,
        );
        assert!(matches!(
            transport.open_logical_channel(&[0xA0]),
            Err(ApduError::Transport(message)) if message.contains("status 5")
        ));
    }

    #[test]
    fn mbim_transport_reports_process_failure() {
        let executor = Arc::new(MockExecutor::new(vec![MbimCommandOutput {
            success: false,
            exit_code: Some(1),
            stdout: String::new(),
            stderr: "error: operation failed".into(),
        }]));
        let transport = MbimCliApduTransport::with_executor(
            "mbimcli-test",
            "/dev/cdc-wdm0",
            Duration::from_secs(2),
            executor,
        );
        assert!(matches!(
            transport.open_logical_channel(&[0xA0]),
            Err(ApduError::Transport(message)) if message.contains("operation failed")
        ));
    }
}
