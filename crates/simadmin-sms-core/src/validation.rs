use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SmsValidationError {
    #[error("phone_number must contain between 1 and 64 characters")]
    InvalidPhoneNumber,
    #[error("content must contain between 1 and 20000 characters")]
    InvalidContent,
    #[error("invalid SMS transport")]
    InvalidTransport,
}

pub fn validate_sms_payload(
    phone_number: &str,
    content: &str,
    transport: &str,
) -> Result<(), SmsValidationError> {
    let phone = phone_number.trim();
    if phone.is_empty() || phone.chars().count() > 64 {
        return Err(SmsValidationError::InvalidPhoneNumber);
    }
    if content.is_empty() || content.chars().count() > 20_000 {
        return Err(SmsValidationError::InvalidContent);
    }
    let trans = transport.trim();
    if trans.is_empty() || trans.chars().count() > 32 {
        return Err(SmsValidationError::InvalidTransport);
    }
    Ok(())
}

pub fn mask_phone_number(value: &str) -> String {
    let chars = value.chars().collect::<Vec<_>>();
    if chars.len() <= 7 {
        return "***".into();
    }
    format!(
        "{}****{}",
        chars.iter().take(3).collect::<String>(),
        chars.iter().skip(chars.len() - 4).collect::<String>()
    )
}

pub fn sanitize_sms_summary(content: &str) -> String {
    let chars: Vec<char> = content.chars().collect();
    if chars.len() <= 60 {
        content.to_string()
    } else {
        format!("{}...", chars[..60].iter().collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation() {
        assert!(validate_sms_payload("13800000000", "hello", "modem").is_ok());
        assert_eq!(
            validate_sms_payload("", "hello", "modem"),
            Err(SmsValidationError::InvalidPhoneNumber)
        );
        assert_eq!(
            validate_sms_payload("13800000000", "", "modem"),
            Err(SmsValidationError::InvalidContent)
        );
        assert_eq!(
            validate_sms_payload("13800000000", "hello", ""),
            Err(SmsValidationError::InvalidTransport)
        );
    }

    #[test]
    fn test_mask_phone() {
        assert_eq!(mask_phone_number("13800138000"), "138****8000");
        assert_eq!(mask_phone_number("95588"), "***");
    }
}
