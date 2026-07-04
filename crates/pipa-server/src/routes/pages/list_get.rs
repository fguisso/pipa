//! `GET /api/pages` — list the caller's pages (Phase 1: the single local
//! owner). `GET /api/pages/:uuid` — single page metadata.
//!
//! Both require a `read:*` (list) or `read:<uuid>` / `read:*` (get) scope.
//! Returns 404 for unknown UUIDs.

use axum::Json;
use axum::extract::{Path, State};
use pipa_core::device::AccessTokenClaims;
use serde::Serialize;

use crate::auth::{AuthClaims, check_scope};
use crate::error::{ApiError, ServerError};
use crate::state::AppState;

use super::util::{CallerIdentity, PageView, caller_identity, require_page_access, require_read};

#[derive(Debug, Serialize)]
pub struct ListResponse {
    pub pages: Vec<PageView>,
}

pub async fn list_pages(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
) -> Result<Json<ListResponse>, ServerError> {
    require_read_wildcard(&claims)?;
    // `local` (the operator superuser) lists every page; a user lists pages
    // across all workspaces they belong to, newest first.
    let caller = caller_identity(&state, &claims).await;
    let rows = match caller {
        CallerIdentity::Local => state.repo.list_all_pages().await?,
        CallerIdentity::User(uid) => {
            let memberships = state.auth.list_workspaces_for_user(&uid).await?;
            let mut pages = Vec::new();
            for m in memberships {
                let mut p = state.repo.list_pages("workspace", &m.workspace.id).await?;
                pages.append(&mut p);
            }
            pages.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            pages
        }
    };
    Ok(Json(ListResponse {
        pages: rows.iter().map(PageView::from).collect(),
    }))
}

pub async fn get_page(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    Path(uuid): Path<String>,
) -> Result<Json<PageView>, ServerError> {
    require_read(&claims, &uuid)?;
    let page = state
        .repo
        .find_page(&uuid)
        .await?
        .ok_or_else(|| ApiError::not_found("page_not_found", "no page with that uuid"))?;
    let caller = caller_identity(&state, &claims).await;
    require_page_access(&state, &caller, &page, false).await?;
    Ok(Json(PageView::from(&page)))
}

fn require_read_wildcard(claims: &AccessTokenClaims) -> Result<(), ApiError> {
    // Listing is intentionally narrower than per-page reads — we don't want a
    // `read:<uuid>` token to enumerate the rest of the owner's pages.
    if check_scope(claims, "read", Some("*")) {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "insufficient_scope",
            "read:* scope required to list pages",
        ))
    }
}
