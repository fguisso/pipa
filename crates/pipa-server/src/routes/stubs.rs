use axum::Router;
use axum::http::StatusCode;
use axum::routing::{delete, get, patch, post};

use crate::state::AppState;

async fn not_implemented() -> (StatusCode, &'static str) {
    (StatusCode::NOT_IMPLEMENTED, "not implemented yet")
}

/// Reserve the URL space that Phase 1 milestones M3–M5 (auth, deploy,
/// comments) will fill in. Every route returns 501 so an integration test can
/// confirm the router shape without depending on the eventual handlers.
pub fn router() -> Router<AppState> {
    let auth = Router::new()
        .route("/api/auth/device-init", post(not_implemented))
        .route("/api/auth/device-poll", post(not_implemented))
        .route("/api/auth/mint", post(not_implemented))
        .route("/api/auth/logout", post(not_implemented))
        .route("/api/auth/stepup-init", post(not_implemented))
        .route("/api/auth/stepup-confirm", post(not_implemented));

    let devices = Router::new()
        .route("/api/devices", get(not_implemented))
        .route("/api/devices/:id", delete(not_implemented));

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

    let confirm = Router::new()
        .route("/cli", get(not_implemented))
        .route("/confirm/:code", get(not_implemented));

    let admin = Router::new().route("/admin/*rest", get(not_implemented));

    Router::new()
        .merge(auth)
        .merge(devices)
        .merge(pages)
        .merge(comments)
        .merge(confirm)
        .merge(admin)
}
