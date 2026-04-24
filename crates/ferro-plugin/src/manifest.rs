use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::capability::Capability;
use crate::error::{PluginError, PluginResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub entry: String,
    #[serde(default)]
    pub capabilities: Vec<RawCapability>,
    #[serde(default)]
    pub hooks: Vec<String>,
    #[serde(default)]
    pub config_schema: Option<serde_json::Value>,
}

/// Raw string form in `plugin.toml`, parsed lazily into [`Capability`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RawCapability(pub String);

impl PluginManifest {
    pub async fn load(path: impl AsRef<Path>) -> PluginResult<Self> {
        let bytes = tokio::fs::read(path.as_ref()).await?;
        let s = std::str::from_utf8(&bytes)
            .map_err(|e| PluginError::Manifest(e.to_string()))?;
        let m: Self = toml::from_str(s)?;
        m.validate()?;
        Ok(m)
    }

    pub fn validate(&self) -> PluginResult<()> {
        if self.name.is_empty() {
            return Err(PluginError::Manifest("name required".into()));
        }
        if self.entry.is_empty() {
            return Err(PluginError::Manifest("entry required".into()));
        }
        for c in &self.capabilities {
            c.parse()?;
        }
        Ok(())
    }

    pub fn capabilities_parsed(&self) -> PluginResult<Vec<Capability>> {
        self.capabilities.iter().map(RawCapability::parse).collect()
    }
}

impl RawCapability {
    pub fn parse(&self) -> PluginResult<Capability> {
        self.0.parse()
    }
}
