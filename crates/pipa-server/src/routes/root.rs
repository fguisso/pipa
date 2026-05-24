//! `GET /` — small landing redirect.
//!
//! Unclaimed server (`owner_sessions` empty) → `/setup` so the very first
//! visitor lands on the claim wizard.
//! Already-claimed server → `<ui_path>` (the admin UI). The admin UI then
//! redirects to its own login when there's no admin session cookie.

use axum::Router;
use axum::extract::State;
use axum::response::Redirect;
use axum::routing::get;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(root))
}

async fn root(State(state): State<AppState>) -> Redirect {
    let admins = state.auth.count_admins().await.unwrap_or(0);
    if admins == 0 {
        return Redirect::to("/setup");
    }
    let target = state.config.admin.ui_path.trim_end_matches('/');
    let target = if target.is_empty() { "/admin" } else { target };
    Redirect::to(target)
}
