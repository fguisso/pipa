//! `/api/pages/*` — deploy, list, get, delete, visibility change, stats.
//! Step-up confirmation is required for destructive operations (delete and
//! private->public visibility change), matching SECURITY.md §3.

use axum::Router;
use axum::routing::{delete as delete_route, get, post};
use tower_http::limit::RequestBodyLimitLayer;

use crate::state::AppState;

mod deploy;
mod delete;
mod list_get;
mod stats;
mod visibility;

pub(crate) mod util;

/// Headroom over `config.hosting.max_upload_bytes` for multipart framing,
/// part headers, and the optional small text fields (`uuid`, `mode`, ...).
const BODY_LIMIT_HEADROOM_BYTES: usize = 16 * 1024;

pub fn router(state: &AppState) -> Router<AppState> {
    let upload_limit =
        state.config.hosting.max_upload_bytes as usize + BODY_LIMIT_HEADROOM_BYTES;

    let deploy_only = Router::new()
        .route("/api/pages", post(deploy::deploy))
        .layer(RequestBodyLimitLayer::new(upload_limit));

    Router::new()
        .merge(deploy_only)
        .route("/api/pages", get(list_get::list_pages))
        .route("/api/pages/:uuid", get(list_get::get_page))
        .route("/api/pages/:uuid", delete_route(delete::delete_page))
        .route(
            "/api/pages/:uuid/visibility",
            post(visibility::change_visibility),
        )
        .route("/api/pages/:uuid/stats", get(stats::stats))
}
