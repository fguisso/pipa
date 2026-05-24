//! `GET /api/comments/widget.js` — serves the embedded vanilla JS widget that
//! page owners drop in with a single `<script>` tag. The asset is baked into
//! the binary via `rust_embed` and shipped with a one-hour CDN cache. CORS
//! respects `[comments].allowed_origins`:
//!   - `"same-origin"` (default): we only echo `Access-Control-Allow-Origin`
//!     when the request `Origin` matches `server.public_url`'s host; absent
//!     otherwise so cross-origin embeds fail closed.
//!   - Anything else is treated as a comma-separated allowlist; an exact
//!     match echoes the requested origin.

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

use crate::state::AppState;

#[derive(RustEmbed)]
#[folder = "../../ui/widget/"]
struct WidgetAssets;

pub async fn widget_js(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let Some(asset) = WidgetAssets::get("comments.js") else {
        return (StatusCode::NOT_FOUND, "widget not bundled").into_response();
    };

    let mut resp = Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )
        .header(header::CACHE_CONTROL, "public, max-age=3600")
        .body(Body::from(asset.data.into_owned()))
        .expect("static widget response builds");

    let origin = headers
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string());

    if let Some(allowed) = resolve_cors(&state, origin.as_deref()) {
        if let Ok(val) = HeaderValue::from_str(&allowed) {
            resp.headers_mut()
                .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, val);
            resp.headers_mut().insert(
                header::VARY,
                HeaderValue::from_static("origin"),
            );
        }
    }

    resp
}

fn resolve_cors(state: &AppState, origin: Option<&str>) -> Option<String> {
    let allow = state.config.comments.allowed_origins.trim();
    let origin = origin?;

    if allow.eq_ignore_ascii_case("same-origin") {
        let public = state.config.server.public_url.trim();
        if origin_matches(origin, public) {
            return Some(origin.to_string());
        }
        return None;
    }

    for entry in allow.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if entry == "*" {
            return Some("*".to_string());
        }
        if entry.eq_ignore_ascii_case(origin) {
            return Some(origin.to_string());
        }
    }
    None
}

/// Loose origin compare: same scheme + host (ignoring trailing slash on the
/// configured public_url). Both values are scheme-qualified URLs.
fn origin_matches(origin: &str, public_url: &str) -> bool {
    let strip = |s: &str| s.trim_end_matches('/').to_string();
    strip(origin).eq_ignore_ascii_case(&strip(public_url))
}
