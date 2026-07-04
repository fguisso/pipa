//! `GET <ui_path>` — the dashboard. Server-side pre-fetches the pages list
//! and most-recent audit events so first paint shows real data; Alpine
//! takes over for refresh and any subsequent CRUD on the page.

use askama::Template;
use axum::body::Body;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::routes::pages::util::{OWNER_ID_LOCAL, OWNER_KIND_LOCAL, PageView};
use crate::state::AppState;

use super::activity::audit_events_to_json;
use super::session::{AdminSession, ui_path};

#[derive(Template)]
#[template(path = "admin/dashboard.html")]
struct DashboardTemplate<'a> {
    ui_path: &'a str,
    show_nav: bool,
    tokens_json: String,
    pages_json: String,
    events_json: String,
    ui_path_json: String,
    /// True only in a `thumbnails`-feature build with the runtime toggle on;
    /// gates the per-page preview column in the template.
    thumbnails_enabled: bool,
}

pub async fn dashboard(
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

    let pages = state
        .repo
        .list_pages(OWNER_KIND_LOCAL, OWNER_ID_LOCAL)
        .await
        .unwrap_or_default();
    let pages_view: Vec<PageView> = pages.iter().map(PageView::from).collect();
    let pages_json =
        serde_json::to_string(&pages_view).unwrap_or_else(|_| "[]".to_string());

    let events_json = match state.repo.recent_audit(0).await {
        Ok(mut rows) => {
            rows.sort_by(|a, b| b.ts.cmp(&a.ts));
            rows.truncate(20);
            audit_events_to_json(&rows)
        }
        Err(_) => "[]".to_string(),
    };

    let path = ui_path(&state);
    let tmpl = DashboardTemplate {
        ui_path: path,
        show_nav: true,
        tokens_json: tokens.to_json(),
        pages_json,
        events_json,
        ui_path_json: serde_json::to_string(path).unwrap_or_else(|_| "\"/admin\"".to_string()),
        thumbnails_enabled: cfg!(feature = "thumbnails") && state.config.thumbnails.enabled,
    };
    render(tmpl)
}

pub(crate) fn render<T: Template>(t: T) -> Response {
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
