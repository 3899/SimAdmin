//! Single-admin authentication for the SimAdmin web console.

use std::io::{self, Write};

use anyhow::{bail, Result};
use axum::{
    extract::State,
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use serde_json::Value;
use simadmin_auth::{
    configured_session_ttl_seconds, generate_session_token, hash_password, hash_session_token,
    normalize_security_settings, validate_security_settings, verify_password, ADMIN_PASSWORD_KEY,
};
pub use simadmin_auth::{
    AuthSettingsResponse, AuthStatusResponse, ChangePasswordRequest, LoginRequest,
};

use crate::{
    config::SecurityConfig,
    db::Database,
    models::ApiResponse,
    state::AppState,
    system_event::{
        codes as system_event_codes, severity as system_event_severity,
        status as system_event_status,
    },
};

const SESSION_COOKIE: &str = "simadmin_session";

fn session_cookie(token: &str, settings: &SecurityConfig) -> String {
    simadmin_auth::session_cookie(SESSION_COOKIE, token, settings, false)
}

fn expired_session_cookie() -> String {
    simadmin_auth::expired_session_cookie(SESSION_COOKIE, false)
}

fn cookie_token(headers: &HeaderMap) -> Option<String> {
    simadmin_auth::cookie_token(
        headers
            .get(header::COOKIE)
            .and_then(|value| value.to_str().ok()),
        SESSION_COOKIE,
    )
}

fn wants_login_redirect(headers: &HeaderMap) -> bool {
    simadmin_auth::wants_login_redirect(
        headers
            .get(header::ACCEPT)
            .and_then(|value| value.to_str().ok()),
        headers
            .get("sec-fetch-mode")
            .and_then(|value| value.to_str().ok()),
    )
}

fn unauthorized_response(headers: &HeaderMap, message: impl Into<String>) -> Response {
    if wants_login_redirect(headers) {
        return (StatusCode::SEE_OTHER, [(header::LOCATION, "/login")]).into_response();
    }

    (
        StatusCode::UNAUTHORIZED,
        Json(ApiResponse::<Value>::error(message.into())),
    )
        .into_response()
}

fn response_with_session<T: Serialize>(
    payload: ApiResponse<T>,
    token: &str,
    settings: &SecurityConfig,
) -> Response {
    let mut response = Json(payload).into_response();
    let cookie = session_cookie(token, settings);
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        response.headers_mut().insert(header::SET_COOKIE, value);
    }
    response
}

fn is_authenticated(database: &Database, headers: &HeaderMap) -> bool {
    let Some(token) = cookie_token(headers) else {
        return false;
    };
    database
        .auth_session_valid(&hash_session_token(&token))
        .unwrap_or(false)
}

pub async fn auth_middleware(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    if request.method() == Method::OPTIONS {
        return next.run(request).await;
    }

    let settings = normalize_security_settings(state.config_manager.get_security());
    if !settings.password_protection_enabled {
        return next.run(request).await;
    }

    if !state.database.auth_is_configured().unwrap_or(false) {
        return unauthorized_response(&headers, "管理员密码尚未设置");
    }

    if !is_authenticated(&state.database, &headers) {
        return unauthorized_response(&headers, "请先登录");
    }

    next.run(request).await
}

pub async fn status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> (StatusCode, Json<ApiResponse<AuthStatusResponse>>) {
    let settings = normalize_security_settings(state.config_manager.get_security());
    let configured = state.database.auth_is_configured().unwrap_or(false);
    let authenticated = !settings.password_protection_enabled
        || (configured && is_authenticated(&state.database, &headers));
    (
        StatusCode::OK,
        Json(ApiResponse::success_with_message(
            "Success",
            AuthStatusResponse {
                configured,
                authenticated,
                settings,
            },
        )),
    )
}

pub async fn setup(State(state): State<AppState>, Json(payload): Json<LoginRequest>) -> Response {
    let settings = normalize_security_settings(state.config_manager.get_security());
    if state.database.auth_is_configured().unwrap_or(false) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<Value>::error("管理员密码已设置")),
        )
            .into_response();
    }

    let password_hash = match hash_password(&payload.password, &settings) {
        Ok(hash) => hash,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<Value>::error(err.to_string())),
            )
                .into_response()
        }
    };

    if let Err(err) = state
        .database
        .set_auth_config_value(ADMIN_PASSWORD_KEY, &password_hash)
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<Value>::error(format!(
                "保存管理员密码失败: {err}"
            ))),
        )
            .into_response();
    }

    let session = match generate_session_token() {
        Ok(session) => session,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<Value>::error(err.to_string())),
            )
                .into_response()
        }
    };

    if let Err(err) = state
        .database
        .insert_auth_session(&session.hash, configured_session_ttl_seconds(&settings))
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<Value>::error(format!("创建会话失败: {err}"))),
        )
            .into_response();
    }

    response_with_session(
        ApiResponse::success_with_message("Admin password configured", Value::Null),
        &session.token,
        &settings,
    )
}

pub async fn login(State(state): State<AppState>, Json(payload): Json<LoginRequest>) -> Response {
    let settings = normalize_security_settings(state.config_manager.get_security());
    let Some(password_hash) = state
        .database
        .get_auth_config_value(ADMIN_PASSWORD_KEY)
        .unwrap_or(None)
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<Value>::error("管理员密码尚未设置")),
        )
            .into_response();
    };

    match verify_password(&payload.password, &password_hash) {
        Ok(true) => {}
        Ok(false) => {
            state.system_event_emitter.record_login_failure().await;
            return (
                StatusCode::UNAUTHORIZED,
                Json(ApiResponse::<Value>::error("管理员密码不正确")),
            )
                .into_response();
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<Value>::error(format!("验证密码失败: {err}"))),
            )
                .into_response()
        }
    }

    let session = match generate_session_token() {
        Ok(session) => session,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<Value>::error(err.to_string())),
            )
                .into_response()
        }
    };

    if let Err(err) = state
        .database
        .insert_auth_session(&session.hash, configured_session_ttl_seconds(&settings))
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<Value>::error(format!("创建会话失败: {err}"))),
        )
            .into_response();
    }

    response_with_session(
        ApiResponse::success_with_message("Logged in", Value::Null),
        &session.token,
        &settings,
    )
}

pub async fn change_password(
    State(state): State<AppState>,
    Json(payload): Json<ChangePasswordRequest>,
) -> Response {
    let settings = normalize_security_settings(state.config_manager.get_security());
    let new_hash = match hash_password(&payload.new_password, &settings) {
        Ok(hash) => hash,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<Value>::error(err.to_string())),
            )
                .into_response()
        }
    };

    if let Err(err) = state.database.replace_admin_password_hash(&new_hash) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<Value>::error(format!("更新密码失败: {err}"))),
        )
            .into_response();
    }

    let session = match generate_session_token() {
        Ok(session) => session,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<Value>::error(err.to_string())),
            )
                .into_response()
        }
    };

    if let Err(err) = state
        .database
        .insert_auth_session(&session.hash, configured_session_ttl_seconds(&settings))
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<Value>::error(format!("创建会话失败: {err}"))),
        )
            .into_response();
    }

    state
        .system_event_emitter
        .emit_code(
            system_event_codes::SECURITY_PASSWORD_CHANGED,
            system_event_severity::WARNING,
            system_event_status::CHANGED,
            "admin",
            "管理员密码已修改",
        )
        .await;

    response_with_session(
        ApiResponse::success_with_message("Password updated", Value::Null),
        &session.token,
        &settings,
    )
}

pub async fn get_settings(
    State(state): State<AppState>,
) -> (StatusCode, Json<ApiResponse<AuthSettingsResponse>>) {
    let settings = normalize_security_settings(state.config_manager.get_security());
    let configured = state.database.auth_is_configured().unwrap_or(false);
    (
        StatusCode::OK,
        Json(ApiResponse::success_with_message(
            "Success",
            AuthSettingsResponse {
                configured,
                settings,
            },
        )),
    )
}

pub async fn set_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<SecurityConfig>,
) -> Response {
    let previous_settings = normalize_security_settings(state.config_manager.get_security());
    if let Err(err) = validate_security_settings(&payload) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<Value>::error(err.to_string())),
        )
            .into_response();
    }
    let settings = normalize_security_settings(payload);
    if let Err(err) = state.config_manager.set_security(settings.clone()) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<Value>::error(format!(
                "保存安全设置失败: {err}"
            ))),
        )
            .into_response();
    }

    if previous_settings.password_protection_enabled && !settings.password_protection_enabled {
        state
            .system_event_emitter
            .emit_code(
                system_event_codes::SECURITY_PASSWORD_PROTECTION_DISABLED,
                system_event_severity::CRITICAL,
                system_event_status::CHANGED,
                "security",
                "密码保护已关闭",
            )
            .await;
    } else if previous_settings != settings {
        state
            .system_event_emitter
            .emit_code(
                system_event_codes::SECURITY_POLICY_CHANGED,
                system_event_severity::WARNING,
                system_event_status::CHANGED,
                "security",
                "安全策略已变更",
            )
            .await;
    }

    let mut response = (
        StatusCode::OK,
        Json(ApiResponse::success_with_message(
            "Security settings saved",
            settings.clone(),
        )),
    )
        .into_response();

    if let Some(token) = cookie_token(&headers) {
        let _ = state.database.refresh_auth_session(
            &hash_session_token(&token),
            configured_session_ttl_seconds(&settings),
        );
        if let Ok(value) = HeaderValue::from_str(&session_cookie(&token, &settings)) {
            response.headers_mut().insert(header::SET_COOKIE, value);
        }
    }

    response
}

pub async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(token) = cookie_token(&headers) {
        let _ = state
            .database
            .delete_auth_session(&hash_session_token(&token));
    }

    let mut response =
        Json(ApiResponse::success_with_message("Logged out", Value::Null)).into_response();
    if let Ok(value) = HeaderValue::from_str(&expired_session_cookie()) {
        response.headers_mut().insert(header::SET_COOKIE, value);
    }
    response
}

pub fn reset_admin_password_interactive(
    database: &Database,
    settings: &SecurityConfig,
) -> Result<()> {
    let password = read_password_line("New admin password: ")?;
    let confirm = read_password_line("Confirm admin password: ")?;
    if password != confirm {
        bail!("Passwords do not match");
    }
    let hash = hash_password(&password, settings)?;
    database.replace_admin_password_hash(&hash)?;
    println!("Admin password updated and all web sessions were cleared.");
    Ok(())
}

pub fn clear_admin_auth(database: &Database) -> Result<()> {
    database.clear_admin_auth()?;
    println!("Admin password and all web sessions were cleared.");
    println!("Open the web UI to set a new admin password.");
    Ok(())
}

#[cfg(unix)]
fn read_password_line(prompt: &str) -> Result<String> {
    print!("{prompt}");
    io::stdout().flush()?;
    let _ = std::process::Command::new("stty").arg("-echo").status();
    let mut value = String::new();
    let result = io::stdin().read_line(&mut value);
    let _ = std::process::Command::new("stty").arg("echo").status();
    println!();
    result?;
    Ok(value.trim_end_matches(['\r', '\n']).to_string())
}

#[cfg(not(unix))]
fn read_password_line(prompt: &str) -> Result<String> {
    print!("{prompt}");
    io::stdout().flush()?;
    let mut value = String::new();
    io::stdin().read_line(&mut value)?;
    Ok(value.trim_end_matches(['\r', '\n']).to_string())
}
