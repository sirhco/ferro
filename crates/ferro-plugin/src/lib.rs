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

pub use capability::Capability;
pub use error::{PluginError, PluginResult};
pub use hook::{HookEvent, HookHandler, HookRegistry, LoggingHook};
pub use host::HostContext;
pub use manifest::PluginManifest;
pub use registry::PluginRegistry;
pub use runtime::{PluginHandle, PluginRuntime, RuntimeConfig};
