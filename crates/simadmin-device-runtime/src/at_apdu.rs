use std::{
    io::{Read, Write},
    path::PathBuf,
    time::{Duration, Instant},
};

use crate::{decode_hex, encode_hex, ApduError, ApduResult, ApduTransport};

pub struct SerialAtApduTransport {
    port: PathBuf,
    baud_rate: u32,
    timeout: Duration,
}

impl SerialAtApduTransport {
    pub fn new(port: impl Into<PathBuf>, timeout: Duration) -> Self {
        Self {
            port: port.into(),
            baud_rate: 115_200,
            timeout,
        }
    }

    pub fn with_baud_rate(mut self, baud_rate: u32) -> Self {
        self.baud_rate = baud_rate;
        self
    }

    fn command(&self, command: &str) -> ApduResult<String> {
        let mut port = serialport::new(self.port.to_string_lossy(), self.baud_rate)
            .timeout(Duration::from_millis(200))
            .open()
            .map_err(|error| ApduError::Transport(error.to_string()))?;
        port.write_all(format!("{command}\r").as_bytes())
            .and_then(|_| port.flush())
            .map_err(|error| ApduError::Transport(error.to_string()))?;

        let deadline = Instant::now() + self.timeout;
        let mut response = Vec::new();
        let mut buffer = [0u8; 512];
        while Instant::now() < deadline {
            match port.read(&mut buffer) {
                Ok(0) => {}
                Ok(read) => {
                    response.extend_from_slice(&buffer[..read]);
                    let text = String::from_utf8_lossy(&response);
                    if text.contains("\r\nOK\r\n") || text.contains("\r\nERROR\r\n") {
                        break;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {}
                Err(error) => return Err(ApduError::Transport(error.to_string())),
            }
        }
        let response = String::from_utf8_lossy(&response).into_owned();
        if !response.contains("OK") || response.contains("ERROR") {
            return Err(ApduError::Transport(if response.trim().is_empty() {
                "AT APDU command timed out".into()
            } else {
                response
            }));
        }
        Ok(response)
    }
}

impl ApduTransport for SerialAtApduTransport {
    type Channel = u8;

    fn open_logical_channel(&self, aid: &[u8]) -> ApduResult<Self::Channel> {
        let response = self.command(&format!("AT+CCHO=\"{}\"", encode_hex(aid)))?;
        response
            .lines()
            .map(str::trim)
            .find_map(|line| {
                line.strip_prefix("+CCHO:").map(str::trim).or_else(|| {
                    line.bytes()
                        .all(|byte| byte.is_ascii_digit())
                        .then_some(line)
                })
            })
            .and_then(|value| value.parse::<u8>().ok())
            .filter(|channel| *channel > 0)
            .ok_or_else(|| ApduError::InvalidResponse("AT+CCHO did not return a channel".into()))
    }

    fn transmit(&self, channel: Self::Channel, command: &[u8]) -> ApduResult<Vec<u8>> {
        let command = encode_hex(command);
        let response = self.command(&format!(
            "AT+CGLA={channel},{},\"{command}\"",
            command.len()
        ))?;
        let value = response
            .lines()
            .map(str::trim)
            .find_map(|line| line.strip_prefix("+CGLA:").map(str::trim))
            .and_then(|value| value.split_once(',').map(|(_, apdu)| apdu))
            .map(|value| value.trim().trim_matches('"'))
            .ok_or_else(|| ApduError::InvalidResponse("AT+CGLA response is missing".into()))?;
        decode_hex(value)
    }

    fn close_logical_channel(&self, channel: Self::Channel) -> ApduResult<()> {
        self.command(&format!("AT+CCHC={channel}"))?;
        Ok(())
    }
}
