//! `GET <ui_path>/devices` — device list with revoke-self and copy-to-clipboard
//! CLI hints for cross-device revokes (those require step-up which the admin
//! UI deliberately doesn't drive — see ADR 0004 + spec §step-up).

use askama::Template;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::state::AppState;

use super::dashboard::render;
use super::session::{AdminSession, ui_path};

#[derive(Template)]
#[template(path = "admin/devices.html")]
struct DevicesTemplate<'a> {
    ui_path: &'a str,
    show_nav: bool,
    tokens_json: String,
    self_device_json: String,
}

pub async fn devices_page(
    State(state): State<AppState>,
    session: AdminSession,
) -> Response {
    let tokens = match session.mint_tokens(&state) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(error = %e, "mint admin tokens");
            return (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response();
        }
    };
    let tmpl = DevicesTemplate {
        ui_path: ui_path(&state),
        show_nav: true,
        tokens_json: tokens.to_json(),
        self_device_json: serde_json::to_string(&session.device.id)
            .unwrap_or_else(|_| "\"\"".to_string()),
    };
    render(tmpl)
}
