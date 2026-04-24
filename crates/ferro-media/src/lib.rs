//! Media storage abstraction. Backends are feature-gated.
//!
//! The [`MediaStore`] trait is deliberately small: put/get/delete/presign.
//! Image transforms (resize, format, quality) live in [`image_pipeline`] and
//! operate on byte streams returned by any backend.

#![deny(rust_2018_idioms, unreachable_pub)]

pub mod backends;
pub mod config;
pub mod error;
#[cfg(feature = "images")]
pub mod image_pipeline;
pub mod store;

pub use config::MediaConfig;
pub use error::{MediaError, MediaResult};
pub use store::{ByteStream, MediaRef, MediaStore};

pub async fn connect(cfg: &MediaConfig) -> MediaResult<Box<dyn MediaStore>> {
    backends::connect(cfg).await
}
