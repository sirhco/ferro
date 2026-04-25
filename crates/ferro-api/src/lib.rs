//! Ferro HTTP API: Axum router layered with GraphQL, REST, auth, and OpenAPI.

#![deny(rust_2018_idioms, unreachable_pub)]

pub mod auth;
pub mod error;
pub mod graphql;
pub mod openapi;
pub mod rest;
pub mod sse;
pub mod state;
pub mod ui;

use std::sync::Arc;

use axum::Router;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

pub use error::{ApiError, ApiResult};
pub use state::{AppState, AuthOptions};

pub fn router(state: Arc<AppState>) -> Router {
    let cors = CorsLayer::permissive();
    let trace = TraceLayer::new_for_http();
    let compression = CompressionLayer::new().br(true).gzip(true);

    // NOTE: request timeout deferred — `tower::timeout::TimeoutLayer` maps its
    // error to `Box<dyn Error>` which Axum 0.7's router cannot fold into
    // `Infallible`. Reinstate via `tower_http::timeout` once that feature is
    // enabled in the workspace.
    Router::new()
        .merge(rest::router())
        .merge(graphql::router())
        .merge(openapi::router())
        .merge(sse::router())
        .layer(compression)
        .layer(cors)
        .layer(trace)
        .with_state(state)
        // Stateless routes mounted after `with_state` so they don't need to
        // satisfy the AppState binding: Swagger UI assets + the operator HTML
        // (landing page + minimal admin SPA).
        .merge(openapi::swagger_ui_router())
        .merge(ui::router())
}
