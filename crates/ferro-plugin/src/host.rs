//! Host-side state a plugin sees through the component ABI.

use std::sync::Arc;

use wasmtime_wasi::{WasiCtx, WasiView};

use crate::{capability::Capability, hook::HookRegistry};

/// Per-invocation host context. Holds granted capabilities + references back
/// to the rest of the system. One `HostContext` is created per `wasmtime::Store`
/// (per plugin invocation) and dropped when the call returns.
pub struct HostContext {
    pub plugin_name: String,
    pub granted: Vec<Capability>,
    pub wasi: WasiCtx,
    pub table: wasmtime_wasi::ResourceTable,
    pub services: Arc<Services>,
}

/// Real services a plugin can reach across the component ABI. Built once at
/// startup and shared `Arc`-cloned into each `HostContext`.
pub struct Services {
    pub repo: Arc<dyn ferro_storage::Repository>,
    /// `(level, target, message)` — invoked from the host `log` import.
    pub logger: Arc<dyn Fn(&str, &str, &str) + Send + Sync>,
    pub hooks: HookRegistry,
}

impl Services {
    /// Build a `Services` with a default `tracing`-backed logger that emits
    /// under the `ferro::plugin` target with the calling plugin's name in a
    /// `plugin = …` field.
    pub fn new(repo: Arc<dyn ferro_storage::Repository>, hooks: HookRegistry) -> Self {
        let logger: Arc<dyn Fn(&str, &str, &str) + Send + Sync> =
            Arc::new(|level: &str, target: &str, msg: &str| match level {
                "trace" => {
                    tracing::trace!(target: "ferro::plugin", plugin = target, "{msg}")
                }
                "debug" => {
                    tracing::debug!(target: "ferro::plugin", plugin = target, "{msg}")
                }
                "warn" => {
                    tracing::warn!(target: "ferro::plugin", plugin = target, "{msg}")
                }
                "error" => {
                    tracing::error!(target: "ferro::plugin", plugin = target, "{msg}")
                }
                _ => {
                    tracing::info!(target: "ferro::plugin", plugin = target, "{msg}")
                }
            });
        Self { repo, hooks, logger }
    }
}

impl HostContext {
    pub fn has(&self, want: &Capability) -> bool {
        self.granted.iter().any(|g| capability_covers(g, want))
    }

    pub fn require(&self, want: &Capability) -> crate::error::PluginResult<()> {
        if self.has(want) {
            Ok(())
        } else {
            Err(crate::error::PluginError::MissingCapability(want.to_string()))
        }
    }
}

fn capability_covers(granted: &Capability, want: &Capability) -> bool {
    use Capability as C;
    match (granted, want) {
        (C::HttpFetch { host: g }, C::HttpFetch { host: w }) => g == "*" || g == w,
        (C::HttpServe { prefix: g }, C::HttpServe { prefix: w }) => w.starts_with(g.as_str()),
        (a, b) => a == b,
    }
}

impl WasiView for HostContext {
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.wasi
    }
    fn table(&mut self) -> &mut wasmtime_wasi::ResourceTable {
        &mut self.table
    }
}
