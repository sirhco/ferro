use thiserror::Error;

pub type CoreResult<T> = Result<T, CoreError>;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("invalid id: {0}")]
    InvalidId(String),

    #[error("invalid slug: {0}")]
    InvalidSlug(String),

    #[error("validation failed: {0}")]
    Validation(String),

    #[error("schema mismatch: {0}")]
    Schema(String),

    #[error("unknown field: {0}")]
    UnknownField(String),

    #[error("unknown content type: {0}")]
    UnknownContentType(String),

    #[error("forbidden: {0}")]
    Forbidden(String),
}
