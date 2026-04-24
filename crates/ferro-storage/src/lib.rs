//! Storage abstraction for Ferro.
//!
//! Backends are feature-gated. The [`Repository`] trait is the single surface
//! the rest of the system talks to; no backend types leak upward.

#![deny(rust_2018_idioms, unreachable_pub)]
#![warn(missing_debug_implementations)]

pub mod backends;
pub mod config;
pub mod error;
pub mod repo;
pub mod schema;

pub use config::StorageConfig;
pub use error::{StorageError, StorageResult};
pub use repo::{
    ContentRepo, ContentTypeRepo, MediaMetaRepo, Repository, SiteRepo, UserRepo,
};

/// Connect to the backend described by `cfg`. The returned box is the root
/// dependency every caller threads through services and handlers.
pub async fn connect(cfg: &StorageConfig) -> StorageResult<Box<dyn Repository>> {
    backends::connect(cfg).await
}
