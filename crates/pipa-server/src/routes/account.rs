//! Phase 3 user self-service area (`/account`).
//!
//! A signed-in user (the `pipa_user` cookie via `CurrentUser`) sees their own
//! CLI devices and recent audit events, and can revoke a device. Revocation is
//! session-authenticated and ownership-checked here — no bearer token needed —
//! so a user can only ever revoke a device it owns.

use std::time::{SystemTime, UNIX_EPOCH};

use askama::Template;
use axum::Router;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use pipa_core::audit::AuditEvent;

use crate::auth::user_cookie::CurrentUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/account", get(account_page))
        .route("/account/devices/:id/revoke", post(revoke_device))
}

struct DeviceRow {
    id: String,
    label: String,
    scope: String,
    created: String,
    last_seen: String,
    revoked: bool,
}

struct AuditRow {
    ts: String,
    action: String,
    target: String,
    success: bool,
}

#[derive(Template)]
#[template(path = "account.html")]
struct AccountTemplate {
    username: String,
    devices: Vec<DeviceRow>,
    events: Vec<AuditRow>,
}

pub async fn account_page(State(state): State<AppState>, current: CurrentUser) -> Response {
    let user = current.user;
    let devices = state
        .auth
        .list_devices_for_user(&user.id)
        .await
        .unwrap_or_default();
    let device_ids: Vec<String> = devices.iter().map(|d| d.id.clone()).collect();

    let device_rows: Vec<DeviceRow> = devices
        .into_iter()
        .map(|d| DeviceRow {
            id: d.id,
            label: d.label,
            scope: d.scope.as_str().to_string(),
            created: fmt_ts(d.created_at),
            last_seen: d.last_seen_at.map(fmt_ts).unwrap_or_else(|| "—".into()),
            revoked: d.revoked_at.is_some(),
        })
        .collect();

    // Audit events attributable to this user: those actored by the user id
    // (login/signup) or by one of their devices (pairing/revocation).
    let events: Vec<AuditRow> = state
        .repo
        .recent_audit(0)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|e: &AuditEvent| e.actor == user.id || device_ids.contains(&e.actor))
        .take(50)
        .map(|e| AuditRow {
            ts: fmt_ts(e.ts),
            action: e.action.as_str().to_string(),
            target: e.target.unwrap_or_default(),
            success: e.success,
        })
        .collect();

    render(AccountTemplate {
        username: user.username,
        devices: device_rows,
        events,
    })
}

pub async fn revoke_device(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<String>,
) -> Response {
    // Ownership check: the device must belong to the signed-in user.
    let owns = state
        .auth
        .list_devices_for_user(&current.user.id)
        .await
        .map(|ds| ds.iter().any(|d| d.id == id))
        .unwrap_or(false);
    if !owns {
        return (StatusCode::FORBIDDEN, "not your device").into_response();
    }
    if let Err(e) = state.auth.revoke_device(&id).await {
        tracing::warn!(error = %e, "user device revoke");
    }
    Redirect::to("/account").into_response()
}

fn render<T: Template>(t: T) -> Response {
    match t.render() {
        Ok(body) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(body))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        Err(e) => {
            tracing::error!(error = %e, "render account template");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn fmt_ts(ts: i64) -> String {
    // Keep it dependency-light: seconds-since-epoch is fine for an internal
    // console; the CLI does the pretty formatting elsewhere.
    let rel = now() - ts;
    if rel < 60 {
        "just now".into()
    } else if rel < 3600 {
        format!("{}m ago", rel / 60)
    } else if rel < 86400 {
        format!("{}h ago", rel / 3600)
    } else {
        format!("{}d ago", rel / 86400)
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
