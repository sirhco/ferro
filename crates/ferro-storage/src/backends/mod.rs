use crate::config::StorageConfig;
use crate::error::{StorageError, StorageResult};
use crate::repo::Repository;

#[cfg(feature = "fs-json")]
pub mod fs_json;
#[cfg(feature = "fs-markdown")]
pub mod fs_markdown;
#[cfg(feature = "postgres")]
pub mod postgres;
#[cfg(feature = "surreal")]
pub mod surreal;

pub async fn connect(cfg: &StorageConfig) -> StorageResult<Box<dyn Repository>> {
    match cfg {
        #[cfg(feature = "surreal")]
        StorageConfig::SurrealEmbedded { .. } | StorageConfig::SurrealRemote { .. } => {
            surreal::connect(cfg).await
        }
        #[cfg(not(feature = "surreal"))]
        StorageConfig::SurrealEmbedded { .. } | StorageConfig::SurrealRemote { .. } => {
            Err(StorageError::BackendNotEnabled("surreal"))
        }

        #[cfg(feature = "postgres")]
        StorageConfig::Postgres { .. } => postgres::connect(cfg).await,
        #[cfg(not(feature = "postgres"))]
        StorageConfig::Postgres { .. } => Err(StorageError::BackendNotEnabled("postgres")),

        #[cfg(feature = "fs-json")]
        StorageConfig::FsJson { .. } => fs_json::connect(cfg).await,
        #[cfg(not(feature = "fs-json"))]
        StorageConfig::FsJson { .. } => Err(StorageError::BackendNotEnabled("fs-json")),

        #[cfg(feature = "fs-markdown")]
        StorageConfig::FsMarkdown { .. } => fs_markdown::connect(cfg).await,
        #[cfg(not(feature = "fs-markdown"))]
        StorageConfig::FsMarkdown { .. } => Err(StorageError::BackendNotEnabled("fs-markdown")),
    }
}
