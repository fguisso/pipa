//! `/api/auth/*` and `/cli`, `/confirm/*` handlers. Split into one module per
//! endpoint family for readability; aggregated into a single Router below.

use axum::Router;
use axum::routing::{get, post};

use crate::state::AppState;

mod cli_page;
mod confirm_page;
mod device_flow;
mod logout;
mod mint;
mod stepup;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/device-init", post(device_flow::device_init))
        .route("/api/auth/device-poll", post(device_flow::device_poll))
        .route("/api/auth/mint", post(mint::mint))
        .route("/api/auth/logout", post(logout::logout))
        .route("/api/auth/stepup-init", post(stepup::stepup_init))
        .route("/api/auth/stepup-status", post(stepup::stepup_status))
        .route("/cli", get(cli_page::cli_get).post(cli_page::cli_post))
        .route(
            "/confirm/:code",
            get(confirm_page::confirm_get).post(confirm_page::confirm_post),
        )
}
