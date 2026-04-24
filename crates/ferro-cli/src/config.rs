use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use ferro_media::MediaConfig;
use ferro_storage::StorageConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FerroConfig {
    pub server: ServerConfig,
    pub storage: StorageConfig,
    pub media: MediaConfig,
    pub auth: AuthConfig,
    pub plugins: PluginConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub bind: String,
    pub public_url: Option<String>,
    #[serde(default)]
    pub admin_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    pub session_secret: String,
    pub jwt_issuer: String,
    #[serde(default)]
    pub allow_public_signup: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    pub dir: PathBuf,
    pub max_memory_mb: usize,
    pub fuel_per_request: u64,
}

impl FerroConfig {
    pub async fn load(path: &Path) -> Result<Self> {
        let bytes = tokio::fs::read(path)
            .await
            .with_context(|| format!("reading config {}", path.display()))?;
        let s = std::str::from_utf8(&bytes).context("config is not utf-8")?;
        let cfg: Self = toml::from_str(s).context("parsing ferro.toml")?;
        Ok(cfg)
    }
}
