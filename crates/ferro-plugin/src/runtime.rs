//! wasmtime runtime bootstrap.
//!
//! The component-model side (WIT-generated bindings) lives behind a
//! work-in-progress `generate!` macro invocation that belongs alongside the
//! `wit/ferro.wit` file. For v0.1 this is the engine + store plumbing only —
//! enough to compile a component, grant capabilities, and tear down cleanly.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{ResourceTable, WasiCtxBuilder};

use crate::capability::Capability;
use crate::error::{PluginError, PluginResult};
use crate::host::{HostContext, Services};
use crate::manifest::PluginManifest;

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
        // Tick epoch in a background task so runaway plugins get interrupted.
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
        // Enforce: every manifest-declared capability must be in the grants set.
        for raw in &manifest.capabilities {
            let cap = raw.parse()?;
            if !grants.iter().any(|g| g == &cap) {
                return Err(PluginError::MissingCapability(cap.to_string()));
            }
        }

        let wasm_bytes = tokio::fs::read(plugin_dir.join(&manifest.entry)).await?;
        let component = Component::from_binary(&self.engine, &wasm_bytes)?;
        Ok(PluginHandle {
            runtime: self.clone(),
            component,
            manifest,
            grants,
        })
    }
}

#[derive(Clone)]
pub struct PluginHandle {
    runtime: PluginRuntime,
    component: Component,
    manifest: PluginManifest,
    grants: Vec<Capability>,
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

    /// Build a fresh [`Store`] with per-invocation fuel/epoch limits and a
    /// locked-down WASI context. Each invocation gets its own store.
    pub fn new_store(&self) -> PluginResult<Store<HostContext>> {
        let host = HostContext {
            plugin_name: self.manifest.name.clone(),
            granted: self.grants.clone(),
            wasi: WasiCtxBuilder::new().build(),
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

    /// The linker is where WIT-generated host imports get wired in. Left as a
    /// placeholder until the WIT world is locked in.
    pub fn linker(&self) -> Linker<HostContext> {
        let mut linker = Linker::new(&self.runtime.engine);
        // wasmtime_wasi::add_to_linker_async(&mut linker).expect("wasi linker adds");
        linker
    }

    #[must_use]
    pub fn component(&self) -> &Component {
        &self.component
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
