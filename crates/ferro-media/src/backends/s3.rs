//! S3 backend scaffold. Fill in once credentials + bucket shape are decided.

use std::time::Duration;

use async_trait::async_trait;
use url::Url;

use crate::config::MediaConfig;
use crate::error::{MediaError, MediaResult};
use crate::store::{ByteStream, MediaRef, MediaStore};

pub async fn connect(cfg: &MediaConfig) -> MediaResult<Box<dyn MediaStore>> {
    let MediaConfig::S3 { bucket, region, prefix } = cfg else {
        unreachable!();
    };
    let config = aws_config::from_env().region(aws_config::Region::new(region.clone())).load().await;
    let client = aws_sdk_s3::Client::new(&config);
    Ok(Box::new(S3Store { client, bucket: bucket.clone(), prefix: prefix.clone() }))
}

pub struct S3Store {
    pub(crate) client: aws_sdk_s3::Client,
    pub(crate) bucket: String,
    pub(crate) prefix: Option<String>,
}

impl std::fmt::Debug for S3Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3Store").field("bucket", &self.bucket).finish()
    }
}

#[async_trait]
impl MediaStore for S3Store {
    async fn put(&self, _key: &str, _body: ByteStream, _mime: &str, _size: u64) -> MediaResult<MediaRef> {
        Err(MediaError::Backend("s3 put not yet implemented".into()))
    }
    async fn get(&self, _key: &str) -> MediaResult<ByteStream> {
        Err(MediaError::Backend("s3 get not yet implemented".into()))
    }
    async fn delete(&self, _key: &str) -> MediaResult<()> {
        Err(MediaError::Backend("s3 delete not yet implemented".into()))
    }
    async fn exists(&self, _key: &str) -> MediaResult<bool> {
        Err(MediaError::Backend("s3 exists not yet implemented".into()))
    }
    async fn presign_get(&self, _key: &str, _ttl: Duration) -> MediaResult<Url> {
        Err(MediaError::Backend("s3 presign not yet implemented".into()))
    }
}
