use crate::{
    qmi_uim::{
        build_get_response_apdu, build_usim_authenticate_apdu,
        parse_usim_authenticate_response_reason, UimApduResponse, UsimAkaApduResult,
        USIM_AID_PREFIX,
    },
    ApduArbiter, ApduError, ApduOperation, ApduTransport,
};

const MAX_APDU_EXCHANGES: usize = 8;

#[derive(Debug, thiserror::Error)]
pub enum UsimAuthError {
    #[error(transparent)]
    Apdu(#[from] ApduError),
    #[error("{0}")]
    Authentication(&'static str),
}

impl UsimAuthError {
    pub fn reason_code(&self) -> &'static str {
        match self {
            Self::Apdu(ApduError::Busy(_)) => "sim_auth_apdu_busy",
            Self::Apdu(ApduError::Transport(_)) => "sim_auth_transport_failed",
            Self::Apdu(ApduError::InvalidResponse(_)) => "sim_auth_response_invalid",
            Self::Apdu(ApduError::EuiccUnavailable) => "sim_auth_application_unavailable",
            Self::Authentication(reason) => reason,
        }
    }
}

pub struct UsimAuthService<T> {
    transport: T,
    arbiter: ApduArbiter,
}

impl<T: ApduTransport> UsimAuthService<T> {
    pub fn new(device_id: impl Into<String>, transport: T) -> Self {
        Self::with_arbiter(transport, ApduArbiter::new(device_id))
    }

    pub fn with_arbiter(transport: T, arbiter: ApduArbiter) -> Self {
        Self { transport, arbiter }
    }

    pub fn verify_access(&self) -> Result<(), UsimAuthError> {
        self.arbiter.execute(ApduOperation::SimAuthentication, || {
            let channel = self.transport.open_logical_channel(USIM_AID_PREFIX)?;
            self.close_with_result(channel, Ok(()))
        })
    }

    pub fn authenticate(
        &self,
        rand: &[u8],
        autn: &[u8],
    ) -> Result<UsimAkaApduResult, UsimAuthError> {
        self.arbiter.execute(ApduOperation::SimAuthentication, || {
            let channel = self.transport.open_logical_channel(USIM_AID_PREFIX)?;
            let result = build_usim_authenticate_apdu(rand, autn)
                .map_err(|_| UsimAuthError::Authentication("sim_auth_request_invalid"))
                .and_then(|command| self.exchange(channel, command))
                .and_then(|response| {
                    parse_usim_authenticate_response_reason(&response)
                        .map_err(UsimAuthError::Authentication)
                });
            self.close_with_result(channel, result)
        })
    }

    fn exchange(
        &self,
        channel: T::Channel,
        mut command: Vec<u8>,
    ) -> Result<UimApduResponse, UsimAuthError> {
        let mut body = Vec::new();
        for _ in 0..MAX_APDU_EXCHANGES {
            let response = self.transport.transmit(channel, &command)?;
            let split = response
                .len()
                .checked_sub(2)
                .ok_or_else(|| ApduError::InvalidResponse("missing APDU status word".into()))?;
            let sw1 = response[split];
            let sw2 = response[split + 1];
            match sw1 {
                0x61 => {
                    body.extend_from_slice(&response[..split]);
                    command = build_get_response_apdu(sw2);
                }
                0x6c => {
                    let Some(le) = command.last_mut() else {
                        return Err(ApduError::InvalidResponse(
                            "cannot correct APDU length without Le".into(),
                        )
                        .into());
                    };
                    *le = sw2;
                }
                _ => {
                    body.extend_from_slice(&response[..split]);
                    return Ok(UimApduResponse {
                        data: body,
                        sw1,
                        sw2,
                    });
                }
            }
        }
        Err(ApduError::InvalidResponse("APDU continuation limit exceeded".into()).into())
    }

    fn close_with_result<R>(
        &self,
        channel: T::Channel,
        result: Result<R, UsimAuthError>,
    ) -> Result<R, UsimAuthError> {
        let close = self.transport.close_logical_channel(channel);
        match (result, close) {
            (Ok(value), Ok(())) => Ok(value),
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    use super::*;

    struct MockTransport {
        closed: Arc<AtomicBool>,
    }

    impl ApduTransport for MockTransport {
        type Channel = u8;

        fn open_logical_channel(&self, aid: &[u8]) -> Result<Self::Channel, ApduError> {
            assert_eq!(aid, USIM_AID_PREFIX);
            Ok(1)
        }

        fn transmit(&self, channel: Self::Channel, command: &[u8]) -> Result<Vec<u8>, ApduError> {
            assert_eq!(channel, 1);
            assert_eq!(&command[..5], &[0x00, 0x88, 0x00, 0x81, 34]);
            Ok([
                vec![0xdb, 8],
                vec![0x11; 8],
                vec![16],
                vec![0x22; 16],
                vec![16],
                vec![0x33; 16],
                vec![0x90, 0x00],
            ]
            .concat())
        }

        fn close_logical_channel(&self, channel: Self::Channel) -> Result<(), ApduError> {
            assert_eq!(channel, 1);
            self.closed.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    #[test]
    fn generic_transport_authenticates_and_closes_the_channel() {
        let closed = Arc::new(AtomicBool::new(false));
        let service = UsimAuthService::new(
            "device-1",
            MockTransport {
                closed: closed.clone(),
            },
        );

        let result = service.authenticate(&[0x44; 16], &[0x55; 16]).unwrap();

        assert_eq!(result.res, vec![0x11; 8]);
        assert_eq!(result.ck, vec![0x22; 16]);
        assert_eq!(result.ik, vec![0x33; 16]);
        assert!(closed.load(Ordering::SeqCst));
    }
}
