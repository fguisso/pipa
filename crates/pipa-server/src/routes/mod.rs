use axum::Router;

use crate::middleware::headers::SecurityHeadersLayer;
use crate::state::AppState;

pub mod health;
pub mod public;
pub mod stubs;

/// Top-level router. Composes:
///   - public file serving at `/p/<uuid>/*`
///   - `/health`
///   - M3–M5 stubs returning 501 so the URL space is reserved
///
/// Global hardening headers are applied here; the page CSP is applied as a
/// per-route layer inside `public::router()`.
pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(public::router(state.clone()))
        .merge(health::router())
        .merge(stubs::router())
        .layer(SecurityHeadersLayer)
        .with_state(state)
}
