use axum::Router;
use axum::http::StatusCode;
use axum::routing::get;

use crate::state::AppState;

/// `GET /health` → 200 with empty body. Per SECURITY.md §2: never expose
/// build SHA, version, or hostname here.
pub fn router() -> Router<AppState> {
    Router::new().route("/health", get(|| async { StatusCode::OK }))
}
