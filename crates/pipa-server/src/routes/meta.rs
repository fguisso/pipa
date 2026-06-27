//! `GET /api/meta` — authenticated capability discovery.
//!
//! Returns the set of optional build features this server actually enforces,
//! so a client can gate feature-dependent flags (e.g. `--zone`) before sending
//! a request that the server would silently ignore. Authenticated (any
//! logged-in device) on purpose: an anonymous visitor must not be able to
//! fingerprint the build, and the list is only useful to a real client.

use axum::Router;
use axum::routing::get;
use axum::{Json, async_trait};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::response::Response;
use serde::Serialize;

use crate::auth::AuthClaims;
use crate::state::AppState;

#[derive(Debug, Serialize)]
struct Meta {
    /// Optional features compiled into this server. Absent features mean the
    /// corresponding flags are accepted but NOT enforced.
    features: Vec<&'static str>,
}

/// Build-time feature list. Each `#[cfg]` push is the single source of truth
/// for "does this server enforce X".
fn enforced_features() -> Vec<&'static str> {
    #[allow(unused_mut)]
    let mut f: Vec<&'static str> = Vec::new();
    #[cfg(feature = "zone")]
    f.push("zone");
    f
}

/// Require a valid access token but don't care about its scope — any logged-in
/// device may read capabilities.
struct AnyDevice;

#[async_trait]
impl FromRequestParts<AppState> for AnyDevice {
    type Rejection = Response;
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        AuthClaims::from_request_parts(parts, state).await?;
        Ok(AnyDevice)
    }
}

async fn meta(_: AnyDevice) -> Json<Meta> {
    Json(Meta {
        features: enforced_features(),
    })
}

pub fn router() -> Router<AppState> {
    Router::new().route("/api/meta", get(meta))
}
