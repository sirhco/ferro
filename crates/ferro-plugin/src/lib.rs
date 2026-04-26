//! Ferro plugin host.
//!
//! Built on `wasmtime` + the component model. Plugins declare capabilities in
//! `plugin.toml`; the host grants them explicitly — nothing is ambient. See
//! [`capability`] for the enumerated set.

#![deny(rust_2018_idioms, unreachable_pub)]

pub mod capability;
pub mod error;
pub mod hook;
pub mod host;
pub mod manifest;
pub mod registry;
pub mod runtime;
pub mod wasm_hook;
pub mod webhook;

pub use capability::Capability;
pub use error::{PluginError, PluginResult};
pub use hook::{HookEvent, HookHandler, HookRegistry, LoggingHook};
pub use host::{HostContext, Services};
pub use manifest::PluginManifest;
pub use registry::{PluginGrant, PluginInfo, PluginRegistry};
pub use runtime::{PluginHandle, PluginRuntime, RuntimeConfig};
pub use wasm_hook::WasmPluginHook;
pub use webhook::{WebhookConfig, WebhookHook};
