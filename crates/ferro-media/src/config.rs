use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum MediaConfig {
    Local {
        path: PathBuf,
        base_url: Option<String>,
    },
    S3 {
        bucket: String,
        region: String,
        prefix: Option<String>,
        /// Override the AWS S3 endpoint. Set this for S3-compatible services:
        /// Cloudflare R2 (`https://<account-id>.r2.cloudflarestorage.com`),
        /// MinIO, DigitalOcean Spaces, Backblaze B2 S3-API, etc. When `None`,
        /// the default AWS S3 endpoint for `region` is used.
        #[serde(default)]
        endpoint: Option<String>,
        /// Force path-style addressing (`https://endpoint/bucket/key`) instead
        /// of virtual-host style (`https://bucket.endpoint/key`). Required by
        /// MinIO and some R2 setups; AWS S3 itself ignores this in modern
        /// regions.
        #[serde(default)]
        force_path_style: Option<bool>,
        /// Explicit access key. When `None`, the standard AWS credential chain
        /// is used (env vars, `~/.aws/credentials`, IAM role).
        #[serde(default)]
        access_key_id: Option<String>,
        /// Explicit secret key. Pair with `access_key_id`. Prefer the env
        /// chain in production; this field exists for non-AWS providers
        /// (R2, MinIO) where the credentials are scoped to the bucket.
        #[serde(default)]
        secret_access_key: Option<String>,
    },
    Gcs {
        bucket: String,
        prefix: Option<String>,
        service_account_path: Option<PathBuf>,
    },
}

impl MediaConfig {
    #[must_use]
    pub fn backend_name(&self) -> &'static str {
        match self {
            Self::Local { .. } => "local",
            Self::S3 { .. } => "s3",
            Self::Gcs { .. } => "gcs",
        }
    }
}
