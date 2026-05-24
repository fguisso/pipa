//! Shared helpers for the `pages::*` handlers — response views, owner
//! constants, scope checks, unix time. Anything cross-cutting between deploy
//! / list / visibility / delete / stats lives here so handler files stay
//! focused on their single endpoint.

use std::time::{SystemTime, UNIX_EPOCH};

use pipa_core::device::AccessTokenClaims;
use pipa_core::{Page, Visibility};
use serde::Serialize;

use crate::auth::check_scope;
use crate::error::ApiError;

/// Phase-1 single-owner constants. Phase 3 will route these through claims.
pub const OWNER_KIND_LOCAL: &str = "local";
pub const OWNER_ID_LOCAL: &str = "local";

/// Public response shape for a page. Mirrors `core::Page` minus
/// `password_hash` — we never expose the argon2 string over the API even to
/// the owner.
#[derive(Debug, Serialize)]
pub struct PageView {
    pub uuid: String,
    pub name: Option<String>,
    pub mode: String,
    pub visibility: String,
    pub owner_kind: String,
    pub owner_id: String,
    pub size_bytes: u64,
    pub file_count: u64,
    pub comments_enabled: bool,
    pub comments_require_approval: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<&Page> for PageView {
    fn from(p: &Page) -> Self {
        Self {
            uuid: p.uuid.clone(),
            name: p.name.clone(),
            mode: p.mode.as_str().to_string(),
            visibility: p.visibility.as_str().to_string(),
            owner_kind: p.owner_kind.clone(),
            owner_id: p.owner_id.clone(),
            size_bytes: p.size_bytes,
            file_count: p.file_count,
            comments_enabled: p.comments_enabled,
            comments_require_approval: p.comments_require_approval,
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}

impl From<Page> for PageView {
    fn from(p: Page) -> Self {
        (&p).into()
    }
}

/// Require a read scope for `uuid`. Accepts `read:<uuid>` or `read:*`.
pub fn require_read(claims: &AccessTokenClaims, uuid: &str) -> Result<(), ApiError> {
    if check_scope(claims, "read", Some(uuid)) {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "insufficient_scope",
            format!("read:{uuid} (or read:*) scope required"),
        ))
    }
}

/// Require a destroy scope for `uuid`. Accepts `destroy:<uuid>` or `destroy:*`.
pub fn require_destroy(claims: &AccessTokenClaims, uuid: &str) -> Result<(), ApiError> {
    if check_scope(claims, "destroy", Some(uuid)) {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "insufficient_scope",
            format!("destroy:{uuid} scope required"),
        ))
    }
}

/// Require an admin scope for `uuid`. Accepts `admin:<uuid>` or `admin:*`.
pub fn require_admin(claims: &AccessTokenClaims, uuid: &str) -> Result<(), ApiError> {
    if check_scope(claims, "admin", Some(uuid)) {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "insufficient_scope",
            format!("admin:{uuid} scope required"),
        ))
    }
}

/// Visibility label preserving the wire spelling.
pub fn vis_str(v: Visibility) -> &'static str {
    v.as_str()
}

pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
