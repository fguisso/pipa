//! `GET <ui_path>/assets/*path` — serve admin static assets (Alpine vendor,
//! admin.css, admin.js) baked into the binary at build time via rust-embed.
//! One-hour public cache; everything here is checksum-stable across a given
//! build.

use axum::body::Body;
use axum::extract::Path;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../ui/public/"]
struct AdminAssets;

pub async fn serve_asset(Path(path): Path<String>) -> Response {
    let safe = path.trim_start_matches('/');
    if safe.contains("..") || safe.is_empty() {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }

    // Layout under `ui/public/`:
    //   admin/admin.css, admin/admin.js, vendor/alpine.min.js
    // We try `admin/<rel>` first (the common case for CSS/JS shipped by us),
    // then fall back to the literal path under `ui/public/` so callers can
    // still reach `vendor/alpine.min.js` as `assets/alpine.min.js`.
    let candidates: [String; 3] = [
        format!("admin/{safe}"),
        format!("vendor/{safe}"),
        safe.to_string(),
    ];

    for key in &candidates {
        if let Some(asset) = AdminAssets::get(key) {
            let mime = mime_guess::from_path(key)
                .first_or_octet_stream()
                .to_string();
            return Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime)
                .header(header::CACHE_CONTROL, "public, max-age=3600")
                .body(Body::from(asset.data.into_owned()))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }
    }

    (StatusCode::NOT_FOUND, "not found").into_response()
}
