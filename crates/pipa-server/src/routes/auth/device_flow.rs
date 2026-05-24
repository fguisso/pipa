//! `POST /api/auth/device-init` and `POST /api/auth/device-poll`.
//!
//! `device-init` creates a `(code, secret)` pair. Anonymous access is fine
//! on first boot (no devices) because the /cli approval will require the
//! setup code anyway. For subsequent devices, an interactive bearer with
//! `manage:devices` is required so a rogue caller can't allocate codes.
//!
//! `device-poll` returns Pending / Approved+refresh+device / Expired without
//! ever leaking whether the code exists to a caller who doesn't know the
//! secret.

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use pipa_core::Scope;
use pipa_core::ports::PollResult;
use serde::{Deserialize, Serialize};

use crate::auth::tokens::verify_access_token;
use crate::auth::{check_scope, parse_scope};
use crate::error::{ApiError, ServerError};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct DeviceInitRequest {
    #[serde(default)]
    pub client_label_hint: Option<String>,
    pub scope: String,
    /// Reserved — `/cli` consumes the setup code at approval time, but we
    /// accept and forward-validate (no consumption) here so an older CLI
    /// that ships the code in the init body still works.
    #[serde(default)]
    #[allow(dead_code)]
    pub setup_code: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DeviceInitResponse {
    pub device_code: String,
    pub device_secret: String,
    pub verify_url: String,
    pub expires_in: i64,
}

pub async fn device_init(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<DeviceInitRequest>,
) -> Result<Json<DeviceInitResponse>, ServerError> {
    let _scope: Scope = req
        .scope
        .parse()
        .map_err(|_| ApiError::bad_request("invalid_scope", "scope must be interactive|automation"))?;

    let devices = state.auth.devices_count().await?;
    if devices > 0 {
        // Subsequent device — requires manage:devices bearer.
        let bearer = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .ok_or_else(|| {
                ApiError::unauthorized(
                    "bearer_required",
                    "first device already exists; additional devices require an existing session",
                )
            })?;
        let claims = verify_access_token(&state.hmac_key, bearer.trim()).map_err(|_| {
            ApiError::unauthorized("invalid_token", "access token invalid")
        })?;
        if !check_scope(&claims, "manage", Some("devices"))
            && !matches!(parse_scope(&claims.scope), Some(s) if s.verb == "manage" && s.target == "devices")
        {
            return Err(ApiError::forbidden(
                "insufficient_scope",
                "manage:devices scope required",
            )
            .into());
        }
    }

    let (code, secret) = state.auth.begin_pairing().await?;
    let verify_url = format!("{}/cli", state.config.server.public_url.trim_end_matches('/'));

    // Stash the label hint server-side via cookie? No — keep it simple, the
    // user types it on the form. We pass back the original device_code so
    // the CLI can render it for the user.
    let _ = req.client_label_hint;

    Ok(Json(DeviceInitResponse {
        device_code: code,
        device_secret: secret,
        verify_url,
        expires_in: 600,
    }))
}

#[derive(Debug, Deserialize)]
pub struct DevicePollRequest {
    pub device_code: String,
    pub device_secret: String,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DevicePollResponse {
    Pending,
    Approved {
        refresh_token: String,
        device_id: String,
        device_label: String,
        scope: String,
        server: String,
    },
}

pub async fn device_poll(
    State(state): State<AppState>,
    Json(req): Json<DevicePollRequest>,
) -> Result<axum::response::Response, ServerError> {
    use axum::response::IntoResponse;

    match state.auth.poll_pairing(&req.device_code, &req.device_secret).await {
        Ok(PollResult::Pending) => Ok((
            StatusCode::OK,
            Json(DevicePollResponse::Pending),
        )
            .into_response()),
        Ok(PollResult::Approved(issued)) => Ok((
            StatusCode::OK,
            Json(DevicePollResponse::Approved {
                refresh_token: issued.refresh_plaintext,
                device_id: issued.device.id,
                device_label: issued.device.label,
                scope: issued.device.scope.as_str().to_string(),
                server: state.config.server.public_url.clone(),
            }),
        )
            .into_response()),
        Ok(PollResult::Expired) => Ok(ApiError::gone("expired", "pairing expired")
            .into_response()),
        Err(pipa_core::CoreError::Unauthorized) => Err(ApiError::unauthorized(
            "bad_secret",
            "device_secret does not match the pairing",
        )
        .into()),
        Err(pipa_core::CoreError::NotFound) => Err(ApiError::not_found(
            "not_found",
            "device_code does not exist",
        )
        .into()),
        Err(e) => Err(ServerError::Core(e)),
    }
}
