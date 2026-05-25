//! Public comment endpoints — no auth, but POST is rate-limited per (ip, page)
//! and per-server. GET returns only `visible` rows with owner-only fields
//! stripped. Per `SECURITY.md` §2 we return 404 (not 403) whenever the page
//! does not exist OR comments aren't enabled, so existence is never leaked.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use gapes_core::CommentStatus;
use gapes_core::audit::{AuditAction, AuditEvent};
use gapes_core::comment::{Comment, NewComment};
use gapes_core::ids::UlidGen;
use gapes_core::{IdGen, Page};
use serde::{Deserialize, Serialize};

use crate::error::{ApiError, ServerError};
use crate::ip_hash::{hmac_ip, hmac_value};
use crate::middleware::forwarded::RealIp;
use crate::middleware::rate_limit::RateLimitResult;
use crate::state::AppState;

use super::sanitize::markdown_to_safe_html;

/// Stripped-down public view: no `contact`, `ip_hash`, `user_agent`, `body_md`.
/// We hand-build this rather than serializing `Comment` so a future field on
/// the core type cannot accidentally leak.
#[derive(Debug, Serialize)]
pub struct PublicCommentView {
    pub id: String,
    pub author: String,
    pub html: String,
    pub ts: i64,
    pub anchor: PublicAnchorView,
}

#[derive(Debug, Serialize)]
pub struct PublicAnchorView {
    pub selector: String,
    pub text: String,
    pub offset: i64,
}

impl From<&Comment> for PublicCommentView {
    fn from(c: &Comment) -> Self {
        Self {
            id: c.id.clone(),
            author: c.author.clone(),
            html: c.body_html.clone(),
            ts: c.ts,
            anchor: PublicAnchorView {
                selector: c.anchor_selector.clone(),
                text: c.anchor_text.clone(),
                offset: c.anchor_offset,
            },
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ListResponse {
    pub comments: Vec<PublicCommentView>,
}

#[derive(Debug, Deserialize)]
pub struct CommentRequest {
    pub author: String,
    pub body: String,
    #[serde(default)]
    pub contact: Option<String>,
    pub anchor: AnchorRequest,
}

#[derive(Debug, Deserialize)]
pub struct AnchorRequest {
    pub selector: String,
    pub text: String,
    pub offset: i64,
}

#[derive(Debug, Serialize)]
pub struct CommentResponse {
    pub id: String,
    pub status: &'static str,
    pub html: String,
    pub ts: i64,
    pub anchor: PublicAnchorView,
}

pub async fn list_comments(
    State(state): State<AppState>,
    Path(uuid): Path<String>,
) -> Result<Response, ServerError> {
    let page = match state.repo.find_page(&uuid).await? {
        Some(p) if p.comments_enabled => p,
        _ => return Ok(not_found_resp()),
    };
    let rows = state.repo.list_comments(&page.uuid, false).await?;
    let body = ListResponse {
        comments: rows.iter().map(PublicCommentView::from).collect(),
    };
    Ok((StatusCode::OK, Json(body)).into_response())
}

pub async fn post_comment(
    State(state): State<AppState>,
    Path(uuid): Path<String>,
    real_ip: RealIp,
    headers: HeaderMap,
    Json(req): Json<CommentRequest>,
) -> Result<Response, ServerError> {
    let page: Page = match state.repo.find_page(&uuid).await? {
        Some(p) if p.comments_enabled => p,
        _ => return Ok(not_found_resp()),
    };

    let cfg = &state.config.comments;
    if !cfg.enabled {
        return Ok(not_found_resp());
    }

    let author = trim_and_cap(&req.author, cfg.max_author_length);
    let body_md = trim_and_cap(&req.body, cfg.max_body_length);
    if author.is_empty() || body_md.is_empty() {
        return Err(ApiError::bad_request(
            "invalid_comment",
            "author and body are required and must be non-empty after trimming",
        )
        .into());
    }
    let contact = req
        .contact
        .as_deref()
        .map(|s| trim_and_cap(s, 256))
        .filter(|s| !s.is_empty());

    let ip_hash = hmac_ip(&state, &real_ip.0);
    let now = super::unix_now();

    match state.comment_limiter.check(&ip_hash, &uuid, now) {
        RateLimitResult::Ok => {}
        RateLimitResult::Retry { after_secs } => {
            return Ok(too_many_resp(after_secs));
        }
    }

    let anchor_selector = trim_and_cap(&req.anchor.selector, 512);
    let anchor_text = trim_and_cap(&req.anchor.text, 500);
    if anchor_selector.is_empty() || anchor_text.is_empty() {
        return Err(ApiError::bad_request(
            "invalid_anchor",
            "anchor selector and text must be non-empty",
        )
        .into());
    }

    let body_html = markdown_to_safe_html(&body_md);
    let status = if page.comments_require_approval {
        CommentStatus::Pending
    } else {
        CommentStatus::Visible
    };

    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|ua| hmac_value(&state, ua));

    let id = UlidGen.new_ulid().to_string();
    let new_comment = NewComment {
        id: id.clone(),
        page_uuid: uuid.clone(),
        author: author.clone(),
        body_md: body_md.clone(),
        body_html: body_html.clone(),
        contact,
        ts: now,
        ip_hash: ip_hash.clone(),
        status,
        user_agent,
        anchor_selector: anchor_selector.clone(),
        anchor_text: anchor_text.clone(),
        anchor_offset: req.anchor.offset,
    };

    let saved = state.repo.create_comment(new_comment).await?;

    let _ = state
        .repo
        .record_audit(
            AuditEvent::success(now, "public".to_string(), AuditAction::CommentCreate)
                .with_target(saved.id.clone())
                .with_ip_hash(ip_hash),
        )
        .await;

    let public_html = match saved.status {
        CommentStatus::Visible => saved.body_html.clone(),
        _ => String::new(),
    };

    let resp = CommentResponse {
        id: saved.id.clone(),
        status: saved.status.as_str(),
        html: public_html,
        ts: saved.ts,
        anchor: PublicAnchorView {
            selector: saved.anchor_selector.clone(),
            text: saved.anchor_text.clone(),
            offset: saved.anchor_offset,
        },
    };
    Ok((StatusCode::OK, Json(resp)).into_response())
}

fn trim_and_cap(s: &str, max: usize) -> String {
    let trimmed = s.trim();
    if trimmed.chars().count() <= max {
        return trimmed.to_string();
    }
    trimmed.chars().take(max).collect()
}

fn not_found_resp() -> Response {
    ApiError::not_found("not_found", "no such page").into_response()
}

fn too_many_resp(after_secs: u64) -> Response {
    let mut resp = ApiError::new(
        StatusCode::TOO_MANY_REQUESTS,
        "rate_limited",
        format!("comment rate limit exceeded; retry in {after_secs}s"),
    )
    .into_response();
    if let Ok(val) = axum::http::HeaderValue::from_str(&after_secs.to_string()) {
        resp.headers_mut().insert("retry-after", val);
    }
    resp
}

