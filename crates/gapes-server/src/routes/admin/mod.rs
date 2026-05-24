//! Admin web UI — five askama-rendered pages plus a small static-asset
//! mount and a JSON endpoint for the activity feed. All routes are gated
//! behind `[admin] ui_enabled` in `pages.toml`; flip it to `false` and the
//! router collapses to an empty `Router`.

use axum::Router;
use axum::routing::{get, post};

use crate::state::AppState;

mod activity;
mod assets;
mod dashboard;
mod devices;
mod pages;
mod session;

/// Returns an empty router when `[admin] ui_enabled = false`. Otherwise
/// mounts all admin routes under `[admin] ui_path` (default `/admin`).
pub fn router(state: &AppState) -> Router<AppState> {
    if !state.config.admin.ui_enabled {
        return Router::new();
    }

    let path = state.config.admin.ui_path.trim_end_matches('/');
    let path = if path.is_empty() { "/admin" } else { path };

    Router::new()
        .route(path, get(dashboard::dashboard))
        .route(&format!("{path}/login"), get(session::login_get).post(session::login_post))
        .route(&format!("{path}/logout"), post(session::logout_post))
        .route(&format!("{path}/pages/:uuid"), get(pages::page_detail))
        .route(&format!("{path}/devices"), get(devices::devices_page))
        .route(&format!("{path}/activity"), get(activity::activity_page))
        .route(&format!("{path}/assets/*path"), get(assets::serve_asset))
        .route("/api/audit/recent", get(activity::recent_audit_json))
}
