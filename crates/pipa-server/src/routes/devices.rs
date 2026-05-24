//! `/api/devices` — list and revoke. Both endpoints require `manage:devices`.
//! Revoking a device that is not the caller's own device requires a valid
//! step-up code, presented via `X-Stepup-Code`.

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{delete, get};
use axum::Json;
use pipa_core::audit::{AuditAction, AuditEvent};
use serde::Serialize;

use crate::auth::{AuthClaims, check_scope, verify_step_up};
use crate::error::{ApiError, ServerError};
use crate::state::AppState;

const STEP_UP_HEADER: &str = "x-stepup-code";
const REVOKE_OPERATION: &str = "device.revoke";

#[derive(Debug, Serialize)]
pub struct ListResponse {
    pub devices: Vec<DeviceDto>,
}

#[derive(Debug, Serialize)]
pub struct DeviceDto {
    pub id: String,
    pub label: String,
    pub scope: String,
    pub created_at: i64,
    pub last_seen_at: Option<i64>,
    pub revoked_at: Option<i64>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/devices", get(list))
        .route("/api/devices/:id", delete(revoke))
}

async fn list(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
) -> Result<Json<ListResponse>, ServerError> {
    require_manage_devices(&claims)?;
    let devices = state.auth.list_devices().await?;
    Ok(Json(ListResponse {
        devices: devices
            .into_iter()
            .map(|d| DeviceDto {
                id: d.id,
                label: d.label,
                scope: d.scope.as_str().to_string(),
                created_at: d.created_at,
                last_seen_at: d.last_seen_at,
                revoked_at: d.revoked_at,
            })
            .collect(),
    }))
}

async fn revoke(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<axum::response::Response, ServerError> {
    require_manage_devices(&claims)?;

    // Self-revocation does not require step-up (it's identical to logout).
    if id != claims.sub {
        let code = headers
            .get(STEP_UP_HEADER)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                ApiError::forbidden(
                    "stepup_required",
                    "revoking another device requires a step-up confirmation",
                )
            })?;
        let ok = verify_step_up(
            &state,
            code.trim(),
            &claims.sub,
            REVOKE_OPERATION,
            Some(&id),
        )
        .await
        .map_err(|e| ServerError::Internal(e))?;
        if !ok {
            return Err(ApiError::forbidden(
                "stepup_invalid",
                "step-up code missing, expired, or for a different operation",
            )
            .into());
        }
    }

    state.auth.revoke_device(&id).await?;

    let now = unix_now();
    let _ = state
        .repo
        .record_audit(
            AuditEvent::success(now, claims.sub.clone(), AuditAction::DeviceRevoke)
                .with_target(id.clone()),
        )
        .await;

    Ok(StatusCode::NO_CONTENT.into_response())
}

fn require_manage_devices(claims: &pipa_core::device::AccessTokenClaims) -> Result<(), ApiError> {
    if !check_scope(claims, "manage", Some("devices")) {
        return Err(ApiError::forbidden(
            "insufficient_scope",
            "manage:devices scope required",
        ));
    }
    Ok(())
}

fn unix_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
