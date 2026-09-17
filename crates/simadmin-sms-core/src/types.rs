use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmsDirection {
    Incoming,
    Outgoing,
}

impl SmsDirection {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Incoming => "incoming",
            Self::Outgoing => "outgoing",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmsStatus {
    Pending,
    Received,
    Sent,
    Failed,
}

impl SmsStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Received => "received",
            Self::Sent => "sent",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmsCategory {
    VerificationCode,
    Notification,
    Marketing,
    Personal,
}

impl SmsCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::VerificationCode => "verification_code",
            Self::Notification => "notification",
            Self::Marketing => "marketing",
            Self::Personal => "personal",
        }
    }
}
