//! S3-compatible media backend.
//!
//! Uses `aws-sdk-s3`. Objects are stored under `bucket/[prefix/]key`. Reads
//! stream through `aws_sdk_s3::primitives::ByteStream` chunk-by-chunk; writes
//! switch to multipart upload above [`MULTIPART_THRESHOLD`], so peak RSS stays
//! bounded by [`PART_SIZE`] regardless of total object size.

use std::time::Duration;

use async_trait::async_trait;
use aws_sdk_s3::{
    error::SdkError,
    operation::get_object::GetObjectError,
    presigning::PresigningConfig,
    primitives::ByteStream as S3ByteStream,
    types::{CompletedMultipartUpload, CompletedPart},
};
use bytes::Bytes;
use futures::StreamExt;
use url::Url;

use crate::{
    config::MediaConfig,
    error::{MediaError, MediaResult},
    store::{ByteStream, MediaRef, MediaStore},
};

/// S3 minimum part size for multipart upload (5 MiB). All parts except the
/// last must be at least this large.
const PART_SIZE: usize = 5 * 1024 * 1024;
/// Switch to multipart above this size. Below it, a single PUT is cheaper
/// and avoids the create/complete round-trips.
const MULTIPART_THRESHOLD: u64 = 8 * 1024 * 1024;

pub async fn connect(cfg: &MediaConfig) -> MediaResult<Box<dyn MediaStore>> {
    let MediaConfig::S3 {
        bucket,
        region,
        prefix,
        endpoint,
        force_path_style,
        access_key_id,
        secret_access_key,
    } = cfg
    else {
        unreachable!();
    };

    let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(aws_config::Region::new(region.clone()));
    if let Some(ep) = endpoint.as_deref() {
        loader = loader.endpoint_url(ep);
    }
    if let (Some(akid), Some(secret)) = (access_key_id.as_deref(), secret_access_key.as_deref()) {
        loader = loader.credentials_provider(aws_sdk_s3::config::Credentials::new(
            akid, secret, None, None, "ferro-config",
        ));
    }
    let shared = loader.load().await;

    let mut builder = aws_sdk_s3::config::Builder::from(&shared);
    if matches!(force_path_style, Some(true)) {
        builder = builder.force_path_style(true);
    }
    let client = aws_sdk_s3::Client::from_conf(builder.build());

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

    /// Buffered single-PUT path. Used for small payloads where the multipart
    /// round-trips would dominate.
    async fn put_single(&self, key_full: &str, mut body: ByteStream, mime: &str) -> MediaResult<u64> {
        let mut buf: Vec<u8> = Vec::new();
        while let Some(chunk) = body.next().await {
            buf.extend_from_slice(&chunk?);
        }
        let uploaded = buf.len() as u64;
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key_full)
            .content_type(mime)
            .body(S3ByteStream::from(buf))
            .send()
            .await
            .map_err(backend)?;
        Ok(uploaded)
    }

    /// Streaming multipart path. Buffers up to `PART_SIZE` per chunk, never
    /// holding more than one part in memory.
    async fn put_multipart(
        &self,
        key_full: &str,
        mut body: ByteStream,
        mime: &str,
    ) -> MediaResult<u64> {
        let create = self
            .client
            .create_multipart_upload()
            .bucket(&self.bucket)
            .key(key_full)
            .content_type(mime)
            .send()
            .await
            .map_err(backend)?;
        let upload_id = create
            .upload_id()
            .ok_or_else(|| MediaError::Backend("S3 missing upload_id".into()))?
            .to_string();

        let result = self.put_multipart_inner(key_full, &upload_id, &mut body).await;
        match result {
            Ok((parts, total)) => {
                self.client
                    .complete_multipart_upload()
                    .bucket(&self.bucket)
                    .key(key_full)
                    .upload_id(&upload_id)
                    .multipart_upload(
                        CompletedMultipartUpload::builder().set_parts(Some(parts)).build(),
                    )
                    .send()
                    .await
                    .map_err(backend)?;
                Ok(total)
            }
            Err(e) => {
                let _ = self
                    .client
                    .abort_multipart_upload()
                    .bucket(&self.bucket)
                    .key(key_full)
                    .upload_id(&upload_id)
                    .send()
                    .await;
                Err(e)
            }
        }
    }

    async fn put_multipart_inner(
        &self,
        key_full: &str,
        upload_id: &str,
        body: &mut ByteStream,
    ) -> MediaResult<(Vec<CompletedPart>, u64)> {
        let mut parts: Vec<CompletedPart> = Vec::new();
        let mut buf: Vec<u8> = Vec::with_capacity(PART_SIZE);
        let mut total: u64 = 0;
        let mut part_num: i32 = 1;

        while let Some(chunk) = body.next().await {
            let chunk = chunk?;
            total += chunk.len() as u64;
            buf.extend_from_slice(&chunk);
            if buf.len() >= PART_SIZE {
                let payload = std::mem::replace(&mut buf, Vec::with_capacity(PART_SIZE));
                self.upload_one_part(key_full, upload_id, part_num, payload, &mut parts).await?;
                part_num += 1;
            }
        }
        if !buf.is_empty() {
            self.upload_one_part(key_full, upload_id, part_num, buf, &mut parts).await?;
        }
        Ok((parts, total))
    }

    async fn upload_one_part(
        &self,
        key_full: &str,
        upload_id: &str,
        part_num: i32,
        payload: Vec<u8>,
        parts: &mut Vec<CompletedPart>,
    ) -> MediaResult<()> {
        let resp = self
            .client
            .upload_part()
            .bucket(&self.bucket)
            .key(key_full)
            .upload_id(upload_id)
            .part_number(part_num)
            .body(S3ByteStream::from(payload))
            .send()
            .await
            .map_err(backend)?;
        parts.push(
            CompletedPart::builder().part_number(part_num).set_e_tag(resp.e_tag().map(str::to_owned)).build(),
        );
        Ok(())
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
        body: ByteStream,
        mime: &str,
        size: u64,
    ) -> MediaResult<MediaRef> {
        let key_full = self.full_key(key);
        let uploaded = if size <= MULTIPART_THRESHOLD {
            self.put_single(&key_full, body, mime).await?
        } else {
            self.put_multipart(&key_full, body, mime).await?
        };
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
        let stream = futures::stream::unfold(resp.body, |mut body| async move {
            match body.next().await {
                Some(Ok(bytes)) => Some((Ok::<Bytes, std::io::Error>(bytes), body)),
                Some(Err(e)) => Some((Err(std::io::Error::other(e)), body)),
                None => None,
            }
        });
        Ok(Box::pin(stream))
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
        match self.client.head_object().bucket(&self.bucket).key(self.full_key(key)).send().await {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Construct a store pointed at an arbitrary endpoint with explicit
    /// credentials and `force_path_style`. Smoke-tests the wiring without
    /// hitting the network — any failure here is a config plumbing bug.
    #[tokio::test]
    async fn connect_with_endpoint_override() {
        let cfg = MediaConfig::S3 {
            bucket: "test-bucket".into(),
            region: "auto".into(),
            prefix: None,
            endpoint: Some("http://127.0.0.1:9000".into()),
            force_path_style: Some(true),
            access_key_id: Some("minioadmin".into()),
            secret_access_key: Some("minioadmin".into()),
        };
        let _store = connect(&cfg).await.expect("connect should succeed offline");
    }
}
