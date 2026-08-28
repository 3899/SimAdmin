use std::{fmt, io, time::Duration};

use crate::{ApduError, ApduResult, ApduTransport};

#[cfg(unix)]
use std::sync::Mutex;

#[cfg(unix)]
use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
};

const QMUX_CTL_SERVICE: u8 = 0x00;
const QMUX_UIM_SERVICE: u8 = 0x0b;
const QMI_CTL_ALLOCATE_CID: u16 = 0x0022;
const QMI_CTL_RELEASE_CID: u16 = 0x0023;
const QMI_PROXY_OPEN: u16 = 0xff00;
const QMI_UIM_SEND_APDU: u16 = 0x003b;
const QMI_UIM_OPEN_LOGICAL_CHANNEL: u16 = 0x0042;
const QMI_UIM_LOGICAL_CHANNEL: u16 = 0x003f;

const TLV_RESULT: u8 = 0x02;
const TLV_PROXY_DEVICE_PATH: u8 = 0x01;
const TLV_CTL_SERVICE: u8 = 0x01;
const TLV_CTL_ALLOCATION_INFO: u8 = 0x01;
const TLV_UIM_SLOT: u8 = 0x01;
const TLV_UIM_APDU: u8 = 0x02;
const TLV_UIM_CHANNEL_ID: u8 = 0x10;
const TLV_UIM_PROCEDURE_BYTES: u8 = 0x11;
const TLV_UIM_OPEN_AID: u8 = 0x10;
const TLV_UIM_OPEN_FCI: u8 = 0x11;
const TLV_UIM_APDU_RESPONSE: u8 = 0x10;

pub const DEFAULT_QMI_PROXY_SOCKET: &str = "@qmi-proxy";

pub const USIM_AID_PREFIX: &[u8] = &[0xa0, 0x00, 0x00, 0x00, 0x87, 0x10, 0x02];
pub const USIM_AUTHENTICATE_CLA: u8 = 0x00;
pub const USIM_AUTHENTICATE_INS: u8 = 0x88;
pub const USIM_AUTHENTICATE_P2_3G: u8 = 0x81;
pub const ISO_GET_RESPONSE_INS: u8 = 0xc0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QmiMessage {
    pub service: u8,
    pub client_id: u8,
    pub transaction_id: u16,
    pub message_id: u16,
    pub tlvs: Vec<QmiTlv>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QmiTlv {
    pub tlv_type: u8,
    pub value: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QmiResult {
    pub success: bool,
    pub error_code: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicalChannelOpened {
    pub channel_id: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UimApduResponse {
    pub data: Vec<u8>,
    pub sw1: u8,
    pub sw2: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsimAkaApduResult {
    pub res: Vec<u8>,
    pub ck: Vec<u8>,
    pub ik: Vec<u8>,
    pub auts: Option<Vec<u8>>,
}

#[derive(Debug)]
pub enum QmiUimError {
    Io(io::Error),
    FrameTooShort,
    InvalidFrame,
    MessageTooLarge,
    MissingTlv(&'static str),
    ResultFailure(u16),
    InvalidApduResponse,
    InvalidAkaResponse,
}

impl fmt::Display for QmiUimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::FrameTooShort => write!(f, "QMI frame too short"),
            Self::InvalidFrame => write!(f, "invalid QMI frame"),
            Self::MessageTooLarge => write!(f, "QMI message too large"),
            Self::MissingTlv(name) => write!(f, "QMI response missing {name} TLV"),
            Self::ResultFailure(code) => write!(f, "QMI operation failed with code {code}"),
            Self::InvalidApduResponse => write!(f, "invalid UIM APDU response"),
            Self::InvalidAkaResponse => write!(f, "invalid USIM AKA response"),
        }
    }
}

impl std::error::Error for QmiUimError {}

impl From<io::Error> for QmiUimError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

pub struct QmiProxyApduTransport {
    #[cfg(unix)]
    proxy_socket: String,
    #[cfg(unix)]
    device_path: String,
    #[cfg(unix)]
    slot: u8,
    #[cfg(unix)]
    timeout: Duration,
    #[cfg(unix)]
    session: Mutex<Option<QmiProxyChannelSession>>,
}

#[cfg(unix)]
struct QmiProxyChannelSession {
    connection: QmiProxyConnection,
    client_id: u8,
    channel_id: u8,
}

impl QmiProxyApduTransport {
    pub fn new(
        proxy_socket: impl Into<String>,
        device_path: impl Into<String>,
        slot: u8,
        timeout: Duration,
    ) -> Self {
        #[cfg(not(unix))]
        let _ = (proxy_socket.into(), device_path.into(), slot, timeout);
        Self {
            #[cfg(unix)]
            proxy_socket: proxy_socket.into(),
            #[cfg(unix)]
            device_path: device_path.into(),
            #[cfg(unix)]
            slot,
            #[cfg(unix)]
            timeout,
            #[cfg(unix)]
            session: Mutex::new(None),
        }
    }

    pub fn with_defaults(device_path: impl Into<String>) -> Self {
        Self::new(
            DEFAULT_QMI_PROXY_SOCKET,
            device_path,
            1,
            Duration::from_secs(5),
        )
    }
}

impl ApduTransport for QmiProxyApduTransport {
    type Channel = u8;

    fn open_logical_channel(&self, aid: &[u8]) -> ApduResult<Self::Channel> {
        #[cfg(not(unix))]
        {
            let _ = aid;
            Err(ApduError::Transport(
                "QMI UIM transport requires Linux".into(),
            ))
        }

        #[cfg(unix)]
        {
            let mut session = self
                .session
                .lock()
                .map_err(|_| ApduError::Transport("QMI UIM session lock is poisoned".into()))?;
            if session.is_some() {
                return Err(ApduError::Busy(self.device_path.clone()));
            }
            let mut connection = QmiProxyConnection::connect(&self.proxy_socket, self.timeout)
                .map_err(qmi_transport_error)?;
            connection
                .proxy_open(&self.device_path)
                .map_err(qmi_transport_error)?;
            let client_id = connection.allocate_uim_cid().map_err(qmi_transport_error)?;
            let opened = match connection.open_logical_channel(client_id, self.slot, aid) {
                Ok(opened) => opened,
                Err(error) => {
                    let _ = connection.release_uim_cid(client_id);
                    return Err(qmi_transport_error(error));
                }
            };
            let channel_id = opened.channel_id;
            *session = Some(QmiProxyChannelSession {
                connection,
                client_id,
                channel_id,
            });
            Ok(channel_id)
        }
    }

    fn transmit(&self, channel: Self::Channel, command: &[u8]) -> ApduResult<Vec<u8>> {
        #[cfg(not(unix))]
        {
            let _ = (channel, command);
            Err(ApduError::Transport(
                "QMI UIM transport requires Linux".into(),
            ))
        }

        #[cfg(unix)]
        {
            let mut session = self
                .session
                .lock()
                .map_err(|_| ApduError::Transport("QMI UIM session lock is poisoned".into()))?;
            let session = session
                .as_mut()
                .filter(|session| session.channel_id == channel)
                .ok_or_else(|| ApduError::Transport("QMI UIM channel is not open".into()))?;
            let response = session
                .connection
                .send_apdu(session.client_id, self.slot, channel, command)
                .map_err(qmi_transport_error)?;
            let mut value = response.data;
            value.extend_from_slice(&[response.sw1, response.sw2]);
            Ok(value)
        }
    }

    fn close_logical_channel(&self, channel: Self::Channel) -> ApduResult<()> {
        #[cfg(not(unix))]
        {
            let _ = channel;
            Err(ApduError::Transport(
                "QMI UIM transport requires Linux".into(),
            ))
        }

        #[cfg(unix)]
        {
            let mut guard = self
                .session
                .lock()
                .map_err(|_| ApduError::Transport("QMI UIM session lock is poisoned".into()))?;
            let mut session = guard
                .take()
                .filter(|session| session.channel_id == channel)
                .ok_or_else(|| ApduError::Transport("QMI UIM channel is not open".into()))?;
            let close = session.connection.close_logical_channel(
                session.client_id,
                self.slot,
                session.channel_id,
            );
            let release = session.connection.release_uim_cid(session.client_id);
            close.map_err(qmi_transport_error)?;
            release.map_err(qmi_transport_error)
        }
    }
}

#[cfg(unix)]
fn qmi_transport_error(error: QmiUimError) -> ApduError {
    ApduError::Transport(error.to_string())
}

pub fn build_proxy_open_frame(path: &str, transaction_id: u16) -> Result<Vec<u8>, QmiUimError> {
    encode_qmi_message(&QmiMessage {
        service: QMUX_CTL_SERVICE,
        client_id: 0,
        transaction_id,
        message_id: QMI_PROXY_OPEN,
        tlvs: vec![tlv(TLV_PROXY_DEVICE_PATH, path.as_bytes().to_vec())],
    })
}

pub fn build_allocate_uim_cid_frame(transaction_id: u16) -> Result<Vec<u8>, QmiUimError> {
    encode_qmi_message(&QmiMessage {
        service: QMUX_CTL_SERVICE,
        client_id: 0,
        transaction_id,
        message_id: QMI_CTL_ALLOCATE_CID,
        tlvs: vec![tlv(TLV_CTL_SERVICE, vec![QMUX_UIM_SERVICE])],
    })
}

pub fn build_release_uim_cid_frame(
    client_id: u8,
    transaction_id: u16,
) -> Result<Vec<u8>, QmiUimError> {
    encode_qmi_message(&QmiMessage {
        service: QMUX_CTL_SERVICE,
        client_id: 0,
        transaction_id,
        message_id: QMI_CTL_RELEASE_CID,
        tlvs: vec![tlv(TLV_CTL_SERVICE, vec![QMUX_UIM_SERVICE, client_id])],
    })
}

pub fn parse_allocated_cid(message: &QmiMessage) -> Result<u8, QmiUimError> {
    ensure_success(message)?;
    let value = find_tlv(message, TLV_CTL_ALLOCATION_INFO)
        .ok_or(QmiUimError::MissingTlv("allocation_info"))?;
    if value.len() < 2 || value[0] != QMUX_UIM_SERVICE {
        return Err(QmiUimError::InvalidFrame);
    }
    Ok(value[1])
}

pub fn build_open_logical_channel_frame(
    client_id: u8,
    transaction_id: u16,
    slot: u8,
    aid: &[u8],
) -> Result<Vec<u8>, QmiUimError> {
    let mut aid_value = Vec::with_capacity(1 + aid.len());
    aid_value.push(aid.len() as u8);
    aid_value.extend_from_slice(aid);
    encode_qmi_message(&QmiMessage {
        service: QMUX_UIM_SERVICE,
        client_id,
        transaction_id,
        message_id: QMI_UIM_OPEN_LOGICAL_CHANNEL,
        tlvs: vec![
            tlv(TLV_UIM_SLOT, vec![slot]),
            tlv(TLV_UIM_OPEN_AID, aid_value),
            tlv(TLV_UIM_OPEN_FCI, vec![0x00]),
        ],
    })
}

pub fn build_close_logical_channel_frame(
    client_id: u8,
    transaction_id: u16,
    slot: u8,
    channel_id: u8,
) -> Result<Vec<u8>, QmiUimError> {
    encode_qmi_message(&QmiMessage {
        service: QMUX_UIM_SERVICE,
        client_id,
        transaction_id,
        message_id: QMI_UIM_LOGICAL_CHANNEL,
        tlvs: vec![
            tlv(TLV_UIM_SLOT, vec![slot]),
            tlv(TLV_UIM_CHANNEL_ID, vec![channel_id]),
            tlv(0x13, vec![0x01]),
        ],
    })
}

pub fn parse_open_logical_channel(
    message: &QmiMessage,
) -> Result<LogicalChannelOpened, QmiUimError> {
    ensure_success(message)?;
    let value =
        find_tlv(message, TLV_UIM_CHANNEL_ID).ok_or(QmiUimError::MissingTlv("channel_id"))?;
    let channel_id = *value.first().ok_or(QmiUimError::InvalidFrame)?;
    Ok(LogicalChannelOpened { channel_id })
}

pub fn build_send_apdu_frame(
    client_id: u8,
    transaction_id: u16,
    slot: u8,
    channel_id: u8,
    apdu: &[u8],
) -> Result<Vec<u8>, QmiUimError> {
    let mut apdu_value = Vec::with_capacity(2 + apdu.len());
    apdu_value.extend_from_slice(&(apdu.len() as u16).to_le_bytes());
    apdu_value.extend_from_slice(apdu);
    encode_qmi_message(&QmiMessage {
        service: QMUX_UIM_SERVICE,
        client_id,
        transaction_id,
        message_id: QMI_UIM_SEND_APDU,
        tlvs: vec![
            tlv(TLV_UIM_SLOT, vec![slot]),
            tlv(TLV_UIM_APDU, apdu_value),
            tlv(TLV_UIM_CHANNEL_ID, vec![channel_id]),
            tlv(TLV_UIM_PROCEDURE_BYTES, vec![0x00]),
        ],
    })
}

pub fn build_usim_authenticate_apdu(rand: &[u8], autn: &[u8]) -> Result<Vec<u8>, QmiUimError> {
    if rand.len() > u8::MAX as usize || autn.len() > u8::MAX as usize {
        return Err(QmiUimError::MessageTooLarge);
    }
    let mut data = Vec::with_capacity(2 + rand.len() + autn.len());
    data.push(rand.len() as u8);
    data.extend_from_slice(rand);
    data.push(autn.len() as u8);
    data.extend_from_slice(autn);
    if data.len() > u8::MAX as usize {
        return Err(QmiUimError::MessageTooLarge);
    }
    Ok(vec![
        USIM_AUTHENTICATE_CLA,
        USIM_AUTHENTICATE_INS,
        0x00,
        USIM_AUTHENTICATE_P2_3G,
        data.len() as u8,
    ]
    .into_iter()
    .chain(data)
    .chain([0x00])
    .collect())
}

pub fn build_get_response_apdu(length: u8) -> Vec<u8> {
    vec![
        USIM_AUTHENTICATE_CLA,
        ISO_GET_RESPONSE_INS,
        0x00,
        0x00,
        length,
    ]
}

pub fn parse_send_apdu_response(message: &QmiMessage) -> Result<UimApduResponse, QmiUimError> {
    ensure_success(message)?;
    let value =
        find_tlv(message, TLV_UIM_APDU_RESPONSE).ok_or(QmiUimError::MissingTlv("apdu_response"))?;
    if value.len() < 4 {
        return Err(QmiUimError::InvalidApduResponse);
    }
    let len = u16::from_le_bytes([value[0], value[1]]) as usize;
    if value.len() < 2 + len || len < 2 {
        return Err(QmiUimError::InvalidApduResponse);
    }
    let apdu = &value[2..2 + len];
    let (body, status) = apdu.split_at(apdu.len() - 2);
    Ok(UimApduResponse {
        data: body.to_vec(),
        sw1: status[0],
        sw2: status[1],
    })
}

pub fn parse_usim_authenticate_response(
    response: &UimApduResponse,
) -> Result<UsimAkaApduResult, QmiUimError> {
    if response.sw1 != 0x90 || response.sw2 != 0x00 || response.data.is_empty() {
        return Err(QmiUimError::InvalidAkaResponse);
    }
    let data = unwrap_authenticate_response_data(&response.data)?;
    match data[0] {
        0xdb => parse_successful_auth_response(&data[1..]),
        0xdc => {
            let (auts, rest) = take_lv(&data[1..])?;
            if !rest.is_empty() {
                return Err(QmiUimError::InvalidAkaResponse);
            }
            Ok(UsimAkaApduResult {
                res: Vec::new(),
                ck: Vec::new(),
                ik: Vec::new(),
                auts: Some(auts.to_vec()),
            })
        }
        _ => Err(QmiUimError::InvalidAkaResponse),
    }
}

pub fn parse_usim_authenticate_response_reason(
    response: &UimApduResponse,
) -> Result<UsimAkaApduResult, &'static str> {
    parse_usim_authenticate_response(response)
        .map_err(|_| classify_usim_authenticate_response(response))
}

pub fn classify_usim_authenticate_response(response: &UimApduResponse) -> &'static str {
    match (response.sw1, response.sw2) {
        (0x90, 0x00) => match response.data.first().copied() {
            Some(0xdb) => "sim_auth_aka_success_parse_failed",
            Some(0xdc) => "sim_auth_aka_sync_failure_parse_failed",
            Some(_) => "sim_auth_aka_response_unknown_tag",
            None => "sim_auth_aka_response_empty",
        },
        (0x61, _) => "sim_auth_apdu_more_data_unhandled",
        (0x6c, _) => "sim_auth_apdu_wrong_length_unhandled",
        (0x67, 0x00) => "sim_auth_apdu_wrong_length",
        (0x69, 0x82) | (0x69, 0x85) => "sim_auth_apdu_security_status",
        (0x6a, 0x80) | (0x6a, 0x86) => "sim_auth_apdu_parameter_rejected",
        (0x6d, 0x00) => "sim_auth_apdu_instruction_not_supported",
        (0x6e, 0x00) => "sim_auth_apdu_class_not_supported",
        _ => "sim_auth_aka_response_parse_failed",
    }
}

pub fn execute_usim_authenticate_via_proxy(
    proxy_socket: &str,
    device_path: &str,
    slot: u8,
    aid: &[u8],
    rand: &[u8],
    autn: &[u8],
    timeout: Duration,
) -> Result<UsimAkaApduResult, QmiUimError> {
    #[cfg(not(unix))]
    {
        let _ = (proxy_socket, device_path, slot, aid, rand, autn, timeout);
        Err(QmiUimError::InvalidFrame)
    }

    #[cfg(unix)]
    {
        let mut conn = QmiProxyConnection::connect(proxy_socket, timeout)?;
        conn.proxy_open(device_path)?;
        let client_id = conn.allocate_uim_cid()?;
        let channel = conn.open_logical_channel(client_id, slot, aid)?;
        let apdu = build_usim_authenticate_apdu(rand, autn)?;
        let response = conn.send_apdu(client_id, slot, channel.channel_id, &apdu);
        let _ = conn.close_logical_channel(client_id, slot, channel.channel_id);
        let _ = conn.release_uim_cid(client_id);
        parse_usim_authenticate_response(&response?)
    }
}

pub fn execute_usim_authenticate_via_proxy_reason(
    proxy_socket: &str,
    device_path: &str,
    slot: u8,
    aid: &[u8],
    rand: &[u8],
    autn: &[u8],
    timeout: Duration,
) -> Result<UsimAkaApduResult, &'static str> {
    #[cfg(not(unix))]
    {
        let _ = (proxy_socket, device_path, slot, aid, rand, autn, timeout);
        Err("sim_auth_platform_unsupported")
    }

    #[cfg(unix)]
    {
        let mut conn = QmiProxyConnection::connect(proxy_socket, timeout)
            .map_err(|_| "sim_auth_proxy_connect_failed")?;
        conn.proxy_open(device_path)
            .map_err(|_| "sim_auth_proxy_open_failed")?;
        let client_id = conn
            .allocate_uim_cid()
            .map_err(|_| "sim_auth_uim_client_failed")?;
        let channel = match conn.open_logical_channel(client_id, slot, aid) {
            Ok(channel) => channel,
            Err(_) => {
                let _ = conn.release_uim_cid(client_id);
                return Err("sim_auth_logical_channel_failed");
            }
        };
        let apdu = match build_usim_authenticate_apdu(rand, autn) {
            Ok(apdu) => apdu,
            Err(_) => {
                let _ = conn.close_logical_channel(client_id, slot, channel.channel_id);
                let _ = conn.release_uim_cid(client_id);
                return Err("sim_auth_apdu_build_failed");
            }
        };
        let mut response = conn.send_apdu(client_id, slot, channel.channel_id, &apdu);
        if matches!(response.as_ref().map(|r| r.sw1), Ok(0x61)) {
            let len = response.as_ref().map(|r| r.sw2).unwrap_or(0);
            response = conn.send_apdu(
                client_id,
                slot,
                channel.channel_id,
                &build_get_response_apdu(len),
            );
        } else if matches!(response.as_ref().map(|r| r.sw1), Ok(0x6c)) {
            let le = response.as_ref().map(|r| r.sw2).unwrap_or(0);
            let mut adjusted = apdu.clone();
            if let Some(last) = adjusted.last_mut() {
                *last = le;
            }
            response = conn.send_apdu(client_id, slot, channel.channel_id, &adjusted);
        }
        let _ = conn.close_logical_channel(client_id, slot, channel.channel_id);
        let _ = conn.release_uim_cid(client_id);
        let response = response.map_err(|_| "sim_auth_apdu_exchange_failed")?;
        parse_usim_authenticate_response_reason(&response)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct QmiUimRetryPolicy {
    pub attempts: usize,
    pub timeout: Duration,
    pub retry_delay: Duration,
}

pub fn execute_usim_authenticate_via_proxy_reason_with_retry(
    proxy_socket: &str,
    device_path: &str,
    slot: u8,
    aid: &[u8],
    rand: &[u8],
    autn: &[u8],
    policy: QmiUimRetryPolicy,
) -> Result<UsimAkaApduResult, &'static str> {
    let attempts = policy.attempts.max(1);
    let mut last_reason = "sim_auth_retry_not_attempted";
    for attempt in 1..=attempts {
        match execute_usim_authenticate_via_proxy_reason(
            proxy_socket,
            device_path,
            slot,
            aid,
            rand,
            autn,
            policy.timeout,
        ) {
            Ok(result) => return Ok(result),
            Err(reason) => {
                last_reason = reason;
                if attempt == attempts || !sim_auth_reason_is_retryable(reason) {
                    return Err(reason);
                }
                std::thread::sleep(policy.retry_delay);
            }
        }
    }
    Err(last_reason)
}

pub fn verify_usim_application_via_proxy_reason(
    proxy_socket: &str,
    device_path: &str,
    slot: u8,
    aid: &[u8],
    timeout: Duration,
) -> Result<(), &'static str> {
    #[cfg(not(unix))]
    {
        let _ = (proxy_socket, device_path, slot, aid, timeout);
        Err("sim_auth_platform_unsupported")
    }

    #[cfg(unix)]
    {
        let mut conn = QmiProxyConnection::connect(proxy_socket, timeout)
            .map_err(|_| "sim_auth_proxy_connect_failed")?;
        conn.proxy_open(device_path)
            .map_err(|_| "sim_auth_proxy_open_failed")?;
        let client_id = conn
            .allocate_uim_cid()
            .map_err(|_| "sim_auth_uim_client_failed")?;
        let channel = match conn.open_logical_channel(client_id, slot, aid) {
            Ok(channel) => channel,
            Err(_) => {
                let _ = conn.release_uim_cid(client_id);
                return Err("sim_auth_logical_channel_failed");
            }
        };
        let _ = conn.close_logical_channel(client_id, slot, channel.channel_id);
        let _ = conn.release_uim_cid(client_id);
        Ok(())
    }
}

pub fn verify_usim_application_via_proxy_reason_with_retry(
    proxy_socket: &str,
    device_path: &str,
    slot: u8,
    aid: &[u8],
    attempts: usize,
    timeout: Duration,
    retry_delay: Duration,
) -> Result<(), &'static str> {
    let attempts = attempts.max(1);
    let mut last_reason = "sim_auth_gate_not_attempted";
    for attempt in 1..=attempts {
        match verify_usim_application_via_proxy_reason(
            proxy_socket,
            device_path,
            slot,
            aid,
            timeout,
        ) {
            Ok(()) => return Ok(()),
            Err(reason) => {
                last_reason = reason;
                if attempt == attempts || !sim_auth_reason_is_retryable(reason) {
                    return Err(reason);
                }
                std::thread::sleep(retry_delay);
            }
        }
    }
    Err(last_reason)
}

pub fn sim_auth_reason_is_retryable(reason: &str) -> bool {
    matches!(
        reason,
        "sim_auth_proxy_connect_failed"
            | "sim_auth_proxy_open_failed"
            | "sim_auth_uim_client_failed"
            | "sim_auth_logical_channel_failed"
            | "sim_auth_logical_channel_close_failed"
            | "sim_auth_apdu_exchange_failed"
            | "sim_auth_apdu_security_status"
            | "sim_auth_aka_response_parse_failed"
    )
}

#[cfg(unix)]
struct QmiProxyConnection {
    stream: UnixStream,
    next_ctl_transaction: u16,
    next_service_transaction: u16,
}

#[cfg(unix)]
impl QmiProxyConnection {
    fn connect(proxy_socket: &str, timeout: Duration) -> Result<Self, QmiUimError> {
        let stream = if let Some(name) = proxy_socket.strip_prefix('@') {
            connect_abstract_socket(name)?
        } else {
            UnixStream::connect(proxy_socket)?
        };
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        Ok(Self {
            stream,
            next_ctl_transaction: 1,
            next_service_transaction: 1,
        })
    }

    fn proxy_open(&mut self, device_path: &str) -> Result<(), QmiUimError> {
        let tx = self.take_ctl_transaction();
        let frame = build_proxy_open_frame(device_path, tx)?;
        let response = self.send_and_recv(&frame)?;
        if response.message_id != QMI_PROXY_OPEN {
            return Err(QmiUimError::InvalidFrame);
        }
        ensure_success(&response)?;
        Ok(())
    }

    fn allocate_uim_cid(&mut self) -> Result<u8, QmiUimError> {
        let tx = self.take_ctl_transaction();
        let frame = build_allocate_uim_cid_frame(tx)?;
        let response = self.send_and_recv(&frame)?;
        if response.message_id != QMI_CTL_ALLOCATE_CID {
            return Err(QmiUimError::InvalidFrame);
        }
        parse_allocated_cid(&response)
    }

    fn release_uim_cid(&mut self, client_id: u8) -> Result<(), QmiUimError> {
        let tx = self.take_ctl_transaction();
        let frame = build_release_uim_cid_frame(client_id, tx)?;
        let response = self.send_and_recv(&frame)?;
        if response.message_id != QMI_CTL_RELEASE_CID {
            return Err(QmiUimError::InvalidFrame);
        }
        ensure_success(&response)?;
        Ok(())
    }

    fn open_logical_channel(
        &mut self,
        client_id: u8,
        slot: u8,
        aid: &[u8],
    ) -> Result<LogicalChannelOpened, QmiUimError> {
        let tx = self.take_service_transaction();
        let frame = build_open_logical_channel_frame(client_id, tx, slot, aid)?;
        let response = self.send_and_recv(&frame)?;
        if response.message_id != QMI_UIM_OPEN_LOGICAL_CHANNEL {
            return Err(QmiUimError::InvalidFrame);
        }
        parse_open_logical_channel(&response)
    }

    fn close_logical_channel(
        &mut self,
        client_id: u8,
        slot: u8,
        channel_id: u8,
    ) -> Result<(), QmiUimError> {
        let tx = self.take_service_transaction();
        let frame = build_close_logical_channel_frame(client_id, tx, slot, channel_id)?;
        let response = self.send_and_recv(&frame)?;
        if response.message_id != QMI_UIM_LOGICAL_CHANNEL {
            return Err(QmiUimError::InvalidFrame);
        }
        ensure_success(&response)?;
        Ok(())
    }

    fn send_apdu(
        &mut self,
        client_id: u8,
        slot: u8,
        channel_id: u8,
        apdu: &[u8],
    ) -> Result<UimApduResponse, QmiUimError> {
        let tx = self.take_service_transaction();
        let frame = build_send_apdu_frame(client_id, tx, slot, channel_id, apdu)?;
        let response = self.send_and_recv(&frame)?;
        if response.message_id != QMI_UIM_SEND_APDU {
            return Err(QmiUimError::InvalidFrame);
        }
        parse_send_apdu_response(&response)
    }

    fn send_and_recv(&mut self, frame: &[u8]) -> Result<QmiMessage, QmiUimError> {
        self.stream.write_all(frame)?;
        self.stream.flush()?;
        read_qmi_message(&mut self.stream)
    }

    fn take_ctl_transaction(&mut self) -> u16 {
        let current = self.next_ctl_transaction;
        self.next_ctl_transaction = if current == u8::MAX as u16 {
            1
        } else {
            current + 1
        };
        current
    }

    fn take_service_transaction(&mut self) -> u16 {
        let current = self.next_service_transaction;
        self.next_service_transaction = if current == u16::MAX { 1 } else { current + 1 };
        current
    }
}

#[cfg(unix)]
fn connect_abstract_socket(name: &str) -> io::Result<UnixStream> {
    use std::os::linux::net::SocketAddrExt;
    use std::os::unix::net::SocketAddr;
    let addr = SocketAddr::from_abstract_name(name.as_bytes())?;
    UnixStream::connect_addr(&addr)
}

#[cfg(unix)]
fn read_qmi_message(stream: &mut UnixStream) -> Result<QmiMessage, QmiUimError> {
    let mut header = [0u8; 3];
    stream.read_exact(&mut header)?;
    if header[0] != 0x01 {
        return Err(QmiUimError::InvalidFrame);
    }
    let qmux_len = u16::from_le_bytes([header[1], header[2]]) as usize;
    let mut frame = Vec::with_capacity(1 + qmux_len);
    frame.extend_from_slice(&header);
    frame.resize(1 + qmux_len, 0);
    stream.read_exact(&mut frame[3..])?;
    decode_qmi_frame(&frame)
}

pub fn decode_qmi_frame(frame: &[u8]) -> Result<QmiMessage, QmiUimError> {
    if frame.len() < 12 {
        return Err(QmiUimError::FrameTooShort);
    }
    if frame[0] != 0x01 {
        return Err(QmiUimError::InvalidFrame);
    }
    let qmux_len = u16::from_le_bytes([frame[1], frame[2]]) as usize;
    if qmux_len + 1 != frame.len() || frame[3] != 0x80 && frame[3] != 0x00 {
        return Err(QmiUimError::InvalidFrame);
    }
    let service = frame[4];
    let client_id = frame[5];
    let (transaction_id, message_offset) = if service == QMUX_CTL_SERVICE {
        (u16::from(frame[7]), 8usize)
    } else {
        (u16::from_le_bytes([frame[7], frame[8]]), 9usize)
    };
    if frame.len() < message_offset + 4 {
        return Err(QmiUimError::FrameTooShort);
    }
    let message_id = u16::from_le_bytes([frame[message_offset], frame[message_offset + 1]]);
    let tlv_len =
        u16::from_le_bytes([frame[message_offset + 2], frame[message_offset + 3]]) as usize;
    let tlv_start = message_offset + 4;
    let tlv_end = tlv_start
        .checked_add(tlv_len)
        .ok_or(QmiUimError::InvalidFrame)?;
    if tlv_end != frame.len() {
        return Err(QmiUimError::InvalidFrame);
    }
    let tlvs = decode_tlvs(&frame[tlv_start..tlv_end])?;
    Ok(QmiMessage {
        service,
        client_id,
        transaction_id,
        message_id,
        tlvs,
    })
}

pub fn encode_qmi_message(message: &QmiMessage) -> Result<Vec<u8>, QmiUimError> {
    let mut tlv_bytes = Vec::new();
    for item in &message.tlvs {
        if item.value.len() > u16::MAX as usize {
            return Err(QmiUimError::MessageTooLarge);
        }
        tlv_bytes.push(item.tlv_type);
        tlv_bytes.extend_from_slice(&(item.value.len() as u16).to_le_bytes());
        tlv_bytes.extend_from_slice(&item.value);
    }
    if tlv_bytes.len() > u16::MAX as usize {
        return Err(QmiUimError::MessageTooLarge);
    }

    let mut qmi = Vec::new();
    qmi.push(0x00);
    if message.service == QMUX_CTL_SERVICE {
        if message.transaction_id > u8::MAX as u16 {
            return Err(QmiUimError::MessageTooLarge);
        }
        qmi.push(message.transaction_id as u8);
    } else {
        qmi.extend_from_slice(&message.transaction_id.to_le_bytes());
    }
    qmi.extend_from_slice(&message.message_id.to_le_bytes());
    qmi.extend_from_slice(&(tlv_bytes.len() as u16).to_le_bytes());
    qmi.extend_from_slice(&tlv_bytes);

    let qmux_len = 5usize
        .checked_add(qmi.len())
        .ok_or(QmiUimError::MessageTooLarge)?;
    if qmux_len > u16::MAX as usize {
        return Err(QmiUimError::MessageTooLarge);
    }
    let mut frame = Vec::with_capacity(1 + qmux_len);
    frame.push(0x01);
    frame.extend_from_slice(&(qmux_len as u16).to_le_bytes());
    frame.push(0x00);
    frame.push(message.service);
    frame.push(message.client_id);
    frame.extend_from_slice(&qmi);
    Ok(frame)
}

fn parse_successful_auth_response(input: &[u8]) -> Result<UsimAkaApduResult, QmiUimError> {
    let (res, rest) = take_lv(input)?;
    let (ck, rest) = take_lv(rest)?;
    let (ik, rest) = take_lv(rest)?;
    consume_optional_auth_tail(rest)?;
    Ok(UsimAkaApduResult {
        res: res.to_vec(),
        ck: ck.to_vec(),
        ik: ik.to_vec(),
        auts: None,
    })
}

fn consume_optional_auth_tail(mut input: &[u8]) -> Result<(), QmiUimError> {
    while !input.is_empty() {
        let (_value, rest) = take_lv(input)?;
        input = rest;
    }
    Ok(())
}

fn unwrap_authenticate_response_data(input: &[u8]) -> Result<&[u8], QmiUimError> {
    if matches!(input.first(), Some(0xdb | 0xdc)) {
        return Ok(input);
    }
    if input.len() >= 2 {
        let len = usize::from(input[1]);
        if input.len() == 2 + len && matches!(input[2], 0xdb | 0xdc) {
            return Ok(&input[2..]);
        }
    }
    if input.len() >= 3 && input[1] == 0x81 {
        let len = usize::from(input[2]);
        if input.len() == 3 + len && matches!(input[3], 0xdb | 0xdc) {
            return Ok(&input[3..]);
        }
    }
    Err(QmiUimError::InvalidAkaResponse)
}

fn take_lv(input: &[u8]) -> Result<(&[u8], &[u8]), QmiUimError> {
    let (&len, rest) = input.split_first().ok_or(QmiUimError::InvalidAkaResponse)?;
    let len = usize::from(len);
    if rest.len() < len {
        return Err(QmiUimError::InvalidAkaResponse);
    }
    Ok(rest.split_at(len))
}

fn decode_tlvs(mut input: &[u8]) -> Result<Vec<QmiTlv>, QmiUimError> {
    let mut tlvs = Vec::new();
    while !input.is_empty() {
        if input.len() < 3 {
            return Err(QmiUimError::InvalidFrame);
        }
        let tlv_type = input[0];
        let len = u16::from_le_bytes([input[1], input[2]]) as usize;
        if input.len() < 3 + len {
            return Err(QmiUimError::InvalidFrame);
        }
        tlvs.push(tlv(tlv_type, input[3..3 + len].to_vec()));
        input = &input[3 + len..];
    }
    Ok(tlvs)
}

fn ensure_success(message: &QmiMessage) -> Result<QmiResult, QmiUimError> {
    let value = find_tlv(message, TLV_RESULT).ok_or(QmiUimError::MissingTlv("result"))?;
    if value.len() < 4 {
        return Err(QmiUimError::InvalidFrame);
    }
    let result = u16::from_le_bytes([value[0], value[1]]);
    let error = u16::from_le_bytes([value[2], value[3]]);
    if result == 0 {
        Ok(QmiResult {
            success: true,
            error_code: None,
        })
    } else {
        Err(QmiUimError::ResultFailure(error))
    }
}

fn find_tlv(message: &QmiMessage, tlv_type: u8) -> Option<&[u8]> {
    message
        .tlvs
        .iter()
        .find(|item| item.tlv_type == tlv_type)
        .map(|item| item.value.as_slice())
}

fn tlv(tlv_type: u8, value: Vec<u8>) -> QmiTlv {
    QmiTlv { tlv_type, value }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_ctl_proxy_and_allocate_cid_frames() {
        let proxy = build_proxy_open_frame("/dev/wwan0qmi0", 1).expect("proxy frame");
        assert_eq!(&proxy[..4], &[1, 28, 0, 0]);
        assert_eq!(proxy[4], QMUX_CTL_SERVICE);

        let alloc = build_allocate_uim_cid_frame(2).expect("alloc frame");
        let decoded = decode_qmi_frame(&{
            let mut response = alloc.clone();
            response[3] = 0x80;
            response
        })
        .expect("decode");
        assert_eq!(decoded.message_id, QMI_CTL_ALLOCATE_CID);
    }

    #[test]
    fn builds_usim_authenticate_apdu_without_serializing_values() {
        let apdu = build_usim_authenticate_apdu(&[0x11; 16], &[0x22; 16]).expect("apdu");

        assert_eq!(&apdu[..5], &[0x00, 0x88, 0x00, 0x81, 34]);
        assert_eq!(apdu[5], 16);
        assert_eq!(apdu[22], 16);
        assert_eq!(*apdu.last().expect("le"), 0);
    }

    #[test]
    fn parses_usim_authenticate_success_response() {
        let response = UimApduResponse {
            data: [
                vec![0xdb, 8],
                vec![0x11; 8],
                vec![16],
                vec![0x22; 16],
                vec![16],
                vec![0x33; 16],
            ]
            .concat(),
            sw1: 0x90,
            sw2: 0x00,
        };

        let parsed = parse_usim_authenticate_response(&response).expect("aka");

        assert_eq!(parsed.res.len(), 8);
        assert_eq!(parsed.ck.len(), 16);
        assert_eq!(parsed.ik.len(), 16);
        assert_eq!(parsed.auts, None);
    }

    #[test]
    fn parses_wrapped_usim_authenticate_success_response() {
        let inner = [
            vec![0xdb, 4],
            vec![0x11; 4],
            vec![16],
            vec![0x22; 16],
            vec![16],
            vec![0x33; 16],
        ]
        .concat();
        let response = UimApduResponse {
            data: [vec![0x80, inner.len() as u8], inner].concat(),
            sw1: 0x90,
            sw2: 0x00,
        };

        let parsed = parse_usim_authenticate_response(&response).expect("wrapped aka");

        assert_eq!(parsed.res.len(), 4);
        assert_eq!(parsed.ck.len(), 16);
        assert_eq!(parsed.ik.len(), 16);
    }

    #[test]
    fn accepts_optional_tail_after_usim_aka_ck_ik() {
        let response = UimApduResponse {
            data: [
                vec![0xdb, 8],
                vec![0x11; 8],
                vec![16],
                vec![0x22; 16],
                vec![16],
                vec![0x33; 16],
                vec![8],
                vec![0x44; 8],
            ]
            .concat(),
            sw1: 0x90,
            sw2: 0x00,
        };

        let parsed = parse_usim_authenticate_response(&response).expect("aka with optional tail");

        assert_eq!(parsed.res.len(), 8);
        assert_eq!(parsed.ck.len(), 16);
        assert_eq!(parsed.ik.len(), 16);
    }

    #[test]
    fn classifies_apdu_status_without_values() {
        let response = UimApduResponse {
            data: Vec::new(),
            sw1: 0x6a,
            sw2: 0x86,
        };

        assert_eq!(
            parse_usim_authenticate_response_reason(&response).unwrap_err(),
            "sim_auth_apdu_parameter_rejected"
        );
    }

    #[test]
    fn classifies_retryable_sim_auth_reasons() {
        assert!(sim_auth_reason_is_retryable(
            "sim_auth_logical_channel_failed"
        ));
        assert!(sim_auth_reason_is_retryable(
            "sim_auth_apdu_exchange_failed"
        ));
        assert!(!sim_auth_reason_is_retryable(
            "sim_auth_apdu_parameter_rejected"
        ));
        assert!(!sim_auth_reason_is_retryable(
            "sim_auth_platform_unsupported"
        ));
    }

    #[test]
    fn parses_send_apdu_response_tlv() {
        let mut value = Vec::new();
        value.extend_from_slice(&4u16.to_le_bytes());
        value.extend_from_slice(&[0xdb, 0x00, 0x90, 0x00]);
        let message = QmiMessage {
            service: QMUX_UIM_SERVICE,
            client_id: 3,
            transaction_id: 7,
            message_id: QMI_UIM_SEND_APDU,
            tlvs: vec![
                tlv(TLV_RESULT, vec![0, 0, 0, 0]),
                tlv(TLV_UIM_APDU_RESPONSE, value),
            ],
        };

        let response = parse_send_apdu_response(&message).expect("apdu response");

        assert_eq!(response.data, vec![0xdb, 0x00]);
        assert_eq!(response.sw1, 0x90);
        assert_eq!(response.sw2, 0x00);
    }
}
