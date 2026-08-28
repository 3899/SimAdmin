use std::num::NonZeroU32;

use anyhow::{bail, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ring::{
    digest, pbkdf2,
    rand::{SecureRandom, SystemRandom},
};
use serde::{Deserialize, Serialize};

pub const ADMIN_PASSWORD_KEY: &str = "admin_password_hash";
pub const PASSWORD_MAX_LENGTH: usize = 64;
pub const PASSWORD_MIN_LENGTH_MIN: u8 = 1;
pub const PASSWORD_MIN_LENGTH_MAX: u8 = PASSWORD_MAX_LENGTH as u8;
pub const SESSION_TTL_NEVER_SECONDS: i64 = 100 * 365 * 24 * 60 * 60;
pub const SESSION_TTL_OPTIONS: [i64; 5] = [
    24 * 60 * 60,
    7 * 24 * 60 * 60,
    14 * 24 * 60 * 60,
    30 * 24 * 60 * 60,
    -1,
];
pub const IDLE_TIMEOUT_OPTIONS: [i64; 6] =
    [30 * 60, 60 * 60, 2 * 60 * 60, 3 * 60 * 60, 6 * 60 * 60, 0];

const PASSWORD_ALGORITHM: &str = "pbkdf2_sha256";
const PBKDF2_ITERATIONS: u32 = 210_000;
const PASSWORD_SALT_LEN: usize = 16;
const PASSWORD_HASH_LEN: usize = 32;
const SESSION_TOKEN_LEN: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SecurityConfig {
    #[serde(default)]
    pub password_protection_enabled: bool,
    #[serde(default = "default_password_min_length")]
    pub password_min_length: u8,
    #[serde(default = "default_true")]
    pub password_require_letters: bool,
    #[serde(default = "default_true")]
    pub password_require_digits: bool,
    #[serde(default = "default_true")]
    pub password_require_symbols: bool,
    #[serde(default = "default_session_ttl_seconds")]
    pub session_ttl_seconds: i64,
    #[serde(default = "default_idle_timeout_seconds")]
    pub idle_timeout_seconds: i64,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            password_protection_enabled: false,
            password_min_length: default_password_min_length(),
            password_require_letters: true,
            password_require_digits: true,
            password_require_symbols: true,
            session_ttl_seconds: default_session_ttl_seconds(),
            idle_timeout_seconds: default_idle_timeout_seconds(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoginRequest {
    pub password: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChangePasswordRequest {
    pub new_password: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuthStatusResponse {
    pub configured: bool,
    pub authenticated: bool,
    pub settings: SecurityConfig,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuthSettingsResponse {
    pub configured: bool,
    pub settings: SecurityConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionToken {
    pub token: String,
    pub hash: String,
}

pub fn normalize_security_settings(mut settings: SecurityConfig) -> SecurityConfig {
    if !(PASSWORD_MIN_LENGTH_MIN..=PASSWORD_MIN_LENGTH_MAX).contains(&settings.password_min_length)
    {
        settings.password_min_length = SecurityConfig::default().password_min_length;
    }
    if !SESSION_TTL_OPTIONS.contains(&settings.session_ttl_seconds) {
        settings.session_ttl_seconds = SecurityConfig::default().session_ttl_seconds;
    }
    if !IDLE_TIMEOUT_OPTIONS.contains(&settings.idle_timeout_seconds) {
        settings.idle_timeout_seconds = SecurityConfig::default().idle_timeout_seconds;
    }
    if !settings.password_require_letters
        && !settings.password_require_digits
        && !settings.password_require_symbols
    {
        settings.password_require_letters = true;
    }
    settings
}

pub fn validate_security_settings(settings: &SecurityConfig) -> Result<()> {
    if !(PASSWORD_MIN_LENGTH_MIN..=PASSWORD_MIN_LENGTH_MAX).contains(&settings.password_min_length)
    {
        bail!("密码最小长度需为 1-64 之间的整数");
    }
    if !settings.password_require_letters
        && !settings.password_require_digits
        && !settings.password_require_symbols
    {
        bail!("字符类型要求至少需要选择一项");
    }
    if !SESSION_TTL_OPTIONS.contains(&settings.session_ttl_seconds) {
        bail!("会话有效期只能选择 1 天、7 天、14 天、30 天或永不过期");
    }
    if !IDLE_TIMEOUT_OPTIONS.contains(&settings.idle_timeout_seconds) {
        bail!("空闲超时只能选择 30 分钟、1 小时、2 小时、3 小时、6 小时或关闭");
    }
    Ok(())
}

pub fn configured_session_ttl_seconds(settings: &SecurityConfig) -> i64 {
    if settings.session_ttl_seconds < 0 {
        SESSION_TTL_NEVER_SECONDS
    } else {
        settings.session_ttl_seconds
    }
}

pub fn validate_admin_password(password: &str, settings: &SecurityConfig) -> Result<()> {
    let settings = normalize_security_settings(settings.clone());
    if !password
        .bytes()
        .all(|byte| password_byte_allowed(byte, &settings))
    {
        bail!(
            "密码只能包含{}，不能包含空格、中文或未启用的字符类型",
            enabled_password_types_text(&settings)
        );
    }
    if !((settings.password_min_length as usize)..=PASSWORD_MAX_LENGTH).contains(&password.len()) {
        bail!(
            "密码长度需为 {}-{} 个字符",
            settings.password_min_length,
            PASSWORD_MAX_LENGTH
        );
    }
    if settings.password_require_letters && !password.bytes().any(|byte| byte.is_ascii_alphabetic())
    {
        bail!("密码需包含英文字母");
    }
    if settings.password_require_digits && !password.bytes().any(|byte| byte.is_ascii_digit()) {
        bail!("密码需包含数字");
    }
    if settings.password_require_symbols
        && !password
            .bytes()
            .any(|byte| byte.is_ascii_graphic() && !byte.is_ascii_alphanumeric())
    {
        bail!("密码需包含符号");
    }
    Ok(())
}

pub fn hash_password(password: &str, settings: &SecurityConfig) -> Result<String> {
    validate_admin_password(password, settings)?;
    let rng = SystemRandom::new();
    let mut salt = [0u8; PASSWORD_SALT_LEN];
    rng.fill(&mut salt)
        .map_err(|_| anyhow::anyhow!("生成密码盐失败"))?;
    let mut output = [0u8; PASSWORD_HASH_LEN];
    let iterations = NonZeroU32::new(PBKDF2_ITERATIONS).expect("non-zero iterations");
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        iterations,
        &salt,
        password.as_bytes(),
        &mut output,
    );
    Ok(format!(
        "{}${}${}${}",
        PASSWORD_ALGORITHM,
        PBKDF2_ITERATIONS,
        URL_SAFE_NO_PAD.encode(salt),
        URL_SAFE_NO_PAD.encode(output)
    ))
}

pub fn verify_password(password: &str, encoded_hash: &str) -> Result<bool> {
    let parts = encoded_hash.split('$').collect::<Vec<_>>();
    if parts.len() != 4 || parts[0] != PASSWORD_ALGORITHM {
        bail!("不支持的密码哈希格式");
    }
    let iterations = parts[1].parse::<u32>()?;
    let iterations = NonZeroU32::new(iterations).ok_or_else(|| anyhow::anyhow!("密码哈希无效"))?;
    let salt = URL_SAFE_NO_PAD.decode(parts[2])?;
    let expected = URL_SAFE_NO_PAD.decode(parts[3])?;
    Ok(pbkdf2::verify(
        pbkdf2::PBKDF2_HMAC_SHA256,
        iterations,
        &salt,
        password.as_bytes(),
        &expected,
    )
    .is_ok())
}

pub fn generate_session_token() -> Result<SessionToken> {
    let rng = SystemRandom::new();
    let mut raw = [0u8; SESSION_TOKEN_LEN];
    rng.fill(&mut raw)
        .map_err(|_| anyhow::anyhow!("生成会话令牌失败"))?;
    let token = URL_SAFE_NO_PAD.encode(raw);
    let hash = hash_session_token(&token);
    Ok(SessionToken { token, hash })
}

pub fn hash_session_token(token: &str) -> String {
    URL_SAFE_NO_PAD.encode(digest::digest(&digest::SHA256, token.as_bytes()).as_ref())
}

pub fn session_cookie(
    cookie_name: &str,
    token: &str,
    settings: &SecurityConfig,
    secure: bool,
) -> String {
    let secure_attribute = if secure { "; Secure" } else { "" };
    format!(
        "{cookie_name}={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age={}{secure_attribute}",
        configured_session_ttl_seconds(settings)
    )
}

pub fn expired_session_cookie(cookie_name: &str, secure: bool) -> String {
    let secure_attribute = if secure { "; Secure" } else { "" };
    format!("{cookie_name}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0{secure_attribute}")
}

pub fn cookie_token(cookie_header: Option<&str>, cookie_name: &str) -> Option<String> {
    cookie_header?.split(';').find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        (name == cookie_name).then(|| value.to_owned())
    })
}

pub fn wants_login_redirect(accept: Option<&str>, fetch_mode: Option<&str>) -> bool {
    accept.is_some_and(|value| value.contains("text/html"))
        || fetch_mode.is_some_and(|value| value.eq_ignore_ascii_case("navigate"))
}

fn enabled_password_types_text(settings: &SecurityConfig) -> &'static str {
    match (
        settings.password_require_letters,
        settings.password_require_digits,
        settings.password_require_symbols,
    ) {
        (true, true, true) => "英文字母、数字和符号",
        (true, true, false) => "英文字母和数字",
        (true, false, true) => "英文字母和符号",
        (false, true, true) => "数字和符号",
        (true, false, false) => "英文字母",
        (false, true, false) => "数字",
        (false, false, true) => "符号",
        (false, false, false) => "英文字母、数字和符号",
    }
}

fn password_byte_allowed(byte: u8, settings: &SecurityConfig) -> bool {
    byte.is_ascii_graphic()
        && ((settings.password_require_letters && byte.is_ascii_alphabetic())
            || (settings.password_require_digits && byte.is_ascii_digit())
            || (settings.password_require_symbols && !byte.is_ascii_alphanumeric()))
}

const fn default_true() -> bool {
    true
}

const fn default_password_min_length() -> u8 {
    8
}

const fn default_session_ttl_seconds() -> i64 {
    7 * 24 * 60 * 60
}

const fn default_idle_timeout_seconds() -> i64 {
    60 * 60
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_hash_round_trips_without_storing_plaintext() {
        let settings = SecurityConfig::default();
        let hash = hash_password("Example#123", &settings).unwrap();
        assert!(!hash.contains("Example#123"));
        assert!(verify_password("Example#123", &hash).unwrap());
        assert!(!verify_password("Wrong#123", &hash).unwrap());
    }

    #[test]
    fn session_cookie_round_trips_and_supports_secure_mode() {
        let settings = SecurityConfig::default();
        let token = generate_session_token().unwrap();
        let cookie = session_cookie("product_session", &token.token, &settings, true);
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Lax"));
        assert!(cookie.contains("; Secure"));
        assert_eq!(
            cookie_token(Some(&cookie), "product_session").as_deref(),
            Some(token.token.as_str())
        );
    }

    #[test]
    fn invalid_security_options_are_rejected_or_normalized() {
        let settings = SecurityConfig {
            password_min_length: 0,
            password_require_letters: false,
            password_require_digits: false,
            password_require_symbols: false,
            ..SecurityConfig::default()
        };
        assert!(validate_security_settings(&settings).is_err());
        let normalized = normalize_security_settings(settings);
        assert_eq!(normalized.password_min_length, 8);
        assert!(normalized.password_require_letters);
    }
}
