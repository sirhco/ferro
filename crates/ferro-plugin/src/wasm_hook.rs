//! Bridges a loaded WASM plugin into the in-process [`HookRegistry`].
//!
//! One `WasmPluginHook` per loaded plugin. Each fired event is routed through
//! [`PluginHandle::call_on_event`], which checks the plugin's manifest hooks
//! filter and dispatches to the WIT `on-event` export. Errors are logged but
//! not propagated — matches existing hook semantics in
//! [`crate::HookRegistry::dispatch`].

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::{
    error::PluginResult,
    hook::{HookEvent, HookHandler},
    runtime::PluginHandle,
};

/// Hook handler that forwards events to a wasmtime-backed plugin.
pub struct WasmPluginHook {
    plugin_name: String,
    /// Wrapped in `RwLock` so [`crate::PluginRegistry::reload`] can swap the
    /// underlying handle without re-registering with [`HookRegistry`].
    handle: Arc<RwLock<Arc<PluginHandle>>>,
    /// Marker used by `name()` for tracing output. Owned to avoid leaking the
    /// handle's lock through a borrow.
    display_name: String,
    enabled: Arc<std::sync::atomic::AtomicBool>,
}

impl std::fmt::Debug for WasmPluginHook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmPluginHook").field("plugin", &self.plugin_name).finish()
    }
}

impl WasmPluginHook {
    pub fn new(plugin_name: String, handle: Arc<PluginHandle>) -> Self {
        let display_name = format!("wasm:{plugin_name}");
        Self {
            plugin_name,
            handle: Arc::new(RwLock::new(handle)),
            display_name,
            enabled: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        }
    }

    /// Replace the underlying [`PluginHandle`] (used by [`crate::PluginRegistry::reload`]).
    pub async fn swap_handle(&self, new_handle: Arc<PluginHandle>) {
        *self.handle.write().await = new_handle;
    }

    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[async_trait]
impl HookHandler for WasmPluginHook {
    async fn handle(&self, event: &HookEvent) -> PluginResult<()> {
        if !self.is_enabled() {
            return Ok(());
        }
        let handle = self.handle.read().await.clone();
        handle.call_on_event(event).await
    }

    fn name(&self) -> &str {
        &self.display_name
    }

    fn plugin_name(&self) -> Option<&str> {
        Some(&self.plugin_name)
    }
}
