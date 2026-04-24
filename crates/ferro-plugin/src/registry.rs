use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::fs;
use tokio::sync::RwLock;

use crate::error::{PluginError, PluginResult};
use crate::manifest::PluginManifest;
use crate::runtime::{PluginHandle, PluginRuntime};

#[derive(Clone)]
pub struct PluginRegistry {
    runtime: PluginRuntime,
    dir: PathBuf,
    plugins: Arc<RwLock<HashMap<String, PluginHandle>>>,
}

impl std::fmt::Debug for PluginRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginRegistry").field("dir", &self.dir).finish()
    }
}

impl PluginRegistry {
    pub fn new(runtime: PluginRuntime, dir: impl Into<PathBuf>) -> Self {
        Self { runtime, dir: dir.into(), plugins: Arc::new(RwLock::new(HashMap::new())) }
    }

    /// Scan the plugin directory and load any `plugin.toml` manifests.
    pub async fn scan(&self) -> PluginResult<()> {
        if !self.dir.exists() {
            fs::create_dir_all(&self.dir).await?;
            return Ok(());
        }
        let mut dir = fs::read_dir(&self.dir).await?;
        while let Some(entry) = dir.next_entry().await? {
            let ft = entry.file_type().await?;
            if !ft.is_dir() {
                continue;
            }
            let manifest_path = entry.path().join("plugin.toml");
            if !manifest_path.exists() {
                continue;
            }
            let manifest = PluginManifest::load(&manifest_path).await?;
            // MVP grant policy: grant everything the manifest asks for. In
            // prod, gate behind the admin UI's per-plugin approval.
            let grants = manifest.capabilities_parsed()?;
            let handle = self.runtime.load(manifest.clone(), &entry.path(), grants).await?;
            self.plugins.write().await.insert(manifest.name, handle);
        }
        Ok(())
    }

    pub async fn get(&self, name: &str) -> PluginResult<PluginHandle> {
        self.plugins
            .read()
            .await
            .get(name)
            .cloned()
            .ok_or_else(|| PluginError::NotFound(name.to_string()))
    }

    pub async fn list(&self) -> Vec<String> {
        self.plugins.read().await.keys().cloned().collect()
    }
}

pub fn default_plugin_dir() -> PathBuf {
    Path::new("./plugins").to_path_buf()
}
