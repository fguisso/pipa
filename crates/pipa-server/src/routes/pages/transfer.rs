//! `POST /api/pages/:uuid/transfer` — move a page to another workspace.
//!
//! The caller must be able to write the page in its current home (editor+ or the
//! `local` superuser) AND be an editor+ member of the destination workspace
//! (the superuser may target any workspace). The destination quota is enforced.

use axum::Json;
use axum::extract::{Path, State};
use pipa_core::audit::{AuditAction, AuditEvent};
use serde::Deserialize;

use crate::auth::AuthClaims;
use crate::error::{ApiError, ServerError};
use crate::state::AppState;

use super::util::{
    CallerIdentity, OWNER_KIND_WORKSPACE, PageView, caller_identity, enforce_quota, require_admin,
    require_page_access, unix_now,
};

#[derive(Debug, Deserialize)]
pub struct TransferReq {
    /// Destination workspace id.
    pub workspace: String,
}

pub async fn transfer_page(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    Path(uuid): Path<String>,
    Json(body): Json<TransferReq>,
) -> Result<Json<PageView>, ServerError> {
    require_admin(&claims, &uuid)?;

    let page = state
        .repo
        .find_page(&uuid)
        .await?
        .ok_or_else(|| ApiError::not_found("page_not_found", "no page with that uuid"))?;

    let caller = caller_identity(&state, &claims).await;
    // Must be able to write the page where it lives now.
    require_page_access(&state, &caller, &page, true).await?;

    let target = body.workspace.trim();
    if target.is_empty() {
        return Err(ApiError::bad_request("invalid_workspace", "workspace is required").into());
    }

    // Must be allowed to place pages in the destination.
    match &caller {
        CallerIdentity::Local => {
            state
                .auth
                .get_workspace(target)
                .await?
                .ok_or_else(|| ApiError::not_found("workspace_not_found", "no such workspace"))?;
        }
        CallerIdentity::User(uid) => {
            let role = state.auth.get_member_role(target, uid).await?;
            match role {
                Some(r) if r.can_write() => {}
                Some(_) => {
                    return Err(ApiError::forbidden(
                        "insufficient_role",
                        "your role in the destination workspace does not permit receiving pages",
                    )
                    .into());
                }
                None => {
                    return Err(ApiError::forbidden(
                        "not_owner",
                        "you are not a member of the destination workspace",
                    )
                    .into());
                }
            }
        }
    }

    enforce_quota(&state, OWNER_KIND_WORKSPACE, target, page.size_bytes).await?;

    state
        .repo
        .transfer_page(&uuid, OWNER_KIND_WORKSPACE, target)
        .await?;

    let _ = state
        .repo
        .record_audit(
            AuditEvent::success(unix_now(), claims.sub.clone(), AuditAction::PageUpdate)
                .with_target(uuid.clone())
                .with_scope(claims.scope.clone())
                .with_details(format!("transfer to workspace {target}")),
        )
        .await;

    let updated = state
        .repo
        .find_page(&uuid)
        .await?
        .ok_or_else(|| ApiError::not_found("page_not_found", "page vanished after transfer"))?;
    Ok(Json(PageView::from(&updated)))
}
