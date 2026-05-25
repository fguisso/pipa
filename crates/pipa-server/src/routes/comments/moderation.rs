//! Owner-only moderation endpoints. Authorization decision:
//!   - GET moderation, PATCH, DELETE all require `admin:<page_uuid>`.
//!   - DELETE does NOT require step-up — moderation is reversible by simply
//!     re-submitting (or hiding instead). Step-up is reserved for genuinely
//!     destructive page-level operations (see SECURITY.md §3).
//!
//! Moderation responses include the full owner-visible fields, but still
//! omit `ip_hash` — that field is server-internal even for owners (matches
//! the privacy posture for hits).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use pipa_core::CommentStatus;
use pipa_core::audit::{AuditAction, AuditEvent};
use pipa_core::comment::Comment;
use serde::{Deserialize, Serialize};

use crate::auth::AuthClaims;
use crate::error::{ApiError, ServerError};
use crate::routes::pages::util::require_admin;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct OwnerCommentView {
    pub id: String,
    pub page_uuid: String,
    pub author: String,
    pub body_md: String,
    pub html: String,
    pub contact: Option<String>,
    pub ts: i64,
    pub status: &'static str,
    pub user_agent: Option<String>,
    pub anchor_selector: String,
    pub anchor_text: String,
    pub anchor_offset: i64,
}

impl From<&Comment> for OwnerCommentView {
    fn from(c: &Comment) -> Self {
        Self {
            id: c.id.clone(),
            page_uuid: c.page_uuid.clone(),
            author: c.author.clone(),
            body_md: c.body_md.clone(),
            html: c.body_html.clone(),
            contact: c.contact.clone(),
            ts: c.ts,
            status: c.status.as_str(),
            user_agent: c.user_agent.clone(),
            anchor_selector: c.anchor_selector.clone(),
            anchor_text: c.anchor_text.clone(),
            anchor_offset: c.anchor_offset,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ModerationListResponse {
    pub comments: Vec<OwnerCommentView>,
}

pub async fn list_moderation(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    Path(uuid): Path<String>,
) -> Result<Response, ServerError> {
    require_admin(&claims, &uuid)?;
    if state.repo.find_page(&uuid).await?.is_none() {
        return Err(ApiError::not_found("page_not_found", "no page with that uuid").into());
    }
    let rows = state.repo.list_comments(&uuid, true).await?;
    let body = ModerationListResponse {
        comments: rows.iter().map(OwnerCommentView::from).collect(),
    };
    Ok((StatusCode::OK, Json(body)).into_response())
}

#[derive(Debug, Deserialize)]
pub struct PatchCommentRequest {
    pub status: String,
}

pub async fn patch_comment(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    Path(id): Path<String>,
    Json(req): Json<PatchCommentRequest>,
) -> Result<Response, ServerError> {
    let new_status: CommentStatus = req.status.parse().map_err(|_| {
        ApiError::bad_request(
            "invalid_status",
            "status must be visible|pending|hidden",
        )
    })?;

    let existing = state
        .repo
        .find_comment(&id)
        .await?
        .ok_or_else(|| ApiError::not_found("comment_not_found", "no comment with that id"))?;

    require_admin(&claims, &existing.page_uuid)?;

    state.repo.set_comment_status(&id, new_status).await?;

    let updated = state
        .repo
        .find_comment(&id)
        .await?
        .ok_or_else(|| ApiError::not_found("comment_not_found", "no comment with that id"))?;

    let action = match new_status {
        CommentStatus::Visible => AuditAction::CommentApprove,
        CommentStatus::Hidden => AuditAction::CommentHide,
        // Reverting to pending isn't a first-class verb; record as approve so
        // the audit log still tells the story (status changed under an admin).
        CommentStatus::Pending => AuditAction::CommentApprove,
    };
    let _ = state
        .repo
        .record_audit(
            AuditEvent::success(super::unix_now(), claims.sub.clone(), action)
                .with_target(id.clone())
                .with_scope(claims.scope.clone()),
        )
        .await;

    Ok((StatusCode::OK, Json(OwnerCommentView::from(&updated))).into_response())
}

pub async fn delete_comment(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    Path(id): Path<String>,
) -> Result<Response, ServerError> {
    let existing = state
        .repo
        .find_comment(&id)
        .await?
        .ok_or_else(|| ApiError::not_found("comment_not_found", "no comment with that id"))?;
    require_admin(&claims, &existing.page_uuid)?;

    state.repo.delete_comment(&id).await?;

    let _ = state
        .repo
        .record_audit(
            AuditEvent::success(super::unix_now(), claims.sub.clone(), AuditAction::CommentDelete)
                .with_target(id.clone())
                .with_scope(claims.scope.clone()),
        )
        .await;

    Ok(StatusCode::NO_CONTENT.into_response())
}
