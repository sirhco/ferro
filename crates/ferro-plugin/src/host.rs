//! Host-side state a plugin sees through the component ABI.

use std::sync::Arc;

use wasmtime_wasi::{WasiCtx, WasiView};

use crate::capability::Capability;

/// Per-invocation host context. Holds granted capabilities + references back
/// to the rest of the system.
pub struct HostContext {
    pub plugin_name: String,
    pub granted: Vec<Capability>,
    pub wasi: WasiCtx,
    pub table: wasmtime_wasi::ResourceTable,
    pub services: Arc<Services>,
}

/// Placeholder for the set of services plugins can reach. Wire real impls in
/// once the surface firms up. For MVP: content repo + logger.
pub struct Services {
    // pub repo: Arc<dyn ferro_storage::Repository>,
    // pub logger: Arc<dyn Fn(&str, &str) + Send + Sync>,
    pub _placeholder: (),
}

impl Default for Services {
    fn default() -> Self {
        Self { _placeholder: () }
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
        (C::HttpServe { prefix: g }, C::HttpServe { prefix: w }) => {
            w.starts_with(g.as_str())
        }
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
