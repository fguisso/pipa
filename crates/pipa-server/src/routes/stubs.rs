use axum::Router;
use axum::http::StatusCode;
use axum::routing::get;

use crate::state::AppState;

async fn not_implemented() -> (StatusCode, &'static str) {
    (StatusCode::NOT_IMPLEMENTED, "not implemented yet")
}

/// Reserve the URL space that Phase 1 milestone M7 (admin UI) will fill in.
/// M3 routes (`/api/auth/*`, `/api/devices*`, `/cli`, `/confirm/*`), M4
/// routes (`/api/pages*`), and M5 routes (`/api/comments*`,
/// `/api/comments/widget.js`) have all been replaced with real handlers.
pub fn router() -> Router<AppState> {
    Router::new().route("/admin/*rest", get(not_implemented))
}
