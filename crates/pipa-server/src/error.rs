use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use pipa_core::CoreError;
use serde::Serialize;
use thiserror::Error;

/// Catch-all error type for handlers. Maps to a status + body.
///
/// Pages and public file routes use the legacy `not found` / `bad request`
/// text bodies (no leakage). Auth + JSON API routes use `ApiError` to return
/// a consistent `{ "error": "<code>", "message": "<human>" }` JSON shape so
/// the CLI can branch on machine-readable codes.
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

    /// Structured API error — handlers that return JSON should prefer this.
    #[error("api error")]
    Api(ApiError),
}

#[derive(Debug, Clone)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }

    pub fn unauthorized(code: &'static str, msg: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, code, msg)
    }

    pub fn forbidden(code: &'static str, msg: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, code, msg)
    }

    pub fn bad_request(code: &'static str, msg: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code, msg)
    }

    pub fn not_found(code: &'static str, msg: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, code, msg)
    }

    pub fn gone(code: &'static str, msg: impl Into<String>) -> Self {
        Self::new(StatusCode::GONE, code, msg)
    }
}

#[derive(Serialize)]
struct ApiErrorBody<'a> {
    error: &'a str,
    message: &'a str,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ApiErrorBody {
            error: self.code,
            message: &self.message,
        };
        (self.status, Json(body)).into_response()
    }
}

impl From<ApiError> for ServerError {
    fn from(e: ApiError) -> Self {
        ServerError::Api(e)
    }
}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        match self {
            ServerError::Api(e) => return e.into_response(),
            _ => {}
        }
        let (status, body): (StatusCode, &'static str) = match &self {
            ServerError::NotFound => (StatusCode::NOT_FOUND, "not found"),
            ServerError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad request"),
            ServerError::Internal(e) => {
                tracing::error!(error = ?e, "internal server error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error")
            }
            ServerError::Core(CoreError::NotFound) => (StatusCode::NOT_FOUND, "not found"),
            ServerError::Core(CoreError::Unauthorized) => (StatusCode::NOT_FOUND, "not found"),
            ServerError::Core(CoreError::InvalidInput(_)) => {
                (StatusCode::BAD_REQUEST, "bad request")
            }
            ServerError::Core(CoreError::AlreadyExists) => (StatusCode::CONFLICT, "conflict"),
            ServerError::Core(e) => {
                tracing::error!(error = %e, "core error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error")
            }
            ServerError::Api(_) => unreachable!(),
        };
        (status, body).into_response()
    }
}
