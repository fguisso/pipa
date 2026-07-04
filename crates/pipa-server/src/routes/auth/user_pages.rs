//! Phase 3 user auth pages: `/signup`, `/login`, `/logout`.
//!
//! Username + password only (no email/magic-link this phase). On success we
//! mint a `user_sessions` row and set the signed `pipa_user` cookie, then
//! redirect to `?next=` (or `/` — the user landing area). Mirrors the admin
//! `/login` flow but against the `users` table.

use std::time::{SystemTime, UNIX_EPOCH};

use askama::Template;
use axum::body::Body;
use axum::extract::{Form, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use pipa_adapters::{hash_password, verify_password};
use pipa_core::audit::{AuditAction, AuditEvent};
use pipa_core::user::NewUser;
use serde::Deserialize;

use crate::auth::owner_cookie::safe_next;
use crate::auth::user_cookie::{set_cookie_header, sign_cookie};
use crate::state::AppState;

#[derive(Template)]
#[template(path = "user_signup.html")]
struct SignupTemplate<'a> {
    prefill_username: &'a str,
    next: Option<&'a str>,
    error: Option<&'a str>,
}

#[derive(Template)]
#[template(path = "user_login.html")]
struct LoginTemplate<'a> {
    prefill_username: &'a str,
    next: Option<&'a str>,
    error: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
pub struct NextQuery {
    #[serde(default)]
    pub next: Option<String>,
}

pub async fn signup_get(Query(q): Query<NextQuery>) -> Response {
    render(SignupTemplate {
        prefill_username: "",
        next: q.next.as_deref(),
        error: None,
    })
}

pub async fn login_get(Query(q): Query<NextQuery>) -> Response {
    render(LoginTemplate {
        prefill_username: "",
        next: q.next.as_deref(),
        error: None,
    })
}

#[derive(Debug, Deserialize)]
pub struct SignupForm {
    pub username: String,
    pub password: String,
    pub password_confirm: String,
    #[serde(default)]
    pub next: Option<String>,
}

pub async fn signup_post(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    axum::extract::ConnectInfo(remote): axum::extract::ConnectInfo<std::net::SocketAddr>,
    Form(form): Form<SignupForm>,
) -> Response {
    let username = form.username.trim().to_string();
    let next = form.next.as_deref();

    let err = |msg: &str| {
        render(SignupTemplate {
            prefill_username: &username,
            next,
            error: Some(msg),
        })
    };

    if username.len() < 2 {
        return err("username must be at least 2 characters");
    }
    if form.password.len() < 8 {
        return err("password must be at least 8 characters");
    }
    if form.password != form.password_confirm {
        return err("passwords do not match");
    }

    let hash = match hash_password(&form.password) {
        Ok(h) => h,
        Err(e) => {
            tracing::error!(error = %e, "hash_password");
            return (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response();
        }
    };

    let user = match state
        .auth
        .create_user(NewUser {
            username: username.clone(),
            email: None,
            password_hash: hash,
        })
        .await
    {
        Ok(u) => u,
        Err(pipa_core::CoreError::AlreadyExists) => return err("that username is taken"),
        Err(e) => {
            tracing::error!(error = %e, "create_user");
            return (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response();
        }
    };

    let _ = state
        .repo
        .record_audit(
            AuditEvent::success(unix_now(), user.id.clone(), AuditAction::AuthLogin)
                .with_scope("user_signup".to_string()),
        )
        .await;

    finish_session(&state, &user.id, &headers, remote, next).await
}

#[derive(Debug, Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub next: Option<String>,
}

pub async fn login_post(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    axum::extract::ConnectInfo(remote): axum::extract::ConnectInfo<std::net::SocketAddr>,
    Form(form): Form<LoginForm>,
) -> Response {
    let username = form.username.trim().to_string();
    let next = form.next.as_deref();
    let invalid = || {
        render(LoginTemplate {
            prefill_username: &username,
            next,
            error: Some("invalid username or password"),
        })
    };

    if username.is_empty() || form.password.is_empty() {
        return render(LoginTemplate {
            prefill_username: &username,
            next,
            error: Some("username and password are required"),
        });
    }

    let user = match state.auth.find_user_by_username(&username).await {
        Ok(Some(u)) => u,
        Ok(None) => return invalid(),
        Err(e) => {
            tracing::error!(error = %e, "find_user_by_username");
            return (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response();
        }
    };

    if user.disabled_at.is_some() {
        return render(LoginTemplate {
            prefill_username: &username,
            next,
            error: Some("this account is disabled"),
        });
    }

    let pw = form.password.clone();
    let hash = user.password_hash.clone();
    let ok = tokio::task::spawn_blocking(move || verify_password(&hash, &pw).unwrap_or(false))
        .await
        .unwrap_or(false);

    if !ok {
        let _ = state
            .repo
            .record_audit(AuditEvent::failure(
                unix_now(),
                user.id.clone(),
                AuditAction::AuthLogin,
            ))
            .await;
        return invalid();
    }

    let _ = state
        .repo
        .record_audit(
            AuditEvent::success(unix_now(), user.id.clone(), AuditAction::AuthLogin)
                .with_scope("user_login".to_string()),
        )
        .await;

    finish_session(&state, &user.id, &headers, remote, next).await
}

pub async fn logout_post(
    State(state): State<AppState>,
    parts: axum::http::request::Parts,
) -> Response {
    if let Some((session, _user)) = crate::auth::user_cookie::resolve(&parts, &state).await {
        let _ = state.auth.revoke_user_session(&session.id).await;
    }
    let cookie = crate::auth::user_cookie::clear_cookie_header(state.config.server.dev);
    let mut resp = Redirect::to("/login").into_response();
    resp.headers_mut().insert(header::SET_COOKIE, cookie);
    resp
}

/// Mint a session, set the cookie, and redirect to a safe `next`.
async fn finish_session(
    state: &AppState,
    user_id: &str,
    headers: &axum::http::HeaderMap,
    remote: std::net::SocketAddr,
    next: Option<&str>,
) -> Response {
    let user_agent = headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok());
    let ip = Some(remote.ip().to_string());
    let session = match state
        .auth
        .create_user_session(user_id, user_agent, ip.as_deref())
        .await
    {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, "create_user_session");
            return (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response();
        }
    };

    let cookie_value = sign_cookie(&state.hmac_key, &session.id);
    let cookie_header = set_cookie_header(&cookie_value, state.config.server.dev);
    let target = safe_next(next);
    let mut resp = Redirect::to(&target).into_response();
    resp.headers_mut().insert(header::SET_COOKIE, cookie_header);
    resp
}

fn render<T: Template>(t: T) -> Response {
    match t.render() {
        Ok(body) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(body))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        Err(e) => {
            tracing::error!(error = %e, "render user auth template");
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
