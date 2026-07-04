//! Shared helpers for the `pages::*` handlers — response views, owner
//! constants, scope checks, unix time. Anything cross-cutting between deploy
//! / list / visibility / delete / stats lives here so handler files stay
//! focused on their single endpoint.

use std::time::{SystemTime, UNIX_EPOCH};

use pipa_core::device::AccessTokenClaims;
use pipa_core::{Access, Page};
use serde::Serialize;

use crate::auth::check_scope;
use crate::error::ApiError;
use crate::state::AppState;

/// The Phase-1 single-owner ("local") identity. It doubles as the server-
/// operator superuser: it acts on every page and lists all pages.
pub const OWNER_KIND_LOCAL: &str = "local";
pub const OWNER_ID_LOCAL: &str = "local";
/// Phase-4 workspace ownership. Pages a user deploys are owned by a workspace.
pub const OWNER_KIND_WORKSPACE: &str = "workspace";
/// Legacy Phase-3 direct user ownership; migration 0011 moves these to
/// workspaces, but the check tolerates any left behind.
pub const OWNER_KIND_USER: &str = "user";

/// Who is calling: the `local` server-operator superuser, or a specific `user`
/// (resolved from the caller's device → `user_id`).
#[derive(Debug, Clone)]
pub enum CallerIdentity {
    Local,
    User(String),
}

/// Resolve the caller from its access-token claims. `sub` is a device id; a
/// device linked to a user is that `User`, otherwise the single-owner `Local`.
pub async fn caller_identity(state: &AppState, claims: &AccessTokenClaims) -> CallerIdentity {
    let linked = state
        .auth
        .get_device(&claims.sub)
        .await
        .ok()
        .flatten()
        .and_then(|d| d.user_id);
    match linked {
        Some(uid) => CallerIdentity::User(uid),
        None => CallerIdentity::Local,
    }
}

/// The personal-workspace id for a user (deterministic `ws-<userid>`, matching
/// migration 0011 and `create_user`).
pub fn personal_workspace_id(user_id: &str) -> String {
    format!("ws-{user_id}")
}

/// Enforce access to an existing page after the scope check. `need_write` gates
/// deploy/delete/access-change (editor+) vs read/stats (viewer+). `local` is a
/// superuser. Returns 403 `not_owner` (caller isn't in the owning workspace) or
/// `insufficient_role` (member, but role too low).
pub async fn require_page_access(
    state: &AppState,
    caller: &CallerIdentity,
    page: &Page,
    need_write: bool,
) -> Result<(), ApiError> {
    let uid = match caller {
        CallerIdentity::Local => return Ok(()), // superuser
        CallerIdentity::User(uid) => uid,
    };

    match page.owner_kind.as_str() {
        OWNER_KIND_WORKSPACE => {
            let role = state
                .auth
                .get_member_role(&page.owner_id, uid)
                .await
                .map_err(|_| ApiError::internal("membership lookup failed"))?;
            match role {
                Some(r) if (need_write && r.can_write()) || (!need_write && r.can_read()) => Ok(()),
                Some(_) => Err(ApiError::forbidden(
                    "insufficient_role",
                    "your role in this workspace does not permit that action",
                )),
                None => Err(ApiError::forbidden(
                    "not_owner",
                    "you are not a member of this page's workspace",
                )),
            }
        }
        // Legacy direct user ownership (pre-migration remnants).
        OWNER_KIND_USER if &page.owner_id == uid => Ok(()),
        // `local`-owned pages belong to the operator; a user can't touch them.
        _ => Err(ApiError::forbidden("not_owner", "you do not own this page")),
    }
}

/// Resolve which owner a NEWLY created page belongs to. `local` → `("local",
/// "local")`. A user → the requested workspace (must be a member with write
/// rights), or their personal workspace when none is requested.
pub async fn resolve_create_owner(
    state: &AppState,
    caller: &CallerIdentity,
    requested_workspace: Option<&str>,
) -> Result<(String, String), ApiError> {
    let uid = match caller {
        CallerIdentity::Local => {
            return Ok((OWNER_KIND_LOCAL.to_string(), OWNER_ID_LOCAL.to_string()));
        }
        CallerIdentity::User(uid) => uid,
    };

    let ws_id = match requested_workspace {
        Some(w) if !w.is_empty() => w.to_string(),
        _ => personal_workspace_id(uid),
    };

    let role = state
        .auth
        .get_member_role(&ws_id, uid)
        .await
        .map_err(|_| ApiError::internal("membership lookup failed"))?;
    match role {
        Some(r) if r.can_write() => Ok((OWNER_KIND_WORKSPACE.to_string(), ws_id)),
        Some(_) => Err(ApiError::forbidden(
            "insufficient_role",
            "your role in that workspace does not permit deploying",
        )),
        None => Err(ApiError::forbidden(
            "not_owner",
            "you are not a member of that workspace",
        )),
    }
}

/// Enforce a workspace quota before creating a page. `added_bytes` is the size
/// of the incoming deploy. No-op for `local` or for unlimited (NULL) quotas.
pub async fn enforce_quota(
    state: &AppState,
    owner_kind: &str,
    owner_id: &str,
    added_bytes: u64,
) -> Result<(), ApiError> {
    if owner_kind != OWNER_KIND_WORKSPACE {
        return Ok(());
    }
    let Some(ws) = state
        .auth
        .get_workspace(owner_id)
        .await
        .map_err(|_| ApiError::internal("workspace lookup failed"))?
    else {
        return Ok(());
    };

    if let Some(max_pages) = ws.max_pages {
        let count = state
            .repo
            .count_pages_for_owner(owner_kind, owner_id)
            .await
            .map_err(|_| ApiError::internal("page count failed"))?;
        if count as i64 >= max_pages {
            return Err(ApiError::forbidden(
                "quota_exceeded",
                format!("workspace page limit reached ({max_pages})"),
            ));
        }
    }
    if let Some(max_bytes) = ws.max_bytes {
        let used = state
            .repo
            .sum_bytes_for_owner(owner_kind, owner_id)
            .await
            .map_err(|_| ApiError::internal("byte sum failed"))?;
        if used as i64 + added_bytes as i64 > max_bytes {
            return Err(ApiError::forbidden(
                "quota_exceeded",
                format!("workspace storage limit reached ({max_bytes} bytes)"),
            ));
        }
    }
    Ok(())
}

/// Public response shape for a page. Mirrors `core::Page` minus
/// `password_hash` — we never expose the argon2 string over the API even to
/// the owner.
#[derive(Debug, Serialize)]
pub struct PageView {
    pub uuid: String,
    pub name: Option<String>,
    pub mode: String,
    pub access: String,
    pub zone: String,
    pub owner_kind: String,
    pub owner_id: String,
    pub size_bytes: u64,
    pub file_count: u64,
    pub comments_enabled: bool,
    pub comments_require_approval: bool,
    pub csp: String,
    pub archived: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<&Page> for PageView {
    fn from(p: &Page) -> Self {
        Self {
            uuid: p.uuid.clone(),
            name: p.name.clone(),
            mode: p.mode.as_str().to_string(),
            access: p.access.as_str().to_string(),
            zone: p.zone.as_str().to_string(),
            owner_kind: p.owner_kind.clone(),
            owner_id: p.owner_id.clone(),
            size_bytes: p.size_bytes,
            file_count: p.file_count,
            comments_enabled: p.comments_enabled,
            comments_require_approval: p.comments_require_approval,
            csp: p.csp.as_str().to_string(),
            archived: p.archived,
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

/// Access label preserving the wire spelling.
pub fn access_str(a: Access) -> &'static str {
    a.as_str()
}

pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
