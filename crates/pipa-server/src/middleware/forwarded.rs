use std::net::SocketAddr;

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;

use crate::state::AppState;

/// Real client IP, decided by:
///   1. peer IP == configured `trusted_proxy` → first hop in `X-Forwarded-For`
///   2. otherwise → peer IP.
///
/// First-hop-from-trusted-proxy is enough for Phase 1; chains-of-proxies and
/// Forwarded RFC parsing belong in a future hardening pass.
#[derive(Debug, Clone)]
pub struct RealIp(pub String);

#[axum::async_trait]
impl FromRequestParts<AppState> for RealIp {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let peer_ip = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ci| ci.0.ip().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        if peer_ip == state.config.server.trusted_proxy {
            if let Some(hdr) = parts
                .headers
                .get("x-forwarded-for")
                .and_then(|v| v.to_str().ok())
            {
                if let Some(first) = hdr.split(',').next() {
                    let cleaned = first.trim().to_string();
                    if !cleaned.is_empty() {
                        return Ok(RealIp(cleaned));
                    }
                }
            }
        }
        Ok(RealIp(peer_ip))
    }
}
