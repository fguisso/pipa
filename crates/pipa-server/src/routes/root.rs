//! `GET /` — small landing redirect.
//!
//! Signed-in regular user (`pipa_user` cookie) → `/dashboard`, their own sites.
//! Otherwise: unclaimed server (no admin yet) → `/setup` for the claim wizard;
//! already-claimed server → `<ui_path>` (the admin UI), which redirects to its
//! own login when there's no admin session cookie.

use axum::Router;
use axum::extract::State;
use axum::http::request::Parts;
use axum::response::Redirect;
use axum::routing::get;

use crate::auth::user_cookie;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(root))
}

async fn root(State(state): State<AppState>, parts: Parts) -> Redirect {
    // A signed-in user gets their own landing area rather than the admin UI
    // (which would just bounce them to the operator login).
    if user_cookie::resolve(&parts, &state).await.is_some() {
        return Redirect::to("/dashboard");
    }
    let admins = state.auth.count_admins().await.unwrap_or(0);
    if admins == 0 {
        return Redirect::to("/setup");
    }
    let target = state.config.admin.ui_path.trim_end_matches('/');
    let target = if target.is_empty() { "/admin" } else { target };
    Redirect::to(target)
}
