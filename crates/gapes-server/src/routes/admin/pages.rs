//! `GET <ui_path>/pages/:uuid` — page detail. All data (page metadata,
//! comments) is fetched client-side via Alpine so the page tracks reality
//! without a full reload after each toggle.

use askama::Template;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::state::AppState;

use super::dashboard::render;
use super::session::{AdminSession, ui_path};

#[derive(Template)]
#[template(path = "admin/page_detail.html")]
struct PageDetailTemplate<'a> {
    ui_path: &'a str,
    show_nav: bool,
    uuid: &'a str,
    uuid_short: String,
    uuid_json: String,
    tokens_json: String,
    ui_path_json: String,
}

pub async fn page_detail(
    State(state): State<AppState>,
    session: AdminSession,
    Path(uuid): Path<String>,
) -> Response {
    let tokens = match session.mint_tokens(&state) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(error = %e, "mint admin tokens");
            return (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response();
        }
    };
    let path = ui_path(&state);
    let tmpl = PageDetailTemplate {
        ui_path: path,
        show_nav: true,
        uuid: &uuid,
        uuid_short: uuid.chars().take(10).collect(),
        uuid_json: serde_json::to_string(&uuid).unwrap_or_else(|_| "\"\"".to_string()),
        tokens_json: tokens.to_json(),
        ui_path_json: serde_json::to_string(path).unwrap_or_else(|_| "\"/admin\"".to_string()),
    };
    render(tmpl)
}
