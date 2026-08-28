use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io,
    path::PathBuf,
    sync::{Arc, Mutex, TryLockError},
};

#[cfg(unix)]
use std::os::fd::AsRawFd;

use serde::{Deserialize, Serialize};

const ISD_R_AID: &[u8] = &[
    0xA0, 0x00, 0x00, 0x05, 0x59, 0x10, 0x10, 0xFF, 0xFF, 0xFF, 0xFF, 0x89, 0x00, 0x00, 0x01, 0x00,
];
const GET_EUICC_INFO_1: &[u8] = &[0x80, 0xCA, 0xBF, 0x3E, 0x00];
const ISO_GET_RESPONSE_INS: u8 = 0xC0;
const MAX_APDU_EXCHANGES: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApduOperation {
    ReadOnly,
    SimAuthentication,
    ProfileMutation,
}

#[derive(Debug, thiserror::Error)]
pub enum ApduError {
    #[error("APDU channel for device {0} is busy")]
    Busy(String),
    #[error("APDU transport failed: {0}")]
    Transport(String),
    #[error("APDU response is invalid: {0}")]
    InvalidResponse(String),
    #[error("eUICC application is unavailable")]
    EuiccUnavailable,
}

pub type ApduResult<T> = Result<T, ApduError>;

#[derive(Clone)]
pub struct ApduArbiter {
    device_id: String,
    lock: Arc<Mutex<()>>,
}

impl ApduArbiter {
    pub fn new(device_id: impl Into<String>) -> Self {
        Self {
            device_id: device_id.into(),
            lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn execute<T, E>(
        &self,
        _operation: ApduOperation,
        task: impl FnOnce() -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<ApduError>,
    {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| ApduError::Transport("APDU arbiter lock is poisoned".into()))
            .map_err(E::from)?;
        let _process_guard = self.process_lock(false).map_err(E::from)?;
        task()
    }

    pub fn try_execute<T, E>(
        &self,
        _operation: ApduOperation,
        task: impl FnOnce() -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<ApduError>,
    {
        let _guard = match self.lock.try_lock() {
            Ok(guard) => guard,
            Err(TryLockError::WouldBlock) => {
                return Err(E::from(ApduError::Busy(self.device_id.clone())))
            }
            Err(TryLockError::Poisoned(_)) => {
                return Err(E::from(ApduError::Transport(
                    "APDU arbiter lock is poisoned".into(),
                )))
            }
        };
        let _process_guard = self.process_lock(true).map_err(E::from)?;
        task()
    }

    fn process_lock(&self, nonblocking: bool) -> ApduResult<Option<ProcessApduGuard>> {
        let Some(root) = std::env::var_os("SIMADMIN_APDU_LOCK_DIR") else {
            return Ok(None);
        };
        let root = PathBuf::from(root);
        fs::create_dir_all(&root).map_err(|error| {
            ApduError::Transport(format!("cannot create APDU lock directory: {error}"))
        })?;
        let filename = self
            .device_id
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                    character
                } else {
                    '_'
                }
            })
            .collect::<String>();
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join(format!("{filename}.lock")))
            .map_err(|error| ApduError::Transport(format!("cannot open APDU lock: {error}")))?;
        lock_file(&file, nonblocking).map_err(|error| {
            if nonblocking
                && matches!(
                    error.raw_os_error(),
                    Some(code) if code == libc::EAGAIN || code == libc::EWOULDBLOCK
                )
            {
                ApduError::Busy(self.device_id.clone())
            } else {
                ApduError::Transport(format!("cannot acquire APDU lock: {error}"))
            }
        })?;
        Ok(Some(ProcessApduGuard { file }))
    }
}

struct ProcessApduGuard {
    file: File,
}

impl Drop for ProcessApduGuard {
    fn drop(&mut self) {
        let _ = unlock_file(&self.file);
    }
}

#[cfg(unix)]
fn lock_file(file: &File, nonblocking: bool) -> io::Result<()> {
    let operation = libc::LOCK_EX | if nonblocking { libc::LOCK_NB } else { 0 };
    let result = unsafe { libc::flock(file.as_raw_fd(), operation) };
    (result == 0)
        .then_some(())
        .ok_or_else(io::Error::last_os_error)
}

#[cfg(unix)]
fn unlock_file(file: &File) -> io::Result<()> {
    let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) };
    (result == 0)
        .then_some(())
        .ok_or_else(io::Error::last_os_error)
}

#[cfg(not(unix))]
fn lock_file(_file: &File, _nonblocking: bool) -> io::Result<()> {
    Ok(())
}

#[cfg(not(unix))]
fn unlock_file(_file: &File) -> io::Result<()> {
    Ok(())
}

#[derive(Clone, Default)]
pub struct ApduArbiterRegistry {
    arbiters: Arc<Mutex<HashMap<String, ApduArbiter>>>,
}

impl ApduArbiterRegistry {
    pub fn for_device(&self, device_id: impl Into<String>) -> ApduArbiter {
        let device_id = device_id.into();
        let mut arbiters = self.arbiters.lock().unwrap();
        arbiters
            .entry(device_id.clone())
            .or_insert_with(|| ApduArbiter::new(device_id))
            .clone()
    }
}

pub trait ApduTransport {
    type Channel: Copy;

    fn open_logical_channel(&self, aid: &[u8]) -> ApduResult<Self::Channel>;
    fn transmit(&self, channel: Self::Channel, command: &[u8]) -> ApduResult<Vec<u8>>;
    fn close_logical_channel(&self, channel: Self::Channel) -> ApduResult<()>;
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EuiccProbeResult {
    pub detected: bool,
    pub eid: Option<String>,
    pub transport: String,
}

pub struct EuiccService<T> {
    transport: T,
    arbiter: ApduArbiter,
    transport_name: String,
}

impl<T: ApduTransport> EuiccService<T> {
    pub fn new(
        device_id: impl Into<String>,
        transport_name: impl Into<String>,
        transport: T,
    ) -> Self {
        let device_id = device_id.into();
        Self::with_arbiter(transport_name, transport, ApduArbiter::new(device_id))
    }

    pub fn with_arbiter(
        transport_name: impl Into<String>,
        transport: T,
        arbiter: ApduArbiter,
    ) -> Self {
        Self {
            transport,
            arbiter,
            transport_name: transport_name.into(),
        }
    }

    pub fn probe(&self) -> ApduResult<EuiccProbeResult> {
        self.arbiter.execute(ApduOperation::ReadOnly, || {
            let channel = self
                .transport
                .open_logical_channel(ISD_R_AID)
                .map_err(|_| ApduError::EuiccUnavailable)?;
            let result = self
                .exchange(channel, GET_EUICC_INFO_1)
                .and_then(|response| {
                    ensure_success(&response)?;
                    Ok(EuiccProbeResult {
                        detected: true,
                        eid: extract_eid(&response),
                        transport: self.transport_name.clone(),
                    })
                });
            let close = self.transport.close_logical_channel(channel);
            match (result, close) {
                (Ok(value), Ok(())) => Ok(value),
                (Err(error), _) => Err(error),
                (Ok(_), Err(error)) => Err(error),
            }
        })
    }

    fn exchange(&self, channel: T::Channel, command: &[u8]) -> ApduResult<Vec<u8>> {
        let mut current = command.to_vec();
        let mut body = Vec::new();
        for _ in 0..MAX_APDU_EXCHANGES {
            let response = self.transport.transmit(channel, &current)?;
            let (chunk, sw1, sw2) = split_response(&response)?;
            match sw1 {
                0x61 => {
                    body.extend_from_slice(chunk);
                    current = vec![0x00, ISO_GET_RESPONSE_INS, 0x00, 0x00, sw2];
                }
                0x6C => {
                    let Some(le) = current.last_mut() else {
                        return Err(ApduError::InvalidResponse(
                            "cannot correct APDU length without Le".into(),
                        ));
                    };
                    *le = sw2;
                }
                _ => {
                    body.extend_from_slice(chunk);
                    body.extend_from_slice(&[sw1, sw2]);
                    return Ok(body);
                }
            }
        }
        Err(ApduError::InvalidResponse(
            "APDU continuation limit exceeded".into(),
        ))
    }
}

pub fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02X}")).collect()
}

pub fn decode_hex(value: &str) -> ApduResult<Vec<u8>> {
    let value = value.trim();
    if !value.len().is_multiple_of(2) || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ApduError::InvalidResponse(
            "invalid hexadecimal APDU".into(),
        ));
    }
    (0..value.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16)
                .map_err(|error| ApduError::InvalidResponse(error.to_string()))
        })
        .collect()
}

fn ensure_success(response: &[u8]) -> ApduResult<()> {
    let (_, sw1, sw2) = split_response(response)?;
    if (sw1, sw2) == (0x90, 0x00) {
        Ok(())
    } else {
        Err(ApduError::InvalidResponse(format!(
            "status word {:02X}{:02X}",
            sw1, sw2
        )))
    }
}

fn split_response(response: &[u8]) -> ApduResult<(&[u8], u8, u8)> {
    let split = response
        .len()
        .checked_sub(2)
        .ok_or_else(|| ApduError::InvalidResponse("missing APDU status word".into()))?;
    Ok((&response[..split], response[split], response[split + 1]))
}

fn extract_eid(response: &[u8]) -> Option<String> {
    response.windows(18).find_map(|window| {
        if window[0] != 0x5A || window[1] != 0x10 {
            return None;
        }
        let eid = window[2..18]
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<String>();
        (eid.len() == 32 && eid.bytes().all(|byte| byte.is_ascii_digit())).then_some(eid)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct MockTransport {
        closed: Arc<AtomicBool>,
    }

    impl ApduTransport for MockTransport {
        type Channel = u8;

        fn open_logical_channel(&self, aid: &[u8]) -> ApduResult<Self::Channel> {
            assert_eq!(aid, ISD_R_AID);
            Ok(1)
        }

        fn transmit(&self, channel: Self::Channel, command: &[u8]) -> ApduResult<Vec<u8>> {
            assert_eq!(channel, 1);
            assert_eq!(command, GET_EUICC_INFO_1);
            decode_hex("BF3E125A10890490320000000000000000000012349000")
        }

        fn close_logical_channel(&self, channel: Self::Channel) -> ApduResult<()> {
            assert_eq!(channel, 1);
            self.closed.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    #[test]
    fn euicc_probe_reads_eid_and_always_closes_channel() {
        let closed = Arc::new(AtomicBool::new(false));
        let service = EuiccService::new(
            "device-1",
            "mock",
            MockTransport {
                closed: closed.clone(),
            },
        );
        let result = service.probe().unwrap();
        assert!(result.detected);
        assert_eq!(
            result.eid.as_deref(),
            Some("89049032000000000000000000001234")
        );
        assert!(closed.load(Ordering::SeqCst));
    }

    #[test]
    fn arbiter_rejects_parallel_try_execute() {
        let arbiter = ApduArbiter::new("device-1");
        let nested = arbiter.clone();
        let result = arbiter.execute(ApduOperation::ProfileMutation, || {
            nested.try_execute(ApduOperation::SimAuthentication, || Ok(()))
        });
        assert!(matches!(result, Err(ApduError::Busy(device)) if device == "device-1"));
    }

    struct ContinuationTransport {
        responses: Mutex<VecDeque<Vec<u8>>>,
        commands: Mutex<Vec<Vec<u8>>>,
    }

    impl ApduTransport for ContinuationTransport {
        type Channel = u8;

        fn open_logical_channel(&self, _aid: &[u8]) -> ApduResult<Self::Channel> {
            Ok(1)
        }

        fn transmit(&self, _channel: Self::Channel, command: &[u8]) -> ApduResult<Vec<u8>> {
            self.commands.lock().unwrap().push(command.to_vec());
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .ok_or_else(|| ApduError::Transport("missing mock response".into()))
        }

        fn close_logical_channel(&self, _channel: Self::Channel) -> ApduResult<()> {
            Ok(())
        }
    }

    #[test]
    fn euicc_probe_follows_get_response_and_wrong_length_status() {
        let transport = ContinuationTransport {
            responses: Mutex::new(VecDeque::from([
                vec![0x6C, 0x10],
                vec![0xBF, 0x3E, 0x61, 0x12],
                decode_hex("5A10890490320000000000000000000012349000").unwrap(),
            ])),
            commands: Mutex::new(Vec::new()),
        };
        let service = EuiccService::new("device-1", "mock", transport);
        let result = service.probe().unwrap();
        assert_eq!(
            result.eid.as_deref(),
            Some("89049032000000000000000000001234")
        );
        let commands = service.transport.commands.lock().unwrap();
        assert_eq!(commands[1].last(), Some(&0x10));
        assert_eq!(commands[2], vec![0x00, 0xC0, 0x00, 0x00, 0x12]);
    }
}
