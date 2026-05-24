//! `POST /api/auth/logout` — revoke the device behind the current bearer
//! token. Cascades to all refresh tokens for that device (see SqliteAuthStore
//! `revoke_device`).

use axum::extract::State;
use axum::http::StatusCode;
use pipa_core::audit::{AuditAction, AuditEvent};

use crate::auth::AuthClaims;
use crate::error::ServerError;
use crate::state::AppState;

pub async fn logout(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
) -> Result<StatusCode, ServerError> {
    state.auth.revoke_device(&claims.sub).await?;
    let now = chrono_like_now();
    let _ = state
        .repo
        .record_audit(AuditEvent::success(now, claims.sub.clone(), AuditAction::AuthRevoke))
        .await;
    Ok(StatusCode::NO_CONTENT)
}

fn chrono_like_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
