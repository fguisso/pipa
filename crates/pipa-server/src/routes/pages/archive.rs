//! `POST /api/pages/:uuid/archive` — archive or restore a page without
//! deleting its bundle. Requires `admin:<uuid>` and page ownership; unlike
//! delete, this reversible operation does not require a step-up confirmation.

use axum::Json;
use axum::extract::{Path, State};
use pipa_core::audit::{AuditAction, AuditEvent};
use serde::Deserialize;

use crate::auth::AuthClaims;
use crate::error::{ApiError, ServerError};
use crate::state::AppState;

use super::util::{PageView, caller_identity, require_admin, require_page_access, unix_now};

#[derive(Debug, Deserialize)]
pub struct ArchiveBody {
    pub archived: bool,
}

pub async fn set_archive(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    Path(uuid): Path<String>,
    Json(body): Json<ArchiveBody>,
) -> Result<Json<PageView>, ServerError> {
    require_admin(&claims, &uuid)?;

    let page = state
        .repo
        .find_page(&uuid)
        .await?
        .ok_or_else(|| ApiError::not_found("page_not_found", "no page with that uuid"))?;
    let caller = caller_identity(&state, &claims).await;
    require_page_access(&state, &caller, &page, true).await?;

    state.repo.set_page_archived(&uuid, body.archived).await?;

    let action = if body.archived {
        "page.archive"
    } else {
        "page.unarchive"
    };
    let _ = state
        .repo
        .record_audit(
            AuditEvent::success(unix_now(), claims.sub.clone(), AuditAction::PageUpdate)
                .with_target(uuid.clone())
                .with_scope(claims.scope.clone())
                .with_details(action.to_string()),
        )
        .await;

    let updated = state
        .repo
        .find_page(&uuid)
        .await?
        .ok_or_else(|| ApiError::not_found("page_not_found", "no page with that uuid"))?;
    Ok(Json(PageView::from(updated)))
}
