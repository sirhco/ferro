use std::pin::Pin;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use futures::Stream;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::MediaResult;

/// Generic byte stream the store can accept or return. We use a pinned dyn
/// stream so callers are decoupled from any specific SDK's type.
pub type ByteStream =
    Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaRef {
    pub key: String,
    pub size: u64,
    pub mime: String,
    pub url: Option<Url>,
}

#[async_trait]
pub trait MediaStore: Send + Sync {
    async fn put(&self, key: &str, body: ByteStream, mime: &str, size: u64) -> MediaResult<MediaRef>;
    async fn get(&self, key: &str) -> MediaResult<ByteStream>;
    async fn delete(&self, key: &str) -> MediaResult<()>;
    async fn exists(&self, key: &str) -> MediaResult<bool>;
    async fn presign_get(&self, key: &str, ttl: Duration) -> MediaResult<Url>;
}
