use serde::Deserialize;
use thiserror::Error;

/// Mirrors `gapes-server`'s `ApiError` body: `{ "error": "<code>", "message": "<human>" }`.
#[derive(Debug, Clone, Deserialize)]
pub struct ErrorBody {
    pub error: String,
    pub message: String,
}

#[derive(Debug, Error)]
pub enum SdkError {
    #[error("transport: {0}")]
    Transport(#[from] reqwest::Error),

    #[error("url: {0}")]
    Url(#[from] url::ParseError),

    /// Non-2xx response. `body` is `None` only when the server returned an
    /// empty body or one that did not parse as our standard error shape.
    #[error("api {status}: {}", body.as_ref().map(|b| b.message.as_str()).unwrap_or("<no body>"))]
    Api {
        status: u16,
        body: Option<ErrorBody>,
    },

    #[error("invalid response: {0}")]
    Decode(String),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

impl SdkError {
    /// True if the response body decoded as our typed `ErrorBody` and the
    /// `error` code matches.
    pub fn is_code(&self, code: &str) -> bool {
        matches!(self, SdkError::Api { body: Some(b), .. } if b.error == code)
    }

    pub fn status(&self) -> Option<u16> {
        match self {
            SdkError::Api { status, .. } => Some(*status),
            _ => None,
        }
    }
}
