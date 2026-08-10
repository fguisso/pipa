//! Phase 3/4 user dashboard (`/dashboard`).
//!
//! A signed-in user (the `pipa_user` cookie via `CurrentUser`) sees the sites
//! they can reach — grouped by workspace, newest first — with their role in
//! each. It is deliberately **read-only in the browser**: like the admin
//! console, destructive/visibility changes route through the CLI (they need the
//! second-device step-up flow), so we render copy-paste `pipa` commands instead
//! of mutating from a single tab. No bearer token is handed to the browser;
//! everything is rendered server-side from the session, so there is no
//! caller-identity surface to spoof.

use std::time::{SystemTime, UNIX_EPOCH};

use askama::Template;
use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;

use crate::auth::user_cookie::CurrentUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/dashboard", get(dashboard_page))
}

struct PageRow {
    uuid: String,
    short_uuid: String,
    name: String,
    access: String,
    zone: String,
    archived: bool,
    size: String,
    files: u64,
    updated: String,
}

struct WorkspaceRow {
    name: String,
    role: String,
    pages: Vec<PageRow>,
}

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate {
    username: String,
    workspaces: Vec<WorkspaceRow>,
    total_pages: usize,
}

pub async fn dashboard_page(State(state): State<AppState>, current: CurrentUser) -> Response {
    let uid = current.user.id.clone();
    let memberships = state
        .auth
        .list_workspaces_for_user(&uid)
        .await
        .unwrap_or_default();

    let mut workspaces = Vec::new();
    let mut total_pages = 0usize;
    for m in memberships {
        let mut pages = state
            .repo
            .list_pages("workspace", &m.workspace.id)
            .await
            .unwrap_or_default();
        pages.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        let rows: Vec<PageRow> = pages
            .into_iter()
            .map(|p| PageRow {
                short_uuid: p.uuid.chars().take(10).collect(),
                name: p.name.clone().unwrap_or_default(),
                access: p.access.as_str().to_string(),
                zone: p.zone.as_str().to_string(),
                archived: p.archived,
                size: fmt_bytes(p.size_bytes),
                files: p.file_count,
                updated: fmt_ts(p.updated_at),
                uuid: p.uuid,
            })
            .collect();
        total_pages += rows.len();
        workspaces.push(WorkspaceRow {
            name: m.workspace.name,
            role: m.role.as_str().to_string(),
            pages: rows,
        });
    }

    render(DashboardTemplate {
        username: current.user.username,
        workspaces,
        total_pages,
    })
}

fn render<T: Template>(t: T) -> Response {
    match t.render() {
        Ok(body) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(body))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        Err(e) => {
            tracing::error!(error = %e, "render dashboard template");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn fmt_bytes(n: u64) -> String {
    const U: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n} B")
    } else {
        format!("{v:.1} {}", U[i])
    }
}

fn fmt_ts(ts: i64) -> String {
    let rel = now() - ts;
    if rel < 60 {
        "just now".into()
    } else if rel < 3600 {
        format!("{}m ago", rel / 60)
    } else if rel < 86400 {
        format!("{}h ago", rel / 3600)
    } else {
        format!("{}d ago", rel / 86400)
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
