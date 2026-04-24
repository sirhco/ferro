//! S3-compatible media backend.
//!
//! Uses `aws-sdk-s3`. Objects are stored under `bucket/[prefix/]key`. Reads
//! stream through `aws_sdk_s3::primitives::ByteStream`; writes buffer into a
//! single byte blob (streaming multipart-uploads land with v0.5).

use std::time::Duration;

use async_trait::async_trait;
use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::operation::get_object::GetObjectError;
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::primitives::ByteStream as S3ByteStream;
use bytes::Bytes;
use futures::StreamExt;
use url::Url;

use crate::config::MediaConfig;
use crate::error::{MediaError, MediaResult};
use crate::store::{ByteStream, MediaRef, MediaStore};

pub async fn connect(cfg: &MediaConfig) -> MediaResult<Box<dyn MediaStore>> {
    let MediaConfig::S3 { bucket, region, prefix } = cfg else {
        unreachable!();
    };
    let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(aws_config::Region::new(region.clone()))
        .load()
        .await;
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

impl S3Store {
    fn full_key(&self, key: &str) -> String {
        match &self.prefix {
            Some(p) if !p.is_empty() => format!("{}/{}", p.trim_end_matches('/'), key),
            _ => key.to_string(),
        }
    }
}

fn backend<E: std::fmt::Display>(e: E) -> MediaError {
    MediaError::Backend(e.to_string())
}

#[async_trait]
impl MediaStore for S3Store {
    async fn put(
        &self,
        key: &str,
        mut body: ByteStream,
        mime: &str,
        size: u64,
    ) -> MediaResult<MediaRef> {
        let mut buf: Vec<u8> = Vec::with_capacity(size as usize);
        while let Some(chunk) = body.next().await {
            buf.extend_from_slice(&chunk?);
        }
        let uploaded = buf.len() as u64;
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(self.full_key(key))
            .content_type(mime)
            .body(S3ByteStream::from(buf))
            .send()
            .await
            .map_err(backend)?;
        Ok(MediaRef { key: key.to_string(), size: uploaded, mime: mime.to_string(), url: None })
    }

    async fn get(&self, key: &str) -> MediaResult<ByteStream> {
        let resp = match self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(self.full_key(key))
            .send()
            .await
        {
            Ok(r) => r,
            Err(SdkError::ServiceError(e)) if matches!(e.err(), GetObjectError::NoSuchKey(_)) => {
                return Err(MediaError::NotFound);
            }
            Err(e) => return Err(backend(e)),
        };
        // Drain into memory to decouple from the SDK's internal stream types.
        // Swap for a streaming adapter in v0.5.
        let data = resp.body.collect().await.map_err(backend)?.into_bytes();
        let out = futures::stream::once(async move { Ok::<_, std::io::Error>(Bytes::from(data)) });
        Ok(Box::pin(out))
    }

    async fn delete(&self, key: &str) -> MediaResult<()> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(self.full_key(key))
            .send()
            .await
            .map_err(backend)?;
        Ok(())
    }

    async fn exists(&self, key: &str) -> MediaResult<bool> {
        match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(self.full_key(key))
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(SdkError::ServiceError(e)) if e.raw().status().as_u16() == 404 => Ok(false),
            Err(e) => Err(backend(e)),
        }
    }

    async fn presign_get(&self, key: &str, ttl: Duration) -> MediaResult<Url> {
        let cfg = PresigningConfig::expires_in(ttl).map_err(backend)?;
        let presigned = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(self.full_key(key))
            .presigned(cfg)
            .await
            .map_err(backend)?;
        Url::parse(presigned.uri()).map_err(backend)
    }
}
