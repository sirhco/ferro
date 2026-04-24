use ferro_core::CoreError;
use thiserror::Error;

pub type StorageResult<T> = Result<T, StorageError>;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("backend not enabled: {0} — rebuild with feature flag")]
    BackendNotEnabled(&'static str),

    #[error("not found")]
    NotFound,

    #[error("unique violation on `{field}`")]
    UniqueViolation { field: &'static str },

    #[error("backend error: {0}")]
    Backend(String),

    #[error("serialization: {0}")]
    Serde(String),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("core: {0}")]
    Core(#[from] CoreError),
}
