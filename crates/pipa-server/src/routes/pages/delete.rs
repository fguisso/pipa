//! `DELETE /api/pages/:uuid` — soft-delete the page (move to trash, drop the
//! row, which cascade-deletes hits + comments via FK). Requires a fresh
//! step-up confirmation (`X-Stepup-Code`) bound to `(device, page.delete,
//! uuid)`.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use pipa_core::audit::{AuditAction, AuditEvent};

use crate::auth::{AuthClaims, verify_step_up};
use crate::error::{ApiError, ServerError};
use crate::state::AppState;

use super::util::{require_destroy, unix_now};

const STEP_UP_HEADER: &str = "x-stepup-code";
const DELETE_OPERATION: &str = "page.delete";

pub async fn delete_page(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    Path(uuid): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ServerError> {
    require_destroy(&claims, &uuid)?;

    if state.repo.find_page(&uuid).await?.is_none() {
        return Err(ApiError::not_found("page_not_found", "no page with that uuid").into());
    }

    let code = headers
        .get(STEP_UP_HEADER)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            ApiError::forbidden(
                "step_up_required",
                "deleting a page requires a step-up confirmation",
            )
        })?;

    let ok = verify_step_up(&state, code.trim(), &claims.sub, DELETE_OPERATION, Some(&uuid))
        .await
        .map_err(ServerError::Internal)?;
    if !ok {
        return Err(ApiError::forbidden(
            "step_up_invalid",
            "step-up code missing, expired, or for a different operation",
        )
        .into());
    }

    state.storage.delete_page(&uuid).await?;
    state.repo.delete_page(&uuid).await?;

    let _ = state
        .repo
        .record_audit(
            AuditEvent::success(unix_now(), claims.sub.clone(), AuditAction::PageDelete)
                .with_target(uuid.clone())
                .with_scope(claims.scope.clone()),
        )
        .await;

    Ok(StatusCode::NO_CONTENT.into_response())
}
