use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use ferro_auth::AuthError;
use ferro_core::CoreError;
use ferro_media::MediaError;
use ferro_storage::StorageError;
use serde::Serialize;
use thiserror::Error;

pub type ApiResult<T> = Result<T, ApiError>;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Media(#[from] MediaError),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("not found")]
    NotFound,
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("too many requests; retry after {0:?}")]
    RateLimited(std::time::Duration),
    #[error("service unavailable: {0}")]
    Unavailable(String),
    #[error("internal: {0}")]
    Internal(String),
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
    message: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            Self::NotFound | Self::Storage(StorageError::NotFound) => {
                (StatusCode::NOT_FOUND, "not_found")
            }
            Self::Unauthorized | Self::Auth(AuthError::InvalidCredentials) => {
                (StatusCode::UNAUTHORIZED, "unauthorized")
            }
            Self::Forbidden(_) | Self::Auth(AuthError::Forbidden) => {
                (StatusCode::FORBIDDEN, "forbidden")
            }
            Self::RateLimited(_) => (StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
            Self::Unavailable(_) => (StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
            Self::BadRequest(_) | Self::Core(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        };
        (status, Json(ErrorBody { error: code.into(), message: self.to_string() })).into_response()
    }
}
