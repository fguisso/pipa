//! `AuthClaims` axum extractor: pulls a Bearer token out of the
//! `Authorization` header, verifies it with the server HMAC key, and yields
//! the decoded claims. Absent or invalid → 401 with a tiny JSON body.

use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use axum::{Json, async_trait};
use pipa_core::device::AccessTokenClaims;
use serde::Serialize;

use crate::auth::tokens::{TokenError, verify_access_token};
use crate::state::AppState;

#[derive(Debug, Clone)]
pub struct AuthClaims(pub AccessTokenClaims);

#[derive(Debug, Serialize)]
struct ErrBody {
    error: &'static str,
    message: &'static str,
}

fn err(status: StatusCode, error: &'static str, message: &'static str) -> Response {
    let mut resp = (status, Json(ErrBody { error, message })).into_response();
    resp.headers_mut().insert(
        "www-authenticate",
        axum::http::HeaderValue::from_static("Bearer"),
    );
    resp
}

#[async_trait]
impl FromRequestParts<AppState> for AuthClaims {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                err(
                    StatusCode::UNAUTHORIZED,
                    "missing_bearer",
                    "Authorization header is required",
                )
            })?;

        let token = header.strip_prefix("Bearer ").ok_or_else(|| {
            err(
                StatusCode::UNAUTHORIZED,
                "missing_bearer",
                "Authorization header must be `Bearer <token>`",
            )
        })?;

        match verify_access_token(&state.hmac_key, token.trim()) {
            Ok(claims) => Ok(AuthClaims(claims)),
            Err(TokenError::Expired) => Err(err(
                StatusCode::UNAUTHORIZED,
                "token_expired",
                "access token expired; mint a new one",
            )),
            Err(_) => Err(err(
                StatusCode::UNAUTHORIZED,
                "invalid_token",
                "access token is malformed or signature invalid",
            )),
        }
    }
}
