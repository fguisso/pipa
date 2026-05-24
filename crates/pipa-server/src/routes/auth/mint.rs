//! `POST /api/auth/mint` — exchange a refresh token + requested scope for an
//! access token. Rotates the refresh on every successful call (the old one
//! is invalidated atomically inside `rotate_refresh`).
//!
//! Scope validation: automation refresh tokens cannot mint `admin:*`,
//! `destroy:*`, or `manage:*` access tokens, regardless of step-up. This is
//! the M3 enforcement of SECURITY.md §5.

use axum::Json;
use axum::extract::State;
use pipa_core::audit::{AuditAction, AuditEvent};
use pipa_core::device::Scope;
use serde::{Deserialize, Serialize};

use crate::auth::scope::parse_scope;
use crate::auth::tokens::mint_access_token;
use crate::error::{ApiError, ServerError};
use crate::state::AppState;

const ACCESS_TTL_CAP_SECONDS: i64 = 600;
const ACCESS_TTL_DEFAULT_SECONDS: i64 = 300;

#[derive(Debug, Deserialize)]
pub struct MintRequest {
    pub refresh: String,
    pub scope: String,
    #[serde(default)]
    pub ttl_sec: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct MintResponse {
    pub access: String,
    pub refresh: String,
    pub expires: i64,
}

pub async fn mint(
    State(state): State<AppState>,
    Json(req): Json<MintRequest>,
) -> Result<Json<MintResponse>, ServerError> {
    let lookup = state.auth.lookup_refresh(&req.refresh).await?;
    let Some((_token, device)) = lookup else {
        return Err(ApiError::unauthorized("invalid_refresh", "refresh token unknown or revoked").into());
    };

    let scope_ref = parse_scope(&req.scope).ok_or_else(|| {
        ApiError::bad_request("invalid_scope", "scope must be `<verb>:<target>` with a recognized verb")
    })?;

    // Automation cannot mint admin / destroy / manage — defense at the
    // mint layer so even a forged step-up token won't grant escalation.
    if device.scope == Scope::Automation {
        match scope_ref.verb {
            "admin" | "destroy" | "manage" => {
                return Err(ApiError::forbidden(
                    "automation_cannot_escalate",
                    "automation-scope tokens cannot mint admin/destroy/manage scopes",
                )
                .into());
            }
            _ => {}
        }
    }

    let ttl = req
        .ttl_sec
        .unwrap_or(ACCESS_TTL_DEFAULT_SECONDS)
        .clamp(1, ACCESS_TTL_CAP_SECONDS);

    let (_new_refresh, new_plaintext) = state.auth.rotate_refresh(&req.refresh).await?;
    let (token, claims) = mint_access_token(&state.hmac_key, &device.id, &req.scope, ttl)?;

    state.auth.touch_device(&device.id).await?;
    let _ = state
        .repo
        .record_audit(
            AuditEvent::success(claims.iat, device.id.clone(), AuditAction::AuthRefresh)
                .with_scope(req.scope.clone()),
        )
        .await;

    Ok(Json(MintResponse {
        access: token,
        refresh: new_plaintext,
        expires: ttl,
    }))
}
