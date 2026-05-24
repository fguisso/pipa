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

use super::util::{OWNER_ID_LOCAL, OWNER_KIND_LOCAL, PageView, require_read};

#[derive(Debug, Serialize)]
pub struct ListResponse {
    pub pages: Vec<PageView>,
}

pub async fn list_pages(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
) -> Result<Json<ListResponse>, ServerError> {
    require_read_wildcard(&claims)?;
    let rows = state
        .repo
        .list_pages(OWNER_KIND_LOCAL, OWNER_ID_LOCAL)
        .await?;
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
