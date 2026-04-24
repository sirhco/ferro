use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum StorageConfig {
    SurrealEmbedded { path: PathBuf, namespace: String, database: String },
    SurrealRemote { url: String, namespace: String, database: String, user: String, pass: String },
    Postgres { url: String, max_conns: u32 },
    FsJson { path: PathBuf },
    FsMarkdown { path: PathBuf },
}

impl StorageConfig {
    #[must_use]
    pub fn backend_name(&self) -> &'static str {
        match self {
            Self::SurrealEmbedded { .. } | Self::SurrealRemote { .. } => "surreal",
            Self::Postgres { .. } => "postgres",
            Self::FsJson { .. } => "fs-json",
            Self::FsMarkdown { .. } => "fs-markdown",
        }
    }
}
