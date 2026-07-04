//! Phase 3 user session cookie.
//!
//! Cookie name: `pipa_user`. Value `<session_id>.<base64url(hmac)>`, signed
//! with the same `HmacKey` as the owner cookie but domain-separated with
//! `user-session/v1/` so a user cookie can never be presented as an owner
//! cookie (or vice versa).
//!
//! Set on `/signup` and `/login`. The `CurrentUser` extractor resolves it to
//! the live `User` + `UserSession`; disabled users and revoked sessions are
//! rejected.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{HeaderValue, StatusCode};
use axum::response::Redirect;
use axum::async_trait;
use pipa_adapters::HmacKey;
use pipa_core::user::{User, UserSession};

use super::owner_cookie::{read_cookie, sign_with_domain, verify_with_domain};
use crate::state::AppState;

pub const COOKIE_NAME: &str = "pipa_user";
const COOKIE_DOMAIN: &[u8] = b"user-session/v1/";
const COOKIE_MAX_AGE_SECONDS: i64 = 60 * 60 * 24 * 365;

pub fn sign_cookie(key: &HmacKey, session_id: &str) -> String {
    sign_with_domain(key, COOKIE_DOMAIN, session_id)
}

pub fn verify_cookie(key: &HmacKey, value: &str) -> Option<String> {
    verify_with_domain(key, COOKIE_DOMAIN, value)
}

pub fn set_cookie_header(value: &str, dev: bool) -> HeaderValue {
    let secure = if dev { "" } else { " Secure;" };
    let s = format!(
        "{COOKIE_NAME}={value}; Path=/; HttpOnly; SameSite=Lax;{secure} Max-Age={COOKIE_MAX_AGE_SECONDS}"
    );
    HeaderValue::from_str(&s).expect("ASCII-only Set-Cookie")
}

pub fn clear_cookie_header(dev: bool) -> HeaderValue {
    let secure = if dev { "" } else { " Secure;" };
    let s = format!("{COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Lax;{secure} Max-Age=0");
    HeaderValue::from_str(&s).expect("ASCII-only Set-Cookie")
}

/// Resolve the current user from the request: verify the cookie, load the live
/// session (must be unrevoked) and user (must not be disabled). Best-effort
/// touches `last_seen_at`. Returns `None` on any failure.
pub async fn resolve(parts: &Parts, state: &AppState) -> Option<(UserSession, User)> {
    let raw = read_cookie(parts, COOKIE_NAME)?;
    let id = verify_cookie(&state.hmac_key, &raw)?;
    let session = state.auth.find_user_session(&id).await.ok().flatten()?;
    let user = state
        .auth
        .find_user_by_id(&session.user_id)
        .await
        .ok()
        .flatten()?;
    if user.disabled_at.is_some() {
        return None;
    }
    let _ = state.auth.touch_user_session(&session.id).await;
    Some((session, user))
}

/// Extractor requiring a signed-in user. On failure redirects to `/login` with
/// `?next=` set to the current path.
pub struct CurrentUser {
    pub session: UserSession,
    pub user: User,
}

#[async_trait]
impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = Redirect;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if let Some((session, user)) = resolve(parts, state).await {
            return Ok(CurrentUser { session, user });
        }
        let next = parts
            .uri
            .path_and_query()
            .map(|pq| pq.as_str())
            .unwrap_or("/");
        Err(Redirect::to(&format!("/login?next={}", urlencode(next))))
    }
}

/// Optional variant — `Some` when a valid user cookie is present, else `None`.
/// Never redirects. Used by pages that branch on sign-in state (e.g. `/cli`).
pub struct CurrentUserOpt(pub Option<(UserSession, User)>);

#[async_trait]
impl FromRequestParts<AppState> for CurrentUserOpt {
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(CurrentUserOpt(resolve(parts, state).await))
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
