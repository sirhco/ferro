use thiserror::Error;

pub type MediaResult<T> = Result<T, MediaError>;

#[derive(Debug, Error)]
pub enum MediaError {
    #[error("backend not enabled: {0}")]
    BackendNotEnabled(&'static str),

    #[error("not found")]
    NotFound,

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("backend: {0}")]
    Backend(String),

    #[error("unsupported mime: {0}")]
    UnsupportedMime(String),
}
