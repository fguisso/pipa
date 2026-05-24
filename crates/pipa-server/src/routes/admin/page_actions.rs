//! Admin-only mutations: page archive/delete + device revoke.
//!
//! These bypass the step-up dance the CLI variants require. The trust
//! anchor here is the `pipa_owner` cookie + a valid `AdminSession` — i.e.,
//! the user already proved they're the admin by signing in to this browser.
//! Destructive buttons are fronted by explicit confirm modals in the UI.
//!
//! Endpoints:
//!   POST   /api/admin/pages/:uuid/archive    body: `{ "archived": true|false }`
//!   DELETE /api/admin/pages/:uuid
//!   DELETE /api/admin/devices/:id

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use axum::routing::{delete as delete_route, post};
use pipa_core::audit::{AuditAction, AuditEvent};
use serde::Deserialize;

use crate::error::{ApiError, ServerError};
use crate::state::AppState;

use super::session::AdminSession;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/pages/:uuid/archive", post(archive))
        .route("/api/admin/pages/:uuid", delete_route(delete_page))
        .route("/api/admin/devices/:id", delete_route(revoke_device))
}

#[derive(Debug, Deserialize)]
pub struct ArchiveBody {
    pub archived: bool,
}

async fn archive(
    State(state): State<AppState>,
    session: AdminSession,
    Path(uuid): Path<String>,
    Json(body): Json<ArchiveBody>,
) -> Result<Response, ServerError> {
    let page = state.repo.find_page(&uuid).await?;
    if page.is_none() {
        return Err(ApiError::not_found("page_not_found", "no page with that uuid").into());
    }

    state.repo.set_page_archived(&uuid, body.archived).await?;

    let action = if body.archived {
        "page.archive"
    } else {
        "page.unarchive"
    };
    let _ = state
        .repo
        .record_audit(
            AuditEvent::success(unix_now(), session.device.id.clone(), AuditAction::PageUpdate)
                .with_target(uuid.clone())
                .with_scope("admin_ui".to_string())
                .with_details(action.to_string()),
        )
        .await;

    let updated = state.repo.find_page(&uuid).await?;
    Ok(Json(updated).into_response())
}

async fn delete_page(
    State(state): State<AppState>,
    session: AdminSession,
    Path(uuid): Path<String>,
) -> Result<Response, ServerError> {
    if state.repo.find_page(&uuid).await?.is_none() {
        return Err(ApiError::not_found("page_not_found", "no page with that uuid").into());
    }

    state.storage.delete_page(&uuid).await?;
    state.repo.delete_page(&uuid).await?;

    let _ = state
        .repo
        .record_audit(
            AuditEvent::success(unix_now(), session.device.id.clone(), AuditAction::PageDelete)
                .with_target(uuid.clone())
                .with_scope("admin_ui".to_string()),
        )
        .await;

    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn revoke_device(
    State(state): State<AppState>,
    session: AdminSession,
    Path(id): Path<String>,
) -> Result<Response, ServerError> {
    // Refuse to revoke the admin's own synthetic device — that's the device
    // backing this very session. Self-logout is what the admin logout form
    // is for.
    if id == session.device.id {
        return Err(ApiError::bad_request(
            "self_synthetic",
            "cannot revoke the admin UI's own device — use logout instead",
        )
        .into());
    }

    state.auth.revoke_device(&id).await?;

    let _ = state
        .repo
        .record_audit(
            AuditEvent::success(unix_now(), session.device.id.clone(), AuditAction::DeviceRevoke)
                .with_target(id.clone())
                .with_scope("admin_ui".to_string()),
        )
        .await;

    Ok(StatusCode::NO_CONTENT.into_response())
}

fn unix_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
