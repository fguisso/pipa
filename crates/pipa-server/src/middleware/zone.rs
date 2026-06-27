//! Zone detection (compiled only under the `zone` feature).
//!
//! Classifies an incoming request as the internal (LAN) or external (internet)
//! zone so the serving layer can 404 a page whose `zone` doesn't match the
//! channel the request arrived on.
//!
//! A request is the internal ([`Zone::Private`]) zone only when BOTH hold:
//!   * its TCP peer (the reverse proxy) IP is in `config.zone.internal_proxy_ips`, and
//!   * its `Host` header matches one of `config.zone.internal_hosts`
//!     (a leading `*.` is a subdomain wildcard).
//!
//! Otherwise it is the external ([`Zone::Public`]) zone. Requiring BOTH the
//! peer IP *and* the Host means an internet visitor can't reach a private page
//! by forging the Host header — the tunnel's peer IP is never in the internal
//! list, so the page stays a silent 404 from outside.

use axum::http::HeaderMap;
use axum::http::header::HOST;
use pipa_core::Zone;

use crate::state::AppState;

/// Resolve the zone a request arrived on from its proxy peer IP + headers.
pub fn request_zone(state: &AppState, peer_ip: Option<&str>, headers: &HeaderMap) -> Zone {
    let cfg = &state.config.zone;

    let ip_ok = peer_ip
        .map(|ip| cfg.internal_proxy_ips.iter().any(|p| p == ip))
        .unwrap_or(false);

    let host_ok = headers
        .get(HOST)
        .and_then(|v| v.to_str().ok())
        .map(|h| {
            let host = h.split(':').next().unwrap_or(h); // strip :port
            cfg.internal_hosts.iter().any(|pat| host_matches(pat, host))
        })
        .unwrap_or(false);

    if ip_ok && host_ok {
        Zone::Private
    } else {
        Zone::Public
    }
}

/// `*.example.com` matches `x.example.com` and bare `example.com`; an exact
/// pattern matches case-insensitively.
fn host_matches(pattern: &str, host: &str) -> bool {
    if let Some(suffix) = pattern.strip_prefix("*.") {
        host.eq_ignore_ascii_case(suffix)
            || host
                .len()
                .checked_sub(suffix.len() + 1)
                .map(|cut| host[cut..].eq_ignore_ascii_case(&format!(".{suffix}")))
                .unwrap_or(false)
    } else {
        pattern.eq_ignore_ascii_case(host)
    }
}
