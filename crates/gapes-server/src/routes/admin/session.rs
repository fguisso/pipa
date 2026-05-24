//! Admin browser session — now backed by the `gapes_owner` cookie.
//!
//! Sign-in flow:
//!   1. POST `<ui_path>/login` with `username` + `password`.
//!   2. We verify the password (argon2) and, on success, mint an
//!      `owner_sessions` row + set the `gapes_owner` cookie.
//!   3. Subsequent admin GETs use the `AdminSession` extractor below, which
//!      delegates to `OwnerCookie` and then loads the admin's synthetic
//!      "Admin Web UI" device. That device's id is the `sub` for any access
//!      tokens minted inside admin handlers — so existing audit + scope
//!      logic continues to work unmodified.

use std::time::{SystemTime, UNIX_EPOCH};

use askama::Template;
use axum::body::Body;
use axum::extract::{Form, FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::{async_trait, response::Redirect};
use gapes_adapters::verify_password;
use gapes_core::audit::{AuditAction, AuditEvent};
use gapes_core::device::Device;
use serde::Deserialize;

use crate::auth::owner_cookie::{
    OwnerCookie, clear_cookie_header, set_cookie_header, sign_cookie,
};
use crate::auth::tokens::mint_access_token;
use crate::state::AppState;

/// Per-request handle: the admin's synthetic device. Handlers use
/// `mint_tokens()` to derive a per-scope access token bundle for the JSON
/// payload handed to Alpine.
pub struct AdminSession {
    pub device: Device,
}

impl AdminSession {
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
        // Reuse the owner cookie: any browser holding a valid session row is
        // the admin (single-owner). We bounce to login (rather than /setup)
        // when missing so a logged-out admin sees a familiar page — unless
        // no admin exists yet, in which case /setup is the right place.
        let cookie_ok = OwnerCookie::from_request_parts(parts, state).await.is_ok();

        let admin = match state.auth.get_admin().await {
            Ok(Some(a)) => a,
            _ => return Err(redirect_to_bootstrap(state).await),
        };

        if !cookie_ok {
            return Err(redirect_login(state));
        }

        let device = match state.auth.get_device(&admin.synthetic_device_id).await {
            Ok(Some(d)) if d.revoked_at.is_none() => d,
            _ => return Err(redirect_login(state)),
        };

        Ok(AdminSession { device })
    }
}

#[derive(Template)]
#[template(path = "admin/login.html")]
struct LoginTemplate<'a> {
    ui_path: &'a str,
    show_nav: bool,
    prefill_username: &'a str,
    error: Option<&'a str>,
}

pub async fn login_get(State(state): State<AppState>) -> Response {
    // If no admin exists yet, send them to setup instead of a dead login form.
    if state.auth.count_admins().await.unwrap_or(0) == 0 {
        return Redirect::to("/setup").into_response();
    }
    render(LoginTemplate {
        ui_path: ui_path(&state),
        show_nav: false,
        prefill_username: "",
        error: None,
    })
}

#[derive(Debug, Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
}

pub async fn login_post(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    axum::extract::ConnectInfo(remote): axum::extract::ConnectInfo<std::net::SocketAddr>,
    Form(form): Form<LoginForm>,
) -> Response {
    let username = form.username.trim().to_string();
    if username.is_empty() || form.password.is_empty() {
        return render(LoginTemplate {
            ui_path: ui_path(&state),
            show_nav: false,
            prefill_username: &username,
            error: Some("username and password are required"),
        });
    }

    let admin = match state.auth.find_admin_by_username(&username).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            // Don't leak which half is wrong.
            return render(LoginTemplate {
                ui_path: ui_path(&state),
                show_nav: false,
                prefill_username: &username,
                error: Some("invalid username or password"),
            });
        }
        Err(e) => {
            tracing::error!(error = %e, "find_admin_by_username");
            return (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response();
        }
    };

    let pw = form.password.clone();
    let hash = admin.password_hash.clone();
    let ok = tokio::task::spawn_blocking(move || verify_password(&hash, &pw).unwrap_or(false))
        .await
        .unwrap_or(false);

    if !ok {
        let _ = state
            .repo
            .record_audit(AuditEvent::failure(
                unix_now(),
                admin.id.clone(),
                AuditAction::AuthLogin,
            ))
            .await;
        return render(LoginTemplate {
            ui_path: ui_path(&state),
            show_nav: false,
            prefill_username: &username,
            error: Some("invalid username or password"),
        });
    }

    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok());
    let ip = Some(remote.ip().to_string());
    let session = match state
        .auth
        .create_owner_session(user_agent, ip.as_deref())
        .await
    {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, "create_owner_session");
            return (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response();
        }
    };

    let _ = state
        .repo
        .record_audit(
            AuditEvent::success(unix_now(), admin.id.clone(), AuditAction::AuthLogin)
                .with_scope("admin_ui".to_string()),
        )
        .await;

    let cookie_value = sign_cookie(&state.hmac_key, &session.id);
    let cookie_header = set_cookie_header(&cookie_value, state.config.server.dev);

    let mut resp = Redirect::to(ui_path(&state)).into_response();
    resp.headers_mut().insert(header::SET_COOKIE, cookie_header);
    resp
}

pub async fn logout_post(
    State(state): State<AppState>,
    parts: axum::http::request::Parts,
) -> Response {
    if let Some(session) = crate::auth::owner_cookie::resolve(&parts, &state).await {
        let _ = state.auth.revoke_owner_session(&session.id).await;
    }
    let cookie_header = clear_cookie_header(state.config.server.dev);
    let mut resp = Redirect::to(&format!("{}/login", ui_path(&state))).into_response();
    resp.headers_mut().insert(header::SET_COOKIE, cookie_header);
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

async fn redirect_to_bootstrap(state: &AppState) -> Response {
    if state.auth.count_admins().await.unwrap_or(1) == 0 {
        return Redirect::to("/setup").into_response();
    }
    redirect_login(state)
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

