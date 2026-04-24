use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum MediaConfig {
    Local { path: PathBuf, base_url: Option<String> },
    S3 { bucket: String, region: String, prefix: Option<String> },
    Gcs { bucket: String, prefix: Option<String>, service_account_path: Option<PathBuf> },
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
