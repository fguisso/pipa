use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use pipa_core::CoreError;
use thiserror::Error;

/// Catch-all error type for handlers. Maps to a status + tiny opaque body.
/// We deliberately avoid leaking internal failure details — the spec calls
/// for 404 over 403, and we do not want stack traces or db errors reaching
/// the wire.
#[derive(Debug, Error)]
#[allow(dead_code)] // NotFound / BadRequest are reserved for M3+ handlers.
pub enum ServerError {
    #[error("not found")]
    NotFound,

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("internal error")]
    Internal(#[from] anyhow::Error),

    #[error("core error: {0}")]
    Core(#[from] CoreError),
}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        let status = match &self {
            ServerError::NotFound => StatusCode::NOT_FOUND,
            ServerError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ServerError::Internal(e) => {
                tracing::error!(error = ?e, "internal server error");
                StatusCode::INTERNAL_SERVER_ERROR
            }
            ServerError::Core(CoreError::NotFound) => StatusCode::NOT_FOUND,
            ServerError::Core(CoreError::Unauthorized) => StatusCode::NOT_FOUND,
            ServerError::Core(CoreError::InvalidInput(_)) => StatusCode::BAD_REQUEST,
            ServerError::Core(CoreError::AlreadyExists) => StatusCode::CONFLICT,
            ServerError::Core(e) => {
                tracing::error!(error = %e, "core error");
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };
        let body = match status {
            StatusCode::NOT_FOUND => "not found",
            StatusCode::BAD_REQUEST => "bad request",
            StatusCode::CONFLICT => "conflict",
            _ => "internal error",
        };
        (status, body).into_response()
    }
}
