//! `GET /confirm/<code>` shows the step-up confirmation page with the
//! operation, target, requesting device label, and IP (where known). `POST
//! /confirm/<code>` records the confirmation.
//!
//! The destructive endpoint then validates with `consume_step_up` (see
//! `auth::verify_step_up`) — the browser confirmation alone is not enough,
//! the CLI must still present the code via `X-Stepup-Code`.

use std::time::{SystemTime, UNIX_EPOCH};

use askama::Template;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::error::ServerError;
use crate::state::AppState;

#[derive(Template)]
#[template(path = "confirm.html")]
struct ConfirmTemplate<'a> {
    code: &'a str,
    operation: &'a str,
    target: Option<&'a str>,
    device_label: &'a str,
    requesting_ip_short: Option<&'a str>,
    expires_in_human: String,
}

#[derive(Template)]
#[template(path = "confirm_done.html")]
struct ConfirmDoneTemplate;

#[derive(Template)]
#[template(path = "confirm_expired.html")]
struct ConfirmExpiredTemplate;

#[derive(Template)]
#[template(path = "confirm_consumed.html")]
struct ConfirmConsumedTemplate;

pub async fn confirm_get(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Response {
    match confirm_get_inner(state, code).await {
        Ok(resp) => resp,
        Err(e) => e.into_response(),
    }
}

async fn confirm_get_inner(state: AppState, code: String) -> Result<Response, ServerError> {
    let Some(token) = state.auth.step_up_get(&code).await? else {
        return Ok(render(ConfirmExpiredTemplate));
    };
    let now = unix_now();
    if token.consumed_at.is_some() {
        return Ok(render(ConfirmConsumedTemplate));
    }
    if token.expires_at < now {
        return Ok(render(ConfirmExpiredTemplate));
    }
    // If already confirmed, still show the page so the user can re-submit
    // (idempotent) — but we mostly expect the CLI has not consumed yet.
    let device = state.auth.get_device(&token.device_id).await?;
    let device_label = device.as_ref().map(|d| d.label.clone()).unwrap_or_else(|| "(unknown)".into());

    let remaining = (token.expires_at - now).max(0);
    let mins = remaining / 60;
    let secs = remaining % 60;
    let expires_in_human = format!("{mins}:{secs:02}");

    // We never display the raw IP hash to the user; the spec says the page
    // shows the requesting IP, but the storage we have is a daily-rotated
    // HMAC. Render a short fingerprint instead so the human sees *something*
    // that disambiguates two pending step-ups from different sources.
    let ip_owned = token
        .requesting_ip_hash
        .as_deref()
        .map(|h| h.chars().take(12).collect::<String>());

    Ok(render(ConfirmTemplate {
        code: &token.code,
        operation: &token.operation,
        target: token.target.as_deref(),
        device_label: &device_label,
        requesting_ip_short: ip_owned.as_deref(),
        expires_in_human,
    }))
}

pub async fn confirm_post(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Response {
    match confirm_post_inner(state, code).await {
        Ok(resp) => resp,
        Err(e) => e.into_response(),
    }
}

async fn confirm_post_inner(state: AppState, code: String) -> Result<Response, ServerError> {
    let Some(token) = state.auth.step_up_get(&code).await? else {
        return Ok(render(ConfirmExpiredTemplate));
    };
    let now = unix_now();
    if token.consumed_at.is_some() {
        return Ok(render(ConfirmConsumedTemplate));
    }
    if token.expires_at < now {
        return Ok(render(ConfirmExpiredTemplate));
    }
    match state.auth.confirm_step_up(&code).await {
        Ok(()) => Ok(render(ConfirmDoneTemplate)),
        Err(pipa_core::CoreError::Unauthorized) => Ok(render(ConfirmExpiredTemplate)),
        Err(e) => Err(ServerError::Core(e)),
    }
}

fn render<T: Template>(t: T) -> Response {
    match t.render() {
        Ok(body) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(body))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        Err(e) => {
            tracing::error!(error = %e, "render confirm template");
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
