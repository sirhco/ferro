//! Loaded plugin registry.
//!
//! Holds the live set of WASM plugins, their per-plugin grants, and a thin
//! wrapper around each one ([`crate::WasmPluginHook`]) that the
//! [`crate::HookRegistry`] dispatches events to.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use serde::Serialize;
use tokio::{fs, sync::RwLock};

use crate::{
    capability::Capability,
    error::{PluginError, PluginResult},
    hook::HookRegistry,
    manifest::PluginManifest,
    runtime::{PluginHandle, PluginRuntime},
    wasm_hook::WasmPluginHook,
};

/// Operator-side grant for a single plugin (mirrors `[[plugins.grants]]` in
/// `ferro.toml`). Capability strings use the `Capability::FromStr` syntax —
/// e.g. `"content.read"`, `"http.fetch:api.example.com"`.
#[derive(Debug, Clone, Default)]
pub struct PluginGrant {
    pub name: String,
    pub capabilities: Vec<String>,
}

/// Public, JSON-serialisable description of a loaded plugin. Returned by REST
/// `/api/v1/plugins[/{name}]` and by `ferro plugin list|inspect`.
#[derive(Debug, Clone, Serialize)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub declared: Vec<String>,
    pub granted: Vec<String>,
    pub hooks: Vec<String>,
    pub enabled: bool,
}

#[derive(Clone)]
struct LoadedPlugin {
    handle: Arc<PluginHandle>,
    hook: Arc<WasmPluginHook>,
}

#[derive(Clone)]
pub struct PluginRegistry {
    runtime: PluginRuntime,
    dir: PathBuf,
    hooks: HookRegistry,
    plugins: Arc<RwLock<HashMap<String, LoadedPlugin>>>,
    grants: Arc<RwLock<HashMap<String, Vec<Capability>>>>,
}

impl std::fmt::Debug for PluginRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginRegistry").field("dir", &self.dir).finish()
    }
}

impl PluginRegistry {
    pub fn new(
        runtime: PluginRuntime,
        dir: impl Into<PathBuf>,
        hooks: HookRegistry,
        grants: &[PluginGrant],
    ) -> Self {
        let mut grant_map: HashMap<String, Vec<Capability>> = HashMap::new();
        for g in grants {
            let parsed: Vec<Capability> =
                g.capabilities.iter().filter_map(|c| c.parse().ok()).collect();
            grant_map.insert(g.name.clone(), parsed);
        }
        Self {
            runtime,
            dir: dir.into(),
            hooks,
            plugins: Arc::new(RwLock::new(HashMap::new())),
            grants: Arc::new(RwLock::new(grant_map)),
        }
    }

    /// Scan the plugin directory for `<plugin>/plugin.toml`, validate
    /// capabilities against the operator grants table, load the component, and
    /// register a [`WasmPluginHook`] with the [`HookRegistry`]. Idempotent —
    /// safe to re-call after [`Self::reload`].
    pub async fn scan(&self) -> PluginResult<()> {
        if !self.dir.exists() {
            fs::create_dir_all(&self.dir).await?;
            return Ok(());
        }
        let mut entries = fs::read_dir(&self.dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let ft = entry.file_type().await?;
            if !ft.is_dir() {
                continue;
            }
            let manifest_path = entry.path().join("plugin.toml");
            if !manifest_path.exists() {
                continue;
            }
            let manifest = match PluginManifest::load(&manifest_path).await {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!(target: "ferro::plugin", path = %manifest_path.display(), error = %e, "skipping plugin with invalid manifest");
                    continue;
                }
            };
            let plugin_dir = entry.path();
            if let Err(e) = self.load_one(manifest, &plugin_dir).await {
                tracing::warn!(target: "ferro::plugin", path = %plugin_dir.display(), error = %e, "skipping plugin");
            }
        }
        Ok(())
    }

    async fn load_one(&self, manifest: PluginManifest, plugin_dir: &Path) -> PluginResult<()> {
        let name = manifest.name.clone();
        let grants = self.resolved_grants(&name).await;
        let handle = Arc::new(self.runtime.load(manifest.clone(), plugin_dir, grants).await?);

        // Replace any existing hook for this plugin so reloads don't duplicate.
        self.hooks.unregister_plugin(&name).await;
        let hook = Arc::new(WasmPluginHook::new(name.clone(), handle.clone()));
        self.hooks.register(hook.clone()).await;

        self.plugins.write().await.insert(name, LoadedPlugin { handle, hook });
        Ok(())
    }

    async fn resolved_grants(&self, name: &str) -> Vec<Capability> {
        self.grants.read().await.get(name).cloned().unwrap_or_default()
    }

    pub async fn get(&self, name: &str) -> PluginResult<Arc<PluginHandle>> {
        self.plugins
            .read()
            .await
            .get(name)
            .map(|p| p.handle.clone())
            .ok_or_else(|| PluginError::NotFound(name.to_string()))
    }

    pub async fn list(&self) -> Vec<String> {
        self.plugins.read().await.keys().cloned().collect()
    }

    pub async fn describe_all(&self) -> Vec<PluginInfo> {
        let plugins = self.plugins.read().await;
        let mut out: Vec<PluginInfo> = plugins.values().map(plugin_info).collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    pub async fn describe(&self, name: &str) -> PluginResult<PluginInfo> {
        self.plugins
            .read()
            .await
            .get(name)
            .map(plugin_info)
            .ok_or_else(|| PluginError::NotFound(name.to_string()))
    }

    /// Drop every loaded plugin and re-scan the plugins directory. Hot-swap
    /// safe: in-flight calls hold their own `Arc<PluginHandle>` and finish on
    /// the old store before dropping it.
    pub async fn reload(&self) -> PluginResult<()> {
        // Detach all current plugin hooks; scan() will re-register.
        let names: Vec<String> = self.plugins.read().await.keys().cloned().collect();
        for n in &names {
            self.hooks.unregister_plugin(n).await;
        }
        self.plugins.write().await.clear();
        self.scan().await
    }

    /// Update grants for a single plugin and reload just that one. Persisted
    /// only in-memory — operators must edit `ferro.toml` for the change to
    /// survive a restart.
    pub async fn set_grants(&self, name: &str, capabilities: Vec<String>) -> PluginResult<()> {
        let parsed: Vec<Capability> =
            capabilities.iter().map(|s| s.parse()).collect::<PluginResult<Vec<_>>>()?;
        self.grants.write().await.insert(name.to_string(), parsed);
        self.reload_one(name).await
    }

    pub async fn set_enabled(&self, name: &str, enabled: bool) -> PluginResult<()> {
        let plugins = self.plugins.read().await;
        let Some(p) = plugins.get(name) else {
            return Err(PluginError::NotFound(name.to_string()));
        };
        p.hook.set_enabled(enabled);
        Ok(())
    }

    async fn reload_one(&self, name: &str) -> PluginResult<()> {
        let plugin_dir = self.dir.join(name);
        let manifest_path = plugin_dir.join("plugin.toml");
        if !manifest_path.exists() {
            return Err(PluginError::NotFound(name.to_string()));
        }
        let manifest = PluginManifest::load(&manifest_path).await?;
        self.load_one(manifest, &plugin_dir).await
    }
}

fn plugin_info(p: &LoadedPlugin) -> PluginInfo {
    let m = p.handle.manifest();
    PluginInfo {
        name: m.name.clone(),
        version: m.version.clone(),
        description: m.description.clone(),
        declared: m.capabilities.iter().map(|c| c.0.clone()).collect(),
        granted: p.handle.grants().iter().map(|c| c.to_string()).collect(),
        hooks: m.hooks.clone(),
        enabled: p.hook.is_enabled(),
    }
}

pub fn default_plugin_dir() -> PathBuf {
    Path::new("./plugins").to_path_buf()
}
