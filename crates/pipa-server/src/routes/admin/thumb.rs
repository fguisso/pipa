//! `GET <ui_path>/pages/:uuid/thumb` — serve a page's cached thumbnail PNG.
//!
//! Admin-only (the `AdminSession` extractor gates it). Feature-gated behind
//! `thumbnails`. The bytes live at `<data_dir>/thumbnails/<uuid>.png`, written
//! best-effort after each deploy by [`crate::thumbnails`]; a 404 simply means
//! "not captured yet" and the dashboard degrades to no image.

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::state::AppState;

use super::session::AdminSession;

pub async fn serve_thumb(
    State(state): State<AppState>,
    _session: AdminSession,
    Path(uuid): Path<String>,
) -> Response {
    // The uuid is a path component of a filesystem read — reject anything that
    // could escape the thumbnails dir.
    if uuid.is_empty()
        || uuid.contains('/')
        || uuid.contains('\\')
        || uuid.contains("..")
    {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let path = state
        .config
        .server
        .data_dir
        .join("thumbnails")
        .join(format!("{uuid}.png"));

    match tokio::fs::read(&path).await {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "image/png")
            .header(header::CACHE_CONTROL, "private, max-age=60")
            .body(Body::from(bytes))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}
