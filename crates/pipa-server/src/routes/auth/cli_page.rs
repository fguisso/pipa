//! Browser-facing pages for the device-pairing flow.
//!
//! `GET /cli` renders the approval form. The user types the pairing code
//! from their terminal, the human label they want, and — if this is the
//! first device — the setup code. Submitting POST /cli runs the approval.

use askama::Template;
use axum::body::Body;
use axum::extract::{Form, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use pipa_core::audit::{AuditAction, AuditEvent};
use pipa_core::device::Scope;
use serde::Deserialize;

use crate::error::ServerError;
use crate::state::AppState;

#[derive(Template)]
#[template(path = "cli.html")]
struct CliTemplate<'a> {
    prefill_device_code: &'a str,
    prefill_label: &'a str,
    requires_setup_code: bool,
    scope: &'a str,
    error: Option<&'a str>,
}

#[derive(Template)]
#[template(path = "cli_approved.html")]
struct CliApprovedTemplate<'a> {
    label: &'a str,
}

#[derive(Template)]
#[template(path = "cli_error.html")]
struct CliErrorTemplate<'a> {
    reason: &'a str,
}

#[derive(Debug, Deserialize)]
pub struct CliQuery {
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
}

pub async fn cli_get(
    State(state): State<AppState>,
    Query(q): Query<CliQuery>,
) -> Response {
    let devices = state.auth.devices_count().await.unwrap_or(0);
    let scope = q.scope.as_deref().unwrap_or("interactive");
    let tmpl = CliTemplate {
        prefill_device_code: q.code.as_deref().unwrap_or(""),
        prefill_label: q.label.as_deref().unwrap_or(""),
        requires_setup_code: devices == 0,
        scope,
        error: None,
    };
    render(tmpl)
}

#[derive(Debug, Deserialize)]
pub struct CliForm {
    pub device_code: String,
    pub label: String,
    #[serde(default)]
    pub setup_code: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
}

pub async fn cli_post(
    State(state): State<AppState>,
    Form(form): Form<CliForm>,
) -> Response {
    match cli_post_inner(state, form).await {
        Ok(resp) => resp,
        Err(e) => e.into_response(),
    }
}

async fn cli_post_inner(state: AppState, form: CliForm) -> Result<Response, ServerError> {
    let devices = state.auth.devices_count().await?;
    let scope: Scope = form
        .scope
        .as_deref()
        .unwrap_or("interactive")
        .parse()
        .unwrap_or(Scope::Interactive);

    if devices == 0 {
        let code = form.setup_code.as_deref().unwrap_or("");
        let ok = state.auth.consume_setup_code(code).await?;
        if !ok {
            return Ok(render(CliTemplate {
                prefill_device_code: &form.device_code,
                prefill_label: &form.label,
                requires_setup_code: true,
                scope: scope.as_str(),
                error: Some("invalid or expired setup code"),
            }));
        }
    }

    if form.label.trim().is_empty() {
        return Ok(render(CliTemplate {
            prefill_device_code: &form.device_code,
            prefill_label: &form.label,
            requires_setup_code: devices == 0,
            scope: scope.as_str(),
            error: Some("label is required"),
        }));
    }

    let label = form.label.trim().to_string();
    match state
        .auth
        .approve_pairing(&form.device_code, &label, scope)
        .await
    {
        Ok(issued) => {
            let _ = state
                .repo
                .record_audit(AuditEvent::success(
                    unix_now(),
                    issued.device.id.clone(),
                    AuditAction::AuthLogin,
                ))
                .await;
            Ok(render(CliApprovedTemplate { label: &issued.device.label }))
        }
        Err(pipa_core::CoreError::NotFound) => Ok(render(CliErrorTemplate {
            reason: "unknown pairing code — re-check the code shown by your CLI",
        })),
        Err(pipa_core::CoreError::AlreadyExists) => Ok(render(CliErrorTemplate {
            reason: "this pairing has already been approved",
        })),
        Err(pipa_core::CoreError::InvalidInput(msg)) => Ok(render(CliErrorTemplate {
            reason: if msg.contains("expired") {
                "pairing code expired — start a new login from your CLI"
            } else {
                "could not approve pairing"
            },
        })),
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
            tracing::error!(error = %e, "render cli template");
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
