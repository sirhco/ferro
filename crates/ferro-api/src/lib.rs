//! Ferro HTTP API: Axum router layered with GraphQL, REST, auth, and OpenAPI.

#![deny(rust_2018_idioms, unreachable_pub)]

pub mod error;
pub mod graphql;
pub mod rest;
pub mod state;

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

pub use error::{ApiError, ApiResult};
pub use state::AppState;

pub fn router(state: Arc<AppState>) -> Router {
    let cors = CorsLayer::permissive();
    let trace = TraceLayer::new_for_http();
    let compression = CompressionLayer::new().br(true).gzip(true);

    Router::new()
        .merge(rest::router())
        .merge(graphql::router())
        .layer(compression)
        .layer(cors)
        .layer(trace)
        .layer(tower::timeout::TimeoutLayer::new(Duration::from_secs(30)))
        .with_state(state)
}
