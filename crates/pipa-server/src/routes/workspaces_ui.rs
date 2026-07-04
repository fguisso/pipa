//! Phase 4 workspace web UI (`/workspaces`). Session-authenticated via the
//! `pipa_user` cookie (`CurrentUser`); a signed-in user sees the workspaces they
//! belong to, can create one, and — where they're `admin`+ — manage members.
//! All writes are role-checked server-side; plain POST forms so it works with or
//! without Alpine.

use askama::Template;
use axum::Router;
use axum::body::Body;
use axum::extract::{Form, Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use pipa_core::workspace::{NewWorkspace, WorkspaceKind, WorkspaceRole};
use serde::Deserialize;

use crate::auth::user_cookie::CurrentUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/workspaces", get(page).post(create))
        .route("/workspaces/:id/members", post(add_member))
        .route("/workspaces/:id/members/:user_id/remove", post(remove_member))
        .route("/workspaces/:id/members/:user_id/role", post(set_role))
}

struct MemView {
    user_id: String,
    username: String,
    role: String,
}

struct WsView {
    id: String,
    name: String,
    kind: String,
    role: String,
    quota: String,
    can_manage: bool,
    members: Vec<MemView>,
}

#[derive(Template)]
#[template(path = "workspaces.html")]
struct WorkspacesTemplate {
    username: String,
    workspaces: Vec<WsView>,
}

pub async fn page(State(state): State<AppState>, current: CurrentUser) -> Response {
    let uid = current.user.id.clone();
    let memberships = state
        .auth
        .list_workspaces_for_user(&uid)
        .await
        .unwrap_or_default();

    let mut views = Vec::new();
    for m in memberships {
        let can_manage = m.role.can_manage_members();
        let members = if can_manage {
            state
                .auth
                .list_members(&m.workspace.id)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|mv| MemView {
                    user_id: mv.user_id,
                    username: mv.username,
                    role: mv.role.as_str().to_string(),
                })
                .collect()
        } else {
            Vec::new()
        };
        let quota = match (m.workspace.max_pages, m.workspace.max_bytes) {
            (None, None) => "unlimited".to_string(),
            (p, b) => format!(
                "pages {} · bytes {}",
                p.map(|v| v.to_string()).unwrap_or_else(|| "∞".into()),
                b.map(|v| v.to_string()).unwrap_or_else(|| "∞".into()),
            ),
        };
        views.push(WsView {
            id: m.workspace.id,
            name: m.workspace.name,
            kind: m.workspace.kind.as_str().to_string(),
            role: m.role.as_str().to_string(),
            quota,
            can_manage,
            members,
        });
    }

    render(WorkspacesTemplate {
        username: current.user.username,
        workspaces: views,
    })
}

#[derive(Deserialize)]
pub struct CreateForm {
    name: String,
}

pub async fn create(
    State(state): State<AppState>,
    current: CurrentUser,
    Form(form): Form<CreateForm>,
) -> Response {
    let name = form.name.trim();
    if !name.is_empty() {
        let _ = state
            .auth
            .create_workspace(
                NewWorkspace {
                    name: name.to_string(),
                    kind: WorkspaceKind::Team,
                    max_pages: None,
                    max_bytes: None,
                },
                &current.user.id,
            )
            .await;
    }
    Redirect::to("/workspaces").into_response()
}

/// 403 unless the signed-in user is `admin`+ in the workspace.
async fn manage_ok(state: &AppState, ws_id: &str, uid: &str) -> bool {
    matches!(
        state.auth.get_member_role(ws_id, uid).await.ok().flatten(),
        Some(r) if r.can_manage_members()
    )
}

#[derive(Deserialize)]
pub struct MemberForm {
    username: String,
    role: String,
}

pub async fn add_member(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<String>,
    Form(form): Form<MemberForm>,
) -> Response {
    if !manage_ok(&state, &id, &current.user.id).await {
        return (StatusCode::FORBIDDEN, "insufficient role").into_response();
    }
    if let (Ok(role), Ok(Some(user))) = (
        role_from(&form.role),
        state.auth.find_user_by_username(form.username.trim()).await,
    ) {
        let _ = state.auth.add_member(&id, &user.id, role).await;
    }
    Redirect::to("/workspaces").into_response()
}

#[derive(Deserialize)]
pub struct RoleForm {
    role: String,
}

pub async fn set_role(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((id, user_id)): Path<(String, String)>,
    Form(form): Form<RoleForm>,
) -> Response {
    if !manage_ok(&state, &id, &current.user.id).await {
        return (StatusCode::FORBIDDEN, "insufficient role").into_response();
    }
    if let Ok(role) = role_from(&form.role) {
        if role != WorkspaceRole::Owner && is_last_owner(&state, &id, &user_id).await {
            return (StatusCode::BAD_REQUEST, "cannot demote the last owner").into_response();
        }
        let _ = state.auth.update_member_role(&id, &user_id, role).await;
    }
    Redirect::to("/workspaces").into_response()
}

pub async fn remove_member(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((id, user_id)): Path<(String, String)>,
) -> Response {
    if !manage_ok(&state, &id, &current.user.id).await {
        return (StatusCode::FORBIDDEN, "insufficient role").into_response();
    }
    if is_last_owner(&state, &id, &user_id).await {
        return (StatusCode::BAD_REQUEST, "cannot remove the last owner").into_response();
    }
    let _ = state.auth.remove_member(&id, &user_id).await;
    Redirect::to("/workspaces").into_response()
}

async fn is_last_owner(state: &AppState, ws_id: &str, user_id: &str) -> bool {
    let members = state.auth.list_members(ws_id).await.unwrap_or_default();
    let owners: Vec<_> = members
        .iter()
        .filter(|m| m.role == WorkspaceRole::Owner)
        .collect();
    owners.len() == 1 && owners[0].user_id == user_id
}

fn role_from(s: &str) -> Result<WorkspaceRole, ()> {
    s.parse().map_err(|_| ())
}

fn render<T: Template>(t: T) -> Response {
    match t.render() {
        Ok(body) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(body))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        Err(e) => {
            tracing::error!(error = %e, "render workspaces template");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
