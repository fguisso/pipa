use axum::Router;

use crate::middleware::headers::SecurityHeadersLayer;
use crate::state::AppState;

pub mod auth;
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
///   - remaining stubs for M5 (comments / admin) returning 501
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
        .merge(stubs::router())
        .layer(SecurityHeadersLayer)
        .with_state(state)
}
