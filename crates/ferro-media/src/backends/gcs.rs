//! GCS backend scaffold.

use std::time::Duration;

use async_trait::async_trait;
use url::Url;

use crate::config::MediaConfig;
use crate::error::{MediaError, MediaResult};
use crate::store::{ByteStream, MediaRef, MediaStore};

pub async fn connect(cfg: &MediaConfig) -> MediaResult<Box<dyn MediaStore>> {
    let MediaConfig::Gcs { bucket, prefix, .. } = cfg else {
        unreachable!();
    };
    Ok(Box::new(GcsStore { bucket: bucket.clone(), prefix: prefix.clone() }))
}

pub struct GcsStore {
    pub(crate) bucket: String,
    pub(crate) prefix: Option<String>,
}

impl std::fmt::Debug for GcsStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GcsStore").field("bucket", &self.bucket).finish()
    }
}

#[async_trait]
impl MediaStore for GcsStore {
    async fn put(&self, _key: &str, _body: ByteStream, _mime: &str, _size: u64) -> MediaResult<MediaRef> {
        Err(MediaError::Backend("gcs put not yet implemented".into()))
    }
    async fn get(&self, _key: &str) -> MediaResult<ByteStream> {
        Err(MediaError::Backend("gcs get not yet implemented".into()))
    }
    async fn delete(&self, _key: &str) -> MediaResult<()> {
        Err(MediaError::Backend("gcs delete not yet implemented".into()))
    }
    async fn exists(&self, _key: &str) -> MediaResult<bool> {
        Err(MediaError::Backend("gcs exists not yet implemented".into()))
    }
    async fn presign_get(&self, _key: &str, _ttl: Duration) -> MediaResult<Url> {
        Err(MediaError::Backend("gcs presign not yet implemented".into()))
    }
}
