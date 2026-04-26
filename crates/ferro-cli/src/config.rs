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
    /// Optional outbound webhook subscribers. Each entry registers as a
    /// `HookHandler` at startup; missing section is treated as empty.
    #[serde(default)]
    pub webhooks: Vec<ferro_plugin::WebhookConfig>,
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
    /// HMAC secret used to sign JWTs. Prefer setting via `FERRO_JWT_SECRET`.
    /// Falls back to `session_secret` when unset; warn loudly in that case.
    #[serde(default)]
    pub jwt_secret: Option<String>,
    #[serde(default)]
    pub allow_public_signup: bool,
}

impl AuthConfig {
    /// Resolve the JWT signing secret, checking `FERRO_JWT_SECRET` first, then
    /// `auth.jwt_secret`, then falling back to `session_secret` with a warning.
    pub fn resolve_jwt_secret(&self) -> String {
        if let Ok(v) = std::env::var("FERRO_JWT_SECRET") {
            if !v.is_empty() {
                return v;
            }
        }
        if let Some(v) = self.jwt_secret.as_deref() {
            if !v.is_empty() {
                return v.to_string();
            }
        }
        tracing::warn!(
            "auth.jwt_secret not set and FERRO_JWT_SECRET not in env; reusing session_secret for JWT signing"
        );
        self.session_secret.clone()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    pub dir: PathBuf,
    pub max_memory_mb: usize,
    pub fuel_per_request: u64,
    /// Operator-side capability grants per plugin. A plugin whose name is
    /// absent from this list loads with empty grants, and is rejected if its
    /// manifest declares any capabilities.
    #[serde(default)]
    pub grants: Vec<PluginGrantConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginGrantConfig {
    pub name: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl PluginGrantConfig {
    pub fn to_grant(&self) -> ferro_plugin::PluginGrant {
        ferro_plugin::PluginGrant {
            name: self.name.clone(),
            capabilities: self.capabilities.clone(),
        }
    }
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
