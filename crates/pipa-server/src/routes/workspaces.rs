//! `/api/workspaces/*` — workspace + membership management for the CLI
//! (`pipa workspace …`). Authenticated by a bearer access token; the caller is
//! resolved to a `user` via its device link. The single-owner `local` operator
//! is not a workspace member, so these endpoints require a user account.
//!
//! Role gates: any member may read; `admin`+ manages members and quotas; the
//! `owner` is created with the workspace. A workspace's last `owner` can't be
//! removed or demoted, so a workspace is never orphaned.

use std::str::FromStr;

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::routing::{delete as delete_route, get, post};
use pipa_core::workspace::{
    NewWorkspace, Workspace, WorkspaceKind, WorkspaceMemberView, WorkspaceMembership, WorkspaceRole,
};
use serde::{Deserialize, Serialize};

use crate::auth::AuthClaims;
use crate::error::{ApiError, ServerError};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/workspaces", get(list).post(create))
        .route("/api/workspaces/:id", get(get_workspace))
        .route("/api/workspaces/:id/members", post(add_member))
        .route(
            "/api/workspaces/:id/members/:user_id",
            delete_route(remove_member),
        )
        .route("/api/workspaces/:id/members/:user_id/role", post(set_role))
        .route("/api/workspaces/:id/quota", post(set_quota))
}

/// Resolve the caller to a user id, or 403 — `local` has no workspaces.
async fn require_user(state: &AppState, claims: &AuthClaims) -> Result<String, ApiError> {
    let uid = state
        .auth
        .get_device(&claims.0.sub)
        .await
        .map_err(|_| ApiError::internal("device lookup failed"))?
        .and_then(|d| d.user_id);
    uid.ok_or_else(|| {
        ApiError::forbidden(
            "not_a_user",
            "workspace commands require a signed-in user account",
        )
    })
}

/// Enforce that `uid` has at least `min` role in `ws_id`.
async fn require_role(
    state: &AppState,
    ws_id: &str,
    uid: &str,
    min: WorkspaceRole,
) -> Result<WorkspaceRole, ApiError> {
    let role = state
        .auth
        .get_member_role(ws_id, uid)
        .await
        .map_err(|_| ApiError::internal("membership lookup failed"))?;
    match role {
        Some(r) if r.rank() >= min.rank() => Ok(r),
        Some(_) => Err(ApiError::forbidden(
            "insufficient_role",
            "your role does not permit that action",
        )),
        None => Err(ApiError::forbidden(
            "not_owner",
            "you are not a member of this workspace",
        )),
    }
}

// GET /api/workspaces
pub async fn list(
    State(state): State<AppState>,
    claims: AuthClaims,
) -> Result<Json<Vec<WorkspaceMembership>>, ServerError> {
    let uid = require_user(&state, &claims).await?;
    let rows = state.auth.list_workspaces_for_user(&uid).await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct CreateReq {
    pub name: String,
}

// POST /api/workspaces
pub async fn create(
    State(state): State<AppState>,
    claims: AuthClaims,
    Json(body): Json<CreateReq>,
) -> Result<Json<Workspace>, ServerError> {
    let uid = require_user(&state, &claims).await?;
    let name = body.name.trim();
    if name.is_empty() {
        return Err(ApiError::bad_request("invalid_name", "name is required").into());
    }
    let ws = state
        .auth
        .create_workspace(
            NewWorkspace {
                name: name.to_string(),
                kind: WorkspaceKind::Team,
                max_pages: None,
                max_bytes: None,
            },
            &uid,
        )
        .await?;
    Ok(Json(ws))
}

#[derive(Debug, Serialize)]
pub struct WorkspaceDetail {
    pub workspace: Workspace,
    pub my_role: WorkspaceRole,
    pub members: Vec<WorkspaceMemberView>,
}

// GET /api/workspaces/:id
pub async fn get_workspace(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<String>,
) -> Result<Json<WorkspaceDetail>, ServerError> {
    let uid = require_user(&state, &claims).await?;
    let my_role = require_role(&state, &id, &uid, WorkspaceRole::Viewer).await?;
    let workspace = state
        .auth
        .get_workspace(&id)
        .await?
        .ok_or_else(|| ApiError::not_found("workspace_not_found", "no such workspace"))?;
    let members = state.auth.list_members(&id).await?;
    Ok(Json(WorkspaceDetail {
        workspace,
        my_role,
        members,
    }))
}

#[derive(Debug, Deserialize)]
pub struct AddMemberReq {
    pub username: String,
    pub role: String,
}

// POST /api/workspaces/:id/members
pub async fn add_member(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<String>,
    Json(body): Json<AddMemberReq>,
) -> Result<Json<Vec<WorkspaceMemberView>>, ServerError> {
    let uid = require_user(&state, &claims).await?;
    require_role(&state, &id, &uid, WorkspaceRole::Admin).await?;
    let role = WorkspaceRole::from_str(&body.role)
        .map_err(|_| ApiError::bad_request("invalid_role", "role must be owner|admin|editor|viewer"))?;
    let target = state
        .auth
        .find_user_by_username(body.username.trim())
        .await?
        .ok_or_else(|| ApiError::not_found("user_not_found", "no user with that username"))?;
    state.auth.add_member(&id, &target.id, role).await?;
    let members = state.auth.list_members(&id).await?;
    Ok(Json(members))
}

#[derive(Debug, Deserialize)]
pub struct RoleReq {
    pub role: String,
}

// POST /api/workspaces/:id/members/:user_id/role
pub async fn set_role(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path((id, target_id)): Path<(String, String)>,
    Json(body): Json<RoleReq>,
) -> Result<Json<Vec<WorkspaceMemberView>>, ServerError> {
    let uid = require_user(&state, &claims).await?;
    require_role(&state, &id, &uid, WorkspaceRole::Admin).await?;
    let role = WorkspaceRole::from_str(&body.role)
        .map_err(|_| ApiError::bad_request("invalid_role", "role must be owner|admin|editor|viewer"))?;
    // Never leave a workspace ownerless.
    if role != WorkspaceRole::Owner {
        guard_last_owner(&state, &id, &target_id).await?;
    }
    state.auth.update_member_role(&id, &target_id, role).await?;
    let members = state.auth.list_members(&id).await?;
    Ok(Json(members))
}

// DELETE /api/workspaces/:id/members/:user_id
pub async fn remove_member(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path((id, target_id)): Path<(String, String)>,
) -> Result<Json<Vec<WorkspaceMemberView>>, ServerError> {
    let uid = require_user(&state, &claims).await?;
    require_role(&state, &id, &uid, WorkspaceRole::Admin).await?;
    guard_last_owner(&state, &id, &target_id).await?;
    state.auth.remove_member(&id, &target_id).await?;
    let members = state.auth.list_members(&id).await?;
    Ok(Json(members))
}

#[derive(Debug, Deserialize)]
pub struct QuotaReq {
    pub max_pages: Option<i64>,
    pub max_bytes: Option<i64>,
}

// POST /api/workspaces/:id/quota
pub async fn set_quota(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<String>,
    Json(body): Json<QuotaReq>,
) -> Result<Json<Workspace>, ServerError> {
    let uid = require_user(&state, &claims).await?;
    require_role(&state, &id, &uid, WorkspaceRole::Admin).await?;
    state
        .auth
        .set_workspace_quota(&id, body.max_pages, body.max_bytes)
        .await?;
    let ws = state
        .auth
        .get_workspace(&id)
        .await?
        .ok_or_else(|| ApiError::not_found("workspace_not_found", "no such workspace"))?;
    Ok(Json(ws))
}

/// Refuse an operation that would demote/remove the workspace's only owner.
async fn guard_last_owner(state: &AppState, ws_id: &str, target_id: &str) -> Result<(), ApiError> {
    let members = state
        .auth
        .list_members(ws_id)
        .await
        .map_err(|_| ApiError::internal("member lookup failed"))?;
    let owners: Vec<&WorkspaceMemberView> = members
        .iter()
        .filter(|m| m.role == WorkspaceRole::Owner)
        .collect();
    let target_is_only_owner =
        owners.len() == 1 && owners[0].user_id == target_id;
    if target_is_only_owner {
        return Err(ApiError::bad_request(
            "last_owner",
            "a workspace must keep at least one owner — promote someone else first",
        ));
    }
    Ok(())
}
