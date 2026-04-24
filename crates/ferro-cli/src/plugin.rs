use std::path::PathBuf;

use anyhow::Result;
use clap::Subcommand;
use ferro_plugin::{PluginRegistry, PluginRuntime, RuntimeConfig};

use crate::config::FerroConfig;

#[derive(Debug, Subcommand)]
pub enum Cmd {
    /// List installed plugins.
    List,
    /// Show a plugin's manifest and capabilities.
    Inspect { name: String },
    /// Rescan the plugin directory and reload.
    Reload,
}

pub async fn run(cmd: Cmd, config_path: PathBuf) -> Result<()> {
    let cfg = FerroConfig::load(&config_path).await?;
    let rt = PluginRuntime::new(
        RuntimeConfig {
            max_memory_bytes: cfg.plugins.max_memory_mb * 1024 * 1024,
            fuel_per_request: cfg.plugins.fuel_per_request,
            ..Default::default()
        },
        Default::default(),
    )?;
    let registry = PluginRegistry::new(rt, cfg.plugins.dir);
    registry.scan().await?;

    match cmd {
        Cmd::List | Cmd::Reload => {
            for name in registry.list().await {
                println!("- {name}");
            }
        }
        Cmd::Inspect { name } => {
            let handle = registry.get(&name).await?;
            let m = handle.manifest();
            println!("{} v{}", m.name, m.version);
            if let Some(d) = &m.description {
                println!("  {d}");
            }
            println!("  capabilities:");
            for c in &m.capabilities {
                println!("    - {}", c.0);
            }
        }
    }
    Ok(())
}
