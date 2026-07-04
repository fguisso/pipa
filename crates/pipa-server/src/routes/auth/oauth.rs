//! OAuth sign-in — **scaffold only** (Phase 3).
//!
//! The data model is in place (`oauth_identities` table, `OAuthProvider`,
//! `AuthStore::{link_oauth, find_user_by_oauth}`) so Phase 5 can wire real
//! GitHub / Google flows without another migration. These endpoints exist to
//! pin the route shape (`/auth/oauth/:provider[/callback]`) and return a clear
//! `501 Not Implemented` until then — no provider calls happen.

use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use pipa_core::user::OAuthProvider;

use crate::error::ApiError;

fn parse_provider(p: &str) -> Result<OAuthProvider, ApiError> {
    p.parse::<OAuthProvider>()
        .map_err(|_| ApiError::bad_request("unknown_provider", "provider must be github|google"))
}

/// `GET /auth/oauth/:provider` — would redirect to the provider's authorize URL.
pub async fn oauth_start(Path(provider): Path<String>) -> impl IntoResponse {
    match parse_provider(&provider) {
        Ok(p) => (
            StatusCode::NOT_IMPLEMENTED,
            format!(
                "OAuth sign-in with {} is not implemented yet (scaffold — planned for Phase 5). \
                 Use /signup or /login for now.",
                p.as_str()
            ),
        )
            .into_response(),
        Err(e) => e.into_response(),
    }
}

/// `GET /auth/oauth/:provider/callback` — would exchange the code, resolve the
/// identity via `find_user_by_oauth` / `link_oauth`, and mint a user session.
pub async fn oauth_callback(Path(provider): Path<String>) -> impl IntoResponse {
    match parse_provider(&provider) {
        Ok(p) => (
            StatusCode::NOT_IMPLEMENTED,
            format!("OAuth callback for {} is not implemented yet (scaffold).", p.as_str()),
        )
            .into_response(),
        Err(e) => e.into_response(),
    }
}
