//! `/api/comments/*` and `/api/pages/:uuid/comments*` — public submit/list,
//! owner moderation, the widget asset, plus a tiny config toggle so owners
//! can enable comments per page without touching SQL directly.

use std::time::{SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete as delete_route, get, patch, post};
use pipa_core::audit::{AuditAction, AuditEvent};
use serde::{Deserialize, Serialize};

use crate::auth::AuthClaims;
use crate::error::{ApiError, ServerError};
use crate::routes::pages::util::{PageView, require_admin};
use crate::state::AppState;

pub mod moderation;
pub mod public;
pub mod sanitize;
pub mod widget;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/pages/:uuid/comments",
            get(public::list_comments).post(public::post_comment),
        )
        .route(
            "/api/pages/:uuid/comments/moderation",
            get(moderation::list_moderation),
        )
        .route(
            "/api/pages/:uuid/comments-config",
            post(set_comments_config),
        )
        .route(
            "/api/comments/:id",
            patch(moderation::patch_comment).delete(delete_route(moderation::delete_comment)),
        )
        .route("/api/comments/widget.js", get(widget::widget_js))
        .route("/api/comments/widget.css", get(widget::widget_css))
}

#[derive(Debug, Deserialize)]
pub struct CommentsConfigRequest {
    pub enabled: bool,
    #[serde(default)]
    pub require_approval: bool,
}

#[derive(Debug, Serialize)]
pub struct CommentsConfigResponse {
    pub page: PageView,
}

/// `POST /api/pages/:uuid/comments-config` — owner toggle for the per-page
/// `comments_enabled` / `comments_require_approval` flags. Lives in the
/// comments module (not pages) since it's part of the comments feature.
/// Authorization mirrors the moderation endpoints: `admin:<uuid>` or
/// `admin:*`. No step-up — toggling comments is reversible.
async fn set_comments_config(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    Path(uuid): Path<String>,
    Json(req): Json<CommentsConfigRequest>,
) -> Result<Response, ServerError> {
    require_admin(&claims, &uuid)?;
    let page = state
        .repo
        .find_page(&uuid)
        .await?
        .ok_or_else(|| ApiError::not_found("page_not_found", "no page with that uuid"))?;

    state
        .repo
        .enable_comments(&uuid, req.enabled, req.require_approval)
        .await?;

    let updated = state
        .repo
        .find_page(&uuid)
        .await?
        .ok_or_else(|| ApiError::not_found("page_not_found", "no page with that uuid"))?;

    let details = serde_json::json!({
        "enabled": req.enabled,
        "require_approval": req.require_approval,
        "previous_enabled": page.comments_enabled,
        "previous_require_approval": page.comments_require_approval,
    })
    .to_string();
    let _ = state
        .repo
        .record_audit(
            AuditEvent::success(unix_now(), claims.sub.clone(), AuditAction::PageUpdate)
                .with_target(uuid.clone())
                .with_scope(claims.scope.clone())
                .with_details(details),
        )
        .await;

    Ok((StatusCode::OK, Json(CommentsConfigResponse {
        page: PageView::from(&updated),
    }))
        .into_response())
}

pub(crate) fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
