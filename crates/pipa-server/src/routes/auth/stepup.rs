//! Step-up init + status. The actual confirmation page lives in
//! `confirm_page.rs` (it's browser-facing, not API).

use axum::Json;
use axum::extract::State;
use pipa_core::device::Scope;
use serde::{Deserialize, Serialize};

use crate::auth::AuthClaims;
use crate::error::{ApiError, ServerError};
use crate::ip_hash::hmac_ip;
use crate::middleware::forwarded::RealIp;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct StepUpInitRequest {
    pub operation: String,
    #[serde(default)]
    pub target: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StepUpInitResponse {
    pub code: String,
    pub verify_url: String,
    pub expires_in: i64,
    pub operation: String,
    pub target: Option<String>,
}

pub async fn stepup_init(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    real_ip: RealIp,
    Json(req): Json<StepUpInitRequest>,
) -> Result<Json<StepUpInitResponse>, ServerError> {
    // Look up device to enforce automation-cannot-stepup.
    let devices = state.auth.list_devices().await?;
    let device = devices
        .into_iter()
        .find(|d| d.id == claims.sub)
        .ok_or_else(|| ApiError::unauthorized("invalid_token", "device no longer exists"))?;

    if device.scope == Scope::Automation {
        return Err(ApiError::forbidden(
            "automation_scope_cannot_stepup",
            "automation devices cannot perform destructive operations",
        )
        .into());
    }

    let ip_hash = hmac_ip(&state, &real_ip.0);
    let token = state
        .auth
        .begin_step_up(
            &claims.sub,
            &req.operation,
            req.target.as_deref(),
            Some(&ip_hash),
        )
        .await?;

    let verify_url = format!(
        "{}/confirm/{}",
        state.config.server.public_url.trim_end_matches('/'),
        token.code
    );
    Ok(Json(StepUpInitResponse {
        code: token.code,
        verify_url,
        expires_in: token.expires_at - token.created_at,
        operation: token.operation,
        target: token.target,
    }))
}

#[derive(Debug, Deserialize)]
pub struct StepUpStatusRequest {
    pub code: String,
}

#[derive(Debug, Serialize)]
pub struct StepUpStatusResponse {
    pub status: &'static str,
}

pub async fn stepup_status(
    State(state): State<AppState>,
    Json(req): Json<StepUpStatusRequest>,
) -> Result<Json<StepUpStatusResponse>, ServerError> {
    // Knowing the code is sufficient — codes are short-lived and single-use
    // anyway, so the CLI does not need a bearer here.
    let status = state.auth.step_up_observe(&req.code).await?;
    Ok(Json(StepUpStatusResponse { status }))
}
