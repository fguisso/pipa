use axum::Router;

use crate::middleware::headers::SecurityHeadersLayer;
use crate::state::AppState;

pub mod admin;
pub mod auth;
pub mod comments;
pub mod devices;
pub mod health;
pub mod pages;
pub mod public;
pub mod stubs;

/// Top-level router. Composes:
///   - public file serving at `/p/<uuid>/*`
///   - `/health`
///   - real auth + devices routes (M3)
///   - real `/api/pages*` routes (M4)
///   - real `/api/comments*` + widget routes (M5)
///   - admin web UI (M7) under `[admin] ui_path`
///
/// Global hardening headers are applied here; the page CSP is applied as a
/// per-route layer inside `public::router()`.
pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(public::router(state.clone()))
        .merge(health::router())
        .merge(auth::router())
        .merge(devices::router())
        .merge(pages::router(&state))
        .merge(comments::router())
        .merge(admin::router(&state))
        .merge(stubs::router())
        .layer(SecurityHeadersLayer)
        .with_state(state)
}
