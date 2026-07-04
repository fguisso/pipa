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
mod oauth;
mod setup_page;
mod stepup;
mod user_pages;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/device-init", post(device_flow::device_init))
        .route("/api/auth/device-poll", post(device_flow::device_poll))
        .route("/api/auth/mint", post(mint::mint))
        .route("/api/auth/logout", post(logout::logout))
        .route("/api/auth/stepup-init", post(stepup::stepup_init))
        .route("/api/auth/stepup-status", post(stepup::stepup_status))
        .route("/setup", get(setup_page::setup_get).post(setup_page::setup_post))
        // Phase 3 user auth (username + password).
        .route("/signup", get(user_pages::signup_get).post(user_pages::signup_post))
        .route("/login", get(user_pages::login_get).post(user_pages::login_post))
        .route("/logout", post(user_pages::logout_post))
        // OAuth scaffold (not implemented — 501).
        .route("/auth/oauth/:provider", get(oauth::oauth_start))
        .route("/auth/oauth/:provider/callback", get(oauth::oauth_callback))
        .route("/cli", get(cli_page::cli_get).post(cli_page::cli_post))
        .route(
            "/confirm/:code",
            get(confirm_page::confirm_get).post(confirm_page::confirm_post),
        )
}
