//! `POST /api/auth/device-init` and `POST /api/auth/device-poll`.
//!
//! `device-init` allocates a `(code, secret)` pair. Anonymous: anyone can ask
//! for a code, but the only thing they can do with it is wait — actual
//! approval lives behind `/cli`, which requires the owner cookie. So allowing
//! anonymous init costs us nothing and lets the CLI flow stay one step (the
//! CLI doesn't need to authenticate to *ask* to be authenticated).
//!
//! `device-poll` returns Pending / Approved+refresh+device / Expired without
//! ever leaking whether the code exists to a caller who doesn't know the
//! secret.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use gapes_core::Scope;
use gapes_core::ports::PollResult;
use serde::{Deserialize, Serialize};

use crate::error::{ApiError, ServerError};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct DeviceInitRequest {
    #[serde(default)]
    pub client_label_hint: Option<String>,
    pub scope: String,
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
    Json(req): Json<DeviceInitRequest>,
) -> Result<Json<DeviceInitResponse>, ServerError> {
    let _scope: Scope = req
        .scope
        .parse()
        .map_err(|_| ApiError::bad_request("invalid_scope", "scope must be interactive|automation"))?;

    let (code, secret) = state.auth.begin_pairing().await?;
    // Verify URL includes the pairing code as a query string so the browser
    // opened by `gapes login` can pre-fill the approval form. Auth still
    // happens at /cli (which requires the owner cookie).
    let base = state.config.server.public_url.trim_end_matches('/');
    let verify_url = format!("{base}/cli?code={code}");

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
        Err(gapes_core::CoreError::Unauthorized) => Err(ApiError::unauthorized(
            "bad_secret",
            "device_secret does not match the pairing",
        )
        .into()),
        Err(gapes_core::CoreError::NotFound) => Err(ApiError::not_found(
            "not_found",
            "device_code does not exist",
        )
        .into()),
        Err(e) => Err(ServerError::Core(e)),
    }
}
