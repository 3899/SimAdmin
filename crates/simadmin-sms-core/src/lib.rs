pub mod assembly;
pub mod classifier;
pub mod fingerprint;
pub mod normalization;
pub mod outbound;
pub mod rules;
pub mod template;
pub mod types;
pub mod validation;
pub mod verification_code;

pub use assembly::{AssembledSms, AssemblyResult, SmsAssembler, SmsPartHeader, SmsPartInput};
pub use classifier::classify_sms;
pub use fingerprint::{compute_content_hash, compute_pdu_hash, compute_sms_fingerprint};
pub use normalization::{
    format_beijing_time, is_phone_number_match, normalize_phone_number,
    parse_and_normalize_timestamp,
};
pub use outbound::{estimate_sms_segments, SmsEncoding, SmsSegmentEstimate};
pub use rules::{
    evaluate_rule_matcher, is_in_quiet_hours, MatcherOperator, QuietHoursSchedule, RuleMatcher,
};
pub use template::{
    clean_empty_verification_code_template, contains_verification_code_placeholder,
    render_sms_template,
};
pub use types::{SmsCategory, SmsDirection, SmsStatus};
pub use validation::{
    mask_phone_number, sanitize_sms_summary, validate_sms_payload, SmsValidationError,
};
pub use verification_code::extract_verification_code;
