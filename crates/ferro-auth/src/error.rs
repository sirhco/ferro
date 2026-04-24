use ferro_storage::StorageError;
use thiserror::Error;

pub type AuthResult<T> = Result<T, AuthError>;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("invalid credentials")]
    InvalidCredentials,

    #[error("account disabled")]
    AccountDisabled,

    #[error("session expired")]
    SessionExpired,

    #[error("session not found")]
    SessionNotFound,

    #[error("forbidden")]
    Forbidden,

    #[error("password hash error: {0}")]
    Hash(String),

    #[error("jwt: {0}")]
    Jwt(#[from] jsonwebtoken::errors::Error),

    #[error("storage: {0}")]
    Storage(#[from] StorageError),
}
