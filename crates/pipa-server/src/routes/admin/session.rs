//! Admin browser session.
//!
//! The session cookie stores the *refresh token* — not an access token —
//! signed with the server HMAC so a tampered cookie is rejected before we
//! ever touch the auth store. Each request mints fresh per-scope access
//! tokens server-side so the bootstrap JSON handed to Alpine is always
//! current. TTL 30 days (matches `auth.refresh_ttl_days` headroom).
//!
//! `AdminSession` is the gated extractor used by every admin GET page: on
//! failure it redirects to `<ui_path>/login` rather than returning 401, so
//! a stale cookie produces a friendly bounce instead of a JSON blob.
//!
//! Why not store an access token directly? Access tokens are scoped to one
//! `verb:target` pair (see `auth/scope.rs`). The admin UI needs `read:*`,
//! `admin:*`, and `manage:devices` for different fetches. Storing the
//! refresh lets us mint each of them on demand, with short TTLs.

use std::time::{SystemTime, UNIX_EPOCH};

use askama::Template;
use axum::body::Body;
use axum::extract::{Form, FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::{async_trait, response::Redirect};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use pipa_core::audit::{AuditAction, AuditEvent};
use pipa_core::device::Device;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;

use crate::auth::tokens::mint_access_token;
use crate::state::AppState;

type HmacSha256 = Hmac<Sha256>;

pub const COOKIE_NAME: &str = "admin_access";
const SESSION_TTL_SECS: i64 = 60 * 60 * 24 * 30;
const COOKIE_DOMAIN: &[u8] = b"admin-session/v1/";

/// Per-request handle: the verified refresh plaintext + the device behind it.
/// Handlers use `mint_tokens()` to derive a per-scope access token bundle.
pub struct AdminSession {
    #[allow(dead_code)] // reserved for re-mint flows (M8+); kept for clarity
    pub refresh: String,
    pub device: Device,
}

impl AdminSession {
    /// Mint one access token per scope the admin pages need. Returns
    /// `(read, admin, manage)`. TTL is short — 5 minutes — because these
    /// land in the HTML bootstrap and the page itself is server-rendered on
    /// every navigation so the user always gets a fresh batch.
    pub fn mint_tokens(&self, state: &AppState) -> anyhow::Result<TokenBundle> {
        let key = &state.hmac_key;
        let (read, _) = mint_access_token(key, &self.device.id, "read:*", 300)?;
        let (admin, _) = mint_access_token(key, &self.device.id, "admin:*", 300)?;
        let (manage, _) = mint_access_token(key, &self.device.id, "manage:devices", 300)?;
        Ok(TokenBundle { read, admin, manage })
    }
}

pub struct TokenBundle {
    pub read: String,
    pub admin: String,
    pub manage: String,
}

impl TokenBundle {
    pub fn to_json(&self) -> String {
        serde_json::json!({
            "read": self.read,
            "admin": self.admin,
            "manage": self.manage,
        })
        .to_string()
    }
}

#[async_trait]
impl FromRequestParts<AppState> for AdminSession {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let cookie = jar.get(COOKIE_NAME).ok_or_else(|| redirect_login(state))?;
        let refresh = match unwrap_cookie(state, cookie.value()) {
            Some(r) => r,
            None => return Err(redirect_login(state)),
        };
        let lookup = state
            .auth
            .lookup_refresh(&refresh)
            .await
            .map_err(|_| redirect_login(state))?;
        let (_token, device) = lookup.ok_or_else(|| redirect_login(state))?;
        if device.revoked_at.is_some() {
            return Err(redirect_login(state));
        }
        Ok(AdminSession { refresh, device })
    }
}

#[derive(Template)]
#[template(path = "admin/login.html")]
struct LoginTemplate<'a> {
    ui_path: &'a str,
    show_nav: bool,
    error: Option<&'a str>,
}

pub async fn login_get(State(state): State<AppState>) -> Response {
    render(LoginTemplate {
        ui_path: ui_path(&state),
        show_nav: false,
        error: None,
    })
}

#[derive(Debug, Deserialize)]
pub struct LoginForm {
    pub refresh: String,
}

pub async fn login_post(
    State(state): State<AppState>,
    jar: CookieJar,
    Form(form): Form<LoginForm>,
) -> Response {
    let refresh = form.refresh.trim().to_string();
    if refresh.is_empty() {
        return render(LoginTemplate {
            ui_path: ui_path(&state),
            show_nav: false,
            error: Some("refresh token is required"),
        });
    }
    let lookup = match state.auth.lookup_refresh(&refresh).await {
        Ok(Some(x)) => x,
        Ok(None) => {
            return render(LoginTemplate {
                ui_path: ui_path(&state),
                show_nav: false,
                error: Some("refresh token unknown, expired, or revoked"),
            });
        }
        Err(e) => {
            tracing::error!(error = %e, "lookup_refresh failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response();
        }
    };

    let (_, device) = lookup;
    if device.revoked_at.is_some() {
        return render(LoginTemplate {
            ui_path: ui_path(&state),
            show_nav: false,
            error: Some("device for this refresh token has been revoked"),
        });
    }

    let _ = state
        .repo
        .record_audit(
            AuditEvent::success(unix_now(), device.id.clone(), AuditAction::AuthLogin)
                .with_scope("admin_ui".to_string()),
        )
        .await;

    let cookie_value = wrap_cookie(&state, &refresh);
    let mut cookie = Cookie::new(COOKIE_NAME, cookie_value);
    cookie.set_path(ui_path(&state).to_string() + "/");
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Strict);
    cookie.set_secure(!state.config.server.dev);
    cookie.set_max_age(time::Duration::seconds(SESSION_TTL_SECS));
    let jar = jar.add(cookie);

    let mut resp = Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header(header::LOCATION, ui_path(&state))
        .body(Body::empty())
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    for (k, v) in jar.into_response().headers() {
        if k == header::SET_COOKIE {
            resp.headers_mut().append(k, v.clone());
        }
    }
    resp
}

pub async fn logout_post(State(state): State<AppState>, jar: CookieJar) -> Response {
    let mut cookie = Cookie::new(COOKIE_NAME, "");
    cookie.set_path(ui_path(&state).to_string() + "/");
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Strict);
    cookie.set_secure(!state.config.server.dev);
    cookie.set_max_age(time::Duration::seconds(0));
    let jar = jar.add(cookie);

    let mut resp = Redirect::to(&format!("{}/login", ui_path(&state))).into_response();
    for (k, v) in jar.into_response().headers() {
        if k == header::SET_COOKIE {
            resp.headers_mut().append(k, v.clone());
        }
    }
    resp
}

pub fn ui_path(state: &AppState) -> &str {
    state
        .config
        .admin
        .ui_path
        .trim_end_matches('/')
}

fn redirect_login(state: &AppState) -> Response {
    Redirect::to(&format!("{}/login", ui_path(state))).into_response()
}

/// Cookie value: `<refresh>|<expires>.<hex-hmac>`. We use `|` because the
/// refresh token is base64url-safe (no `|`), so split is unambiguous. The
/// HMAC is keyed by `COOKIE_DOMAIN` so this signature cannot be confused
/// with a step-up or page-password cookie sharing the same HMAC key.
fn wrap_cookie(state: &AppState, refresh: &str) -> String {
    let exp = unix_now() + SESSION_TTL_SECS;
    let body = format!("{refresh}|{exp}");
    let mut mac = HmacSha256::new_from_slice(state.hmac_key.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(COOKIE_DOMAIN);
    mac.update(body.as_bytes());
    let sig = hex::encode(mac.finalize().into_bytes());
    format!("{body}.{sig}")
}

fn unwrap_cookie(state: &AppState, raw: &str) -> Option<String> {
    let (body, sig_hex) = raw.rsplit_once('.')?;
    let sig = hex::decode(sig_hex).ok()?;
    let mut mac = HmacSha256::new_from_slice(state.hmac_key.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(COOKIE_DOMAIN);
    mac.update(body.as_bytes());
    mac.verify_slice(&sig).ok()?;
    let (refresh, exp_str) = body.rsplit_once('|')?;
    let exp: i64 = exp_str.parse().ok()?;
    if exp < unix_now() {
        return None;
    }
    Some(refresh.to_string())
}

fn render<T: Template>(t: T) -> Response {
    match t.render() {
        Ok(body) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(body))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        Err(e) => {
            tracing::error!(error = %e, "render admin template");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
