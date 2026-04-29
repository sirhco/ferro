use crate::{config::MediaConfig, error::MediaResult, store::MediaStore};

#[cfg(feature = "gcs")]
pub mod gcs;
#[cfg(feature = "local")]
pub mod local;
#[cfg(feature = "s3")]
pub mod s3;

pub async fn connect(cfg: &MediaConfig) -> MediaResult<Box<dyn MediaStore>> {
    match cfg {
        #[cfg(feature = "local")]
        MediaConfig::Local { .. } => local::connect(cfg).await,
        #[cfg(not(feature = "local"))]
        MediaConfig::Local { .. } => Err(crate::error::MediaError::BackendNotEnabled("local")),
        #[cfg(feature = "s3")]
        MediaConfig::S3 { .. } => s3::connect(cfg).await,
        #[cfg(not(feature = "s3"))]
        MediaConfig::S3 { .. } => Err(crate::error::MediaError::BackendNotEnabled("s3")),
        #[cfg(feature = "gcs")]
        MediaConfig::Gcs { .. } => gcs::connect(cfg).await,
        #[cfg(not(feature = "gcs"))]
        MediaConfig::Gcs { .. } => Err(crate::error::MediaError::BackendNotEnabled("gcs")),
    }
}
