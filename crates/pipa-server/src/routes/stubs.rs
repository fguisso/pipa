use axum::Router;
use axum::http::StatusCode;
use axum::routing::{get, patch, post};

use crate::state::AppState;

async fn not_implemented() -> (StatusCode, &'static str) {
    (StatusCode::NOT_IMPLEMENTED, "not implemented yet")
}

/// Reserve the URL space that Phase 1 milestones M4 (deploy) and M5
/// (comments + admin) will fill in. Every route returns 501 so an
/// integration test can confirm the router shape without depending on the
/// eventual handlers. M3 routes (`/api/auth/*`, `/api/devices*`, `/cli`,
/// `/confirm/*`) have been replaced with real handlers.
pub fn router() -> Router<AppState> {
    let pages = Router::new()
        .route("/api/pages", get(not_implemented).post(not_implemented))
        .route(
            "/api/pages/:uuid",
            get(not_implemented).delete(not_implemented),
        )
        .route("/api/pages/:uuid/visibility", post(not_implemented))
        .route("/api/pages/:uuid/stats", get(not_implemented));

    let comments = Router::new()
        .route(
            "/api/pages/:uuid/comments",
            get(not_implemented).post(not_implemented),
        )
        .route(
            "/api/pages/:uuid/comments/moderation",
            get(not_implemented),
        )
        .route(
            "/api/comments/:id",
            patch(not_implemented).delete(not_implemented),
        )
        .route("/api/comments/widget.js", get(not_implemented));

    let admin = Router::new().route("/admin/*rest", get(not_implemented));

    Router::new().merge(pages).merge(comments).merge(admin)
}
