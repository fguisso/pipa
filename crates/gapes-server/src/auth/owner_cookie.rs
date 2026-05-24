//! Browser-side ownership cookie.
//!
//! Cookie name: `gapes_owner`
//! Value format: `<session_id>.<base64url(hmac)>` signed with the existing
//! `HmacKey` and domain-separated with `owner-session/v1/`.
//!
//! Set on POST /setup (TOFU claim). Subsequent requests with this cookie are
//! recognized as the server's owner — sufficient to approve a `/cli` pairing
//! without typing any code.

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;
use axum::http::{HeaderValue, header};
use axum::response::Redirect;
use axum::{async_trait, http::StatusCode};
use gapes_adapters::HmacKey;
use gapes_core::device::OwnerSession;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::net::SocketAddr;

use crate::state::AppState;

type HmacSha256 = Hmac<Sha256>;

pub const COOKIE_NAME: &str = "gapes_owner";
const COOKIE_DOMAIN: &[u8] = b"owner-session/v1/";
/// 1 year. Renewed implicitly on every authenticated request (server-side
/// `last_seen_at` is updated, but the cookie itself is opaque so we don't
/// need to re-set it on each request — we re-set it only when it would
/// otherwise expire soon, deferred for now).
const COOKIE_MAX_AGE_SECONDS: i64 = 60 * 60 * 24 * 365;

/// Sign a session id into the cookie value `id.sig`.
pub fn sign_cookie(key: &HmacKey, session_id: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(key.as_bytes()).expect("any-length HMAC key");
    mac.update(COOKIE_DOMAIN);
    mac.update(session_id.as_bytes());
    let sig = mac.finalize().into_bytes();
    format!("{session_id}.{}", base64url(&sig))
}

/// Verify a cookie value `id.sig` and return the session id when the signature
/// matches. Returns `None` for any malformed input — callers should treat
/// "no cookie" and "bad cookie" the same way.
pub fn verify_cookie(key: &HmacKey, value: &str) -> Option<String> {
    let (id, sig_b64) = value.split_once('.')?;
    let provided_sig = base64url_decode(sig_b64)?;
    let mut mac = HmacSha256::new_from_slice(key.as_bytes()).expect("any-length HMAC key");
    mac.update(COOKIE_DOMAIN);
    mac.update(id.as_bytes());
    mac.verify_slice(&provided_sig).ok()?;
    Some(id.to_string())
}

/// Build a `Set-Cookie` header value for the owner cookie. Marks `Secure` only
/// when the server is not in dev mode (HTTP loopback dev would otherwise drop
/// the cookie).
pub fn set_cookie_header(value: &str, dev: bool) -> HeaderValue {
    let secure = if dev { "" } else { " Secure;" };
    let s = format!(
        "{COOKIE_NAME}={value}; Path=/; HttpOnly; SameSite=Lax;{secure} Max-Age={COOKIE_MAX_AGE_SECONDS}"
    );
    HeaderValue::from_str(&s).expect("ASCII-only Set-Cookie")
}

/// Build a `Set-Cookie` header that clears the owner cookie.
#[allow(dead_code)]
pub fn clear_cookie_header(dev: bool) -> HeaderValue {
    let secure = if dev { "" } else { " Secure;" };
    let s = format!("{COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Lax;{secure} Max-Age=0");
    HeaderValue::from_str(&s).expect("ASCII-only Set-Cookie")
}

/// Read the `gapes_owner` cookie from a request, ignoring other cookies.
fn extract_cookie_value(parts: &Parts) -> Option<String> {
    for h in parts.headers.get_all(header::COOKIE).iter() {
        let s = h.to_str().ok()?;
        for piece in s.split(';') {
            let piece = piece.trim();
            if let Some(rest) = piece.strip_prefix(&format!("{COOKIE_NAME}=")) {
                return Some(rest.to_string());
            }
        }
    }
    None
}

/// Resolve the owner session from the request: read the cookie, verify the
/// HMAC, look up the session in the DB, ensure it's still active. Returns
/// `None` on any failure.
pub async fn resolve(parts: &Parts, state: &AppState) -> Option<OwnerSession> {
    let raw = extract_cookie_value(parts)?;
    let id = verify_cookie(&state.hmac_key, &raw)?;
    let session = state.auth.find_owner_session(&id).await.ok().flatten()?;
    // Best-effort touch — we don't fail the request if it errors.
    let _ = state.auth.touch_owner_session(&session.id).await;
    Some(session)
}

/// Capture the request's client IP from axum's ConnectInfo, when available.
/// Used when minting a new session at /setup.
pub fn client_ip(parts: &Parts) -> Option<String> {
    parts
        .extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip().to_string())
}

/// Extractor for handlers that require an owner cookie. Yields the
/// `OwnerSession` on success; on failure, redirects to `/setup` with the
/// current request URI as `?next=…`. Use this on owner-only pages like
/// `/cli` and the (future) `/admin/sessions` view.
pub struct OwnerCookie(pub OwnerSession);

#[async_trait]
impl FromRequestParts<AppState> for OwnerCookie {
    type Rejection = Redirect;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if let Some(session) = resolve(parts, state).await {
            return Ok(OwnerCookie(session));
        }
        let next = parts
            .uri
            .path_and_query()
            .map(|pq| pq.as_str())
            .unwrap_or("/");
        let target = format!(
            "/setup?next={}",
            urlencode_path_query(next)
        );
        Err(Redirect::to(&target))
    }
}

/// Optional extractor — yields `Some(session)` when the request has an owner
/// cookie, `None` otherwise. Used by handlers that branch on owner state but
/// shouldn't redirect (e.g., `/setup` itself).
pub struct OwnerCookieOpt(pub Option<OwnerSession>);

#[async_trait]
impl FromRequestParts<AppState> for OwnerCookieOpt {
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(OwnerCookieOpt(resolve(parts, state).await))
    }
}

/// Validate a `?next=` value: must be a relative path starting with `/` and
/// not `//` (which would be protocol-relative). Returns the cleaned path or a
/// default `/` when invalid.
pub fn safe_next(next: Option<&str>) -> String {
    match next {
        Some(n) if n.starts_with('/') && !n.starts_with("//") => n.to_string(),
        _ => "/".to_string(),
    }
}

// --- minimal helpers ----------------------------------------------------

fn urlencode_path_query(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

const URL_ALPHA: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn base64url(input: &[u8]) -> String {
    let mut out = String::with_capacity((input.len() * 4).div_ceil(3));
    let mut i = 0;
    while i + 3 <= input.len() {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8) | (input[i + 2] as u32);
        out.push(URL_ALPHA[((n >> 18) & 0x3f) as usize] as char);
        out.push(URL_ALPHA[((n >> 12) & 0x3f) as usize] as char);
        out.push(URL_ALPHA[((n >> 6) & 0x3f) as usize] as char);
        out.push(URL_ALPHA[(n & 0x3f) as usize] as char);
        i += 3;
    }
    let rem = input.len() - i;
    if rem == 1 {
        let n = (input[i] as u32) << 16;
        out.push(URL_ALPHA[((n >> 18) & 0x3f) as usize] as char);
        out.push(URL_ALPHA[((n >> 12) & 0x3f) as usize] as char);
    } else if rem == 2 {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8);
        out.push(URL_ALPHA[((n >> 18) & 0x3f) as usize] as char);
        out.push(URL_ALPHA[((n >> 12) & 0x3f) as usize] as char);
        out.push(URL_ALPHA[((n >> 6) & 0x3f) as usize] as char);
    }
    out
}

fn base64url_decode(input: &str) -> Option<Vec<u8>> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4 + 2);
    let mut buf: u32 = 0;
    let mut bits: u8 = 0;
    for &b in bytes {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        };
        buf = (buf << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xff) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_round_trip() {
        let key = HmacKey::from_bytes(vec![9u8; 32]);
        let cookie = sign_cookie(&key, "abc123");
        assert_eq!(verify_cookie(&key, &cookie).as_deref(), Some("abc123"));
    }

    #[test]
    fn tampered_id_fails() {
        let key = HmacKey::from_bytes(vec![9u8; 32]);
        let cookie = sign_cookie(&key, "abc123");
        let mut parts = cookie.split('.');
        let _ = parts.next();
        let sig = parts.next().unwrap();
        let bad = format!("evil123.{sig}");
        assert!(verify_cookie(&key, &bad).is_none());
    }

    #[test]
    fn wrong_key_fails() {
        let key1 = HmacKey::from_bytes(vec![1u8; 32]);
        let key2 = HmacKey::from_bytes(vec![2u8; 32]);
        let cookie = sign_cookie(&key1, "abc123");
        assert!(verify_cookie(&key2, &cookie).is_none());
    }

    #[test]
    fn safe_next_filters() {
        assert_eq!(safe_next(Some("/cli?code=X")), "/cli?code=X");
        assert_eq!(safe_next(Some("//evil.com")), "/");
        assert_eq!(safe_next(Some("https://evil.com")), "/");
        assert_eq!(safe_next(None), "/");
    }
}
