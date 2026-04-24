//! Ferro CLI internals exposed as a library so integration tests can drive
//! commands end-to-end. The `ferro` binary is `src/main.rs`.

pub mod build;
pub mod config;
pub mod export;
pub mod import;
pub mod init;
pub mod migrate;
pub mod plugin;
pub mod serve;
