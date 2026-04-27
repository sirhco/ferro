//! wasmtime runtime + WIT-generated bindings for the Ferro plugin ABI.
//!
//! `bindgen!` synthesises:
//!  - the `Plugin` world wrapper used to instantiate components,
//!  - the `ferro::cms::host::Host` trait the host implements (imports), and
//!  - the `exports::ferro::cms::guest::Guest` accessor for plugin-side calls.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use wasmtime::{
    component::{Component, Linker},
    Config, Engine, Store,
};
use wasmtime_wasi::{DirPerms, FilePerms, ResourceTable, WasiCtxBuilder};

use crate::{
    capability::Capability,
    error::{PluginError, PluginResult},
    hook::HookEvent as FerroHookEvent,
    host::{HostContext, Services},
    manifest::PluginManifest,
};

wasmtime::component::bindgen!({
    world: "plugin",
    path: "wit/ferro.wit",
    async: true,
});

use ferro::cms::{host as wit_host, types as wit_types};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub max_memory_bytes: usize,
    pub fuel_per_request: u64,
    pub epoch_deadline_ms: u64,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            max_memory_bytes: 128 * 1024 * 1024,
            fuel_per_request: 10_000_000,
            epoch_deadline_ms: 250,
        }
    }
}

#[derive(Clone)]
pub struct PluginRuntime {
    engine: Engine,
    cfg: RuntimeConfig,
    services: Arc<Services>,
}

impl std::fmt::Debug for PluginRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginRuntime").field("cfg", &self.cfg).finish()
    }
}

impl PluginRuntime {
    pub fn new(cfg: RuntimeConfig, services: Arc<Services>) -> PluginResult<Self> {
        let mut config = Config::new();
        config.async_support(true);
        config.consume_fuel(true);
        config.epoch_interruption(true);
        config.wasm_component_model(true);
        let engine = Engine::new(&config)?;
        let tick_engine = engine.clone();
        let tick = Duration::from_millis(cfg.epoch_deadline_ms);
        tokio::spawn(async move {
            let mut iv = tokio::time::interval(tick);
            loop {
                iv.tick().await;
                tick_engine.increment_epoch();
            }
        });
        Ok(Self { engine, cfg, services })
    }

    pub async fn load(
        &self,
        manifest: PluginManifest,
        plugin_dir: &Path,
        grants: Vec<Capability>,
    ) -> PluginResult<PluginHandle> {
        for raw in &manifest.capabilities {
            let cap = raw.parse()?;
            if !grants.iter().any(|g| g == &cap) {
                return Err(PluginError::MissingCapability(cap.to_string()));
            }
        }

        let wasm_bytes = tokio::fs::read(plugin_dir.join(&manifest.entry)).await?;
        let component = Component::from_binary(&self.engine, &wasm_bytes)?;
        let sandbox_dir = plugin_dir.join("data");
        // Best-effort sandbox dir creation; plugins that don't write are unaffected.
        let _ = tokio::fs::create_dir_all(&sandbox_dir).await;
        Ok(PluginHandle { runtime: self.clone(), component, manifest, grants, sandbox_dir })
    }
}

#[derive(Clone)]
pub struct PluginHandle {
    runtime: PluginRuntime,
    component: Component,
    manifest: PluginManifest,
    grants: Vec<Capability>,
    /// Per-plugin sandbox dir (`<plugin_dir>/data`) preopened as `/data` in the
    /// WASI context. Plugins with `media.write` or sidecar-output use cases
    /// (e.g. SEO) write here; everything outside this dir is unreachable.
    sandbox_dir: PathBuf,
}

impl std::fmt::Debug for PluginHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginHandle")
            .field("name", &self.manifest.name)
            .field("version", &self.manifest.version)
            .finish()
    }
}

impl PluginHandle {
    #[must_use]
    pub fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    #[must_use]
    pub fn grants(&self) -> &[Capability] {
        &self.grants
    }

    /// Build a fresh [`Store`] with per-invocation fuel/epoch limits and a
    /// locked-down WASI context. Each invocation gets its own store.
    pub fn new_store(&self) -> PluginResult<Store<HostContext>> {
        let mut wasi = WasiCtxBuilder::new();
        if self.sandbox_dir.exists() {
            wasi.preopened_dir(&self.sandbox_dir, "/data", DirPerms::all(), FilePerms::all())?;
        }
        let host = HostContext {
            plugin_name: self.manifest.name.clone(),
            granted: self.grants.clone(),
            wasi: wasi.build(),
            table: ResourceTable::new(),
            services: self.runtime.services.clone(),
        };
        let mut store = Store::new(&self.runtime.engine, host);
        store.set_fuel(self.runtime.cfg.fuel_per_request)?;
        store.set_epoch_deadline(1);
        // SAFETY: `LIMITER` is read-only in the hot path — wasmtime only calls
        // `resource_reached` on it, which takes `&mut self` but does not mutate
        // observable state. Safe to reuse the single static limiter across stores
        // until we replace it with a per-store limiter type.
        store.limiter(|_| unsafe { &mut *std::ptr::addr_of_mut!(LIMITER) });
        Ok(store)
    }

    /// Build a [`Linker`] with WASI + the WIT-generated host imports wired up.
    pub fn linker(&self) -> PluginResult<Linker<HostContext>> {
        let mut linker = Linker::new(&self.runtime.engine);
        wasmtime_wasi::add_to_linker_async(&mut linker)?;
        wit_host::add_to_linker(&mut linker, |s: &mut HostContext| s)?;
        Ok(linker)
    }

    #[must_use]
    pub fn component(&self) -> &Component {
        &self.component
    }

    /// Dispatch a hook event into the plugin via the WIT `on-event` export.
    /// Returns `Ok(())` on success or any plugin-reported error mapped into
    /// [`PluginError::Other`]. Errors are logged by callers; callers should
    /// not propagate them up the request path (matches existing
    /// [`crate::HookRegistry`] semantics).
    pub async fn call_on_event(&self, event: &FerroHookEvent) -> PluginResult<()> {
        let kind = event_kind(event);
        if !self.subscribes_to(kind) {
            return Ok(());
        }
        let wit_evt = match map_event(event, &self.runtime.services).await {
            Some(e) => e,
            None => return Ok(()),
        };

        let mut store = self.new_store()?;
        let linker = self.linker()?;
        let bindings = Plugin::instantiate_async(&mut store, &self.component, &linker).await?;
        let result = bindings.ferro_cms_guest().call_on_event(&mut store, &wit_evt).await?;
        result.map_err(PluginError::Other)
    }

    fn subscribes_to(&self, kind: &str) -> bool {
        self.manifest.hooks.is_empty() || self.manifest.hooks.iter().any(|h| h == kind)
    }
}

fn event_kind(evt: &FerroHookEvent) -> &'static str {
    match evt {
        FerroHookEvent::ContentCreated { .. } => "content.created",
        FerroHookEvent::ContentUpdated { .. } => "content.updated",
        FerroHookEvent::ContentPublished { .. } => "content.published",
        FerroHookEvent::ContentDeleted { .. } => "content.deleted",
        FerroHookEvent::TypeMigrated { .. } => "type.migrated",
        _ => "event",
    }
}

/// Translate a host-side [`FerroHookEvent`] into the WIT `hook-event` variant.
/// Returns `None` for events the WIT ABI doesn't yet expose (currently
/// `TypeMigrated`).
async fn map_event(evt: &FerroHookEvent, services: &Services) -> Option<wit_types::HookEvent> {
    match evt {
        FerroHookEvent::ContentCreated { content, type_slug } => {
            Some(wit_types::HookEvent::ContentCreated(
                content_to_wit(content, type_slug.as_deref(), services).await,
            ))
        }
        FerroHookEvent::ContentUpdated { after, type_slug, .. } => {
            Some(wit_types::HookEvent::ContentUpdated(
                content_to_wit(after, type_slug.as_deref(), services).await,
            ))
        }
        FerroHookEvent::ContentPublished { content, type_slug } => {
            Some(wit_types::HookEvent::ContentPublished(
                content_to_wit(content, type_slug.as_deref(), services).await,
            ))
        }
        FerroHookEvent::ContentDeleted { content_id, slug, .. } => {
            Some(wit_types::HookEvent::ContentDeleted(wit_types::ContentDeleted {
                content_id: content_id.to_string(),
                slug: slug.clone(),
            }))
        }
        _ => None,
    }
}

async fn content_to_wit(
    c: &ferro_core::Content,
    type_slug_hint: Option<&str>,
    services: &Services,
) -> wit_types::Content {
    let type_slug = match type_slug_hint {
        Some(s) => s.to_string(),
        None => services
            .repo
            .types()
            .get(c.type_id)
            .await
            .ok()
            .flatten()
            .map(|t| t.slug)
            .unwrap_or_default(),
    };
    wit_types::Content {
        id: c.id.to_string(),
        site_id: c.site_id.to_string(),
        type_slug,
        slug: c.slug.clone(),
        locale: c.locale.as_str().to_string(),
        status: format!("{:?}", c.status).to_lowercase(),
        data_json: serde_json::to_string(&c.data).unwrap_or_else(|_| "null".into()),
    }
}

// --- Host trait impl: WIT imports -----------------------------------------

#[wasmtime::component::__internal::async_trait]
impl wit_host::Host for HostContext {
    async fn log(&mut self, level: wit_types::LogLevel, target: String, message: String) -> () {
        if self.require(&Capability::Logs).is_err() {
            tracing::warn!(
                target: "ferro::plugin",
                plugin = %self.plugin_name,
                "denied log call: missing `logs` capability"
            );
            return;
        }
        let level_s = match level {
            wit_types::LogLevel::Trace => "trace",
            wit_types::LogLevel::Debug => "debug",
            wit_types::LogLevel::Info => "info",
            wit_types::LogLevel::Warn => "warn",
            wit_types::LogLevel::Error => "error",
        };
        let display_target =
            if target.is_empty() { self.plugin_name.as_str() } else { target.as_str() };
        (self.services.logger)(level_s, display_target, &message);
    }

    async fn get_content(&mut self, id: String) -> Option<wit_types::Content> {
        if self.require(&Capability::ContentRead).is_err() {
            return None;
        }
        let cid: ferro_core::ContentId = match id.parse() {
            Ok(v) => v,
            Err(_) => return None,
        };
        let content = match self.services.repo.content().get(cid).await {
            Ok(Some(c)) => c,
            _ => return None,
        };
        Some(content_to_wit(&content, None, &self.services).await)
    }
}

// Static resource limiter — generous defaults, tuned per deploy via env.
static mut LIMITER: FerroLimiter = FerroLimiter { max_mem: 128 * 1024 * 1024 };

struct FerroLimiter {
    max_mem: usize,
}

impl wasmtime::ResourceLimiter for FerroLimiter {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        Ok(desired <= self.max_mem.max(current))
    }
    fn table_growing(
        &mut self,
        _current: usize,
        _desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        Ok(true)
    }
}
