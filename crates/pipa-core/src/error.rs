use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("not found")]
    NotFound,

    #[error("already exists")]
    AlreadyExists,

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("unauthorized")]
    Unauthorized,

    #[error("storage failure: {0}")]
    StorageFailure(String),

    #[error("repository failure: {0}")]
    RepositoryFailure(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;
