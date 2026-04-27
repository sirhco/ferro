//! GCS media backend.
//!
//! Uses `google-cloud-storage` with default application-credentials auth (GCE
//! metadata server, `GOOGLE_APPLICATION_CREDENTIALS`, or gcloud CLI).
//! Objects are stored under `bucket/[prefix/]key`. Uploads buffer to memory
//! (resumable uploads land with v0.5).

use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use futures::StreamExt;
use google_cloud_storage::{
    client::{Client, ClientConfig},
    http::objects::{
        delete::DeleteObjectRequest,
        download::Range,
        get::GetObjectRequest,
        upload::{Media, UploadObjectRequest, UploadType},
    },
    sign::{SignedURLMethod, SignedURLOptions},
};
use url::Url;

use crate::{
    config::MediaConfig,
    error::{MediaError, MediaResult},
    store::{ByteStream, MediaRef, MediaStore},
};

pub async fn connect(cfg: &MediaConfig) -> MediaResult<Box<dyn MediaStore>> {
    let MediaConfig::Gcs { bucket, prefix, .. } = cfg else {
        unreachable!();
    };
    let config = ClientConfig::default()
        .with_auth()
        .await
        .map_err(|e| MediaError::Backend(format!("gcs auth: {e}")))?;
    let client = Client::new(config);
    Ok(Box::new(GcsStore { client, bucket: bucket.clone(), prefix: prefix.clone() }))
}

pub struct GcsStore {
    pub(crate) client: Client,
    pub(crate) bucket: String,
    pub(crate) prefix: Option<String>,
}

impl std::fmt::Debug for GcsStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GcsStore").field("bucket", &self.bucket).finish()
    }
}

impl GcsStore {
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
impl MediaStore for GcsStore {
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
        let full = self.full_key(key);
        let mut media = Media::new(full);
        media.content_type = mime.to_string().into();
        let upload_type = UploadType::Simple(media);
        self.client
            .upload_object(
                &UploadObjectRequest { bucket: self.bucket.clone(), ..Default::default() },
                buf,
                &upload_type,
            )
            .await
            .map_err(backend)?;
        Ok(MediaRef { key: key.to_string(), size: uploaded, mime: mime.to_string(), url: None })
    }

    async fn get(&self, key: &str) -> MediaResult<ByteStream> {
        let data = self
            .client
            .download_object(
                &GetObjectRequest {
                    bucket: self.bucket.clone(),
                    object: self.full_key(key),
                    ..Default::default()
                },
                &Range::default(),
            )
            .await
            .map_err(|e| {
                let s = e.to_string();
                if s.contains("404") || s.to_lowercase().contains("not found") {
                    MediaError::NotFound
                } else {
                    MediaError::Backend(s)
                }
            })?;
        let out = futures::stream::once(async move { Ok::<_, std::io::Error>(Bytes::from(data)) });
        Ok(Box::pin(out))
    }

    async fn delete(&self, key: &str) -> MediaResult<()> {
        self.client
            .delete_object(&DeleteObjectRequest {
                bucket: self.bucket.clone(),
                object: self.full_key(key),
                ..Default::default()
            })
            .await
            .map_err(backend)?;
        Ok(())
    }

    async fn exists(&self, key: &str) -> MediaResult<bool> {
        match self
            .client
            .get_object(&GetObjectRequest {
                bucket: self.bucket.clone(),
                object: self.full_key(key),
                ..Default::default()
            })
            .await
        {
            Ok(_) => Ok(true),
            Err(e) => {
                let s = e.to_string();
                if s.contains("404") || s.to_lowercase().contains("not found") {
                    Ok(false)
                } else {
                    Err(MediaError::Backend(s))
                }
            }
        }
    }

    async fn presign_get(&self, key: &str, ttl: Duration) -> MediaResult<Url> {
        let opts = SignedURLOptions {
            method: SignedURLMethod::GET,
            expires: ttl,
            ..SignedURLOptions::default()
        };
        let url = self
            .client
            .signed_url(&self.bucket, &self.full_key(key), None, None, opts)
            .await
            .map_err(backend)?;
        Url::parse(&url).map_err(backend)
    }
}
