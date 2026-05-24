//! `GET /setup` and `POST /setup` — first-boot admin creation.
//!
//! When no admin exists, `POST /setup` creates the admin user (username +
//! argon2-hashed password + a synthetic "Admin Web UI" device), then mints
//! an owner session and sets the `pipa_owner` cookie so the same browser is
//! immediately authenticated. Subsequent visits to `/setup` on an
//! initialized server show an info page (different copy if this browser is
//! already an admin session).

use askama::Template;
use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use pipa_adapters::hash_password;
use pipa_core::audit::{AuditAction, AuditEvent};
use serde::Deserialize;

use crate::auth::owner_cookie::{
    OwnerCookieOpt, safe_next, set_cookie_header, sign_cookie,
};
use crate::error::ServerError;
use crate::state::AppState;

#[derive(Template)]
#[template(path = "setup.html")]
struct SetupTemplate<'a> {
    prefill_username: &'a str,
    next: Option<&'a str>,
    error: Option<&'a str>,
}

#[derive(Template)]
#[template(path = "setup_claimed.html")]
struct SetupClaimedTemplate<'a> {
    i_am_owner: bool,
    admin_url: &'a str,
    admin_login_url: &'a str,
}

#[derive(Debug, Deserialize)]
pub struct SetupQuery {
    #[serde(default)]
    pub next: Option<String>,
}

pub async fn setup_get(
    State(state): State<AppState>,
    OwnerCookieOpt(session): OwnerCookieOpt,
    Query(q): Query<SetupQuery>,
) -> Response {
    let count = state.auth.count_admins().await.unwrap_or(0);
    if count == 0 {
        return render(SetupTemplate {
            prefill_username: "",
            next: q.next.as_deref(),
            error: None,
        });
    }
    let admin_url = admin_ui_path(&state);
    let login_url = format!("{}/login", admin_url.trim_end_matches('/'));
    render(SetupClaimedTemplate {
        i_am_owner: session.is_some(),
        admin_url: &admin_url,
        admin_login_url: &login_url,
    })
}

#[derive(Debug, Deserialize)]
pub struct SetupForm {
    pub username: String,
    pub password: String,
    pub password_confirm: String,
    #[serde(default)]
    pub next: Option<String>,
}

pub async fn setup_post(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    axum::extract::ConnectInfo(remote): axum::extract::ConnectInfo<std::net::SocketAddr>,
    Form(form): Form<SetupForm>,
) -> Result<Response, ServerError> {
    let count = state.auth.count_admins().await?;
    if count > 0 {
        // Already initialized — refuse to mint a second admin.
        let admin_url = admin_ui_path(&state);
        let login_url = format!("{}/login", admin_url.trim_end_matches('/'));
        return Ok(render(SetupClaimedTemplate {
            i_am_owner: false,
            admin_url: &admin_url,
            admin_login_url: &login_url,
        }));
    }

    let username = form.username.trim().to_string();
    if username.is_empty() {
        return Ok(render(SetupTemplate {
            prefill_username: &username,
            next: form.next.as_deref(),
            error: Some("username is required"),
        }));
    }
    if form.password.len() < 8 {
        return Ok(render(SetupTemplate {
            prefill_username: &username,
            next: form.next.as_deref(),
            error: Some("password must be at least 8 characters"),
        }));
    }
    if form.password != form.password_confirm {
        return Ok(render(SetupTemplate {
            prefill_username: &username,
            next: form.next.as_deref(),
            error: Some("passwords do not match"),
        }));
    }

    let password_hash = match hash_password(&form.password) {
        Ok(h) => h,
        Err(e) => {
            tracing::error!(error = %e, "hash admin password");
            return Ok(render(SetupTemplate {
                prefill_username: &username,
                next: form.next.as_deref(),
                error: Some("internal error hashing password — try again"),
            }));
        }
    };

    let admin = match state.auth.create_admin(&username, &password_hash).await {
        Ok(a) => a,
        Err(pipa_core::CoreError::AlreadyExists) => {
            // Race with another setup POST or username collision (impossible
            // when count_admins was 0, but we surface a clear error anyway).
            return Ok(render(SetupTemplate {
                prefill_username: &username,
                next: form.next.as_deref(),
                error: Some("admin already exists — refresh and sign in"),
            }));
        }
        Err(e) => return Err(ServerError::Core(e)),
    };

    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok());
    let ip = Some(remote.ip().to_string());
    let session = state
        .auth
        .create_owner_session(user_agent, ip.as_deref())
        .await?;

    let _ = state
        .repo
        .record_audit(
            AuditEvent::success(unix_now(), admin.id.clone(), AuditAction::OwnerClaim)
                .with_details(format!("username={username}"))
                .with_ip_hash(ip.clone().unwrap_or_default()),
        )
        .await;

    let cookie_value = sign_cookie(&state.hmac_key, &session.id);
    let cookie_header = set_cookie_header(&cookie_value, state.config.server.dev);

    // After signup, default landing is the admin UI (the user just created
    // it; that's where they want to go). Honor explicit ?next when present.
    let target = match form.next.as_deref() {
        Some(n) if !n.is_empty() => safe_next(Some(n)),
        _ => admin_ui_path(&state),
    };

    let mut resp = Redirect::to(&target).into_response();
    resp.headers_mut().insert(header::SET_COOKIE, cookie_header);
    Ok(resp)
}

fn admin_ui_path(state: &AppState) -> String {
    let p = state.config.admin.ui_path.trim_end_matches('/');
    if p.is_empty() { "/admin".to_string() } else { p.to_string() }
}

fn render<T: Template>(t: T) -> Response {
    match t.render() {
        Ok(body) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(body))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        Err(e) => {
            tracing::error!(error = %e, "render setup template");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn unix_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
