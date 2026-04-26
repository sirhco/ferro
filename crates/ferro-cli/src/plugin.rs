use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use clap::Subcommand;
use ferro_plugin::{HookRegistry, PluginRegistry, PluginRuntime, RuntimeConfig, Services};

use crate::config::FerroConfig;

#[derive(Debug, Subcommand)]
pub enum Cmd {
    /// List installed plugins.
    List,
    /// Show a plugin's manifest, declared/granted capabilities, and hooks.
    Inspect { name: String },
    /// Rescan the plugin directory and reload.
    Reload,
}

pub async fn run(cmd: Cmd, config_path: PathBuf) -> Result<()> {
    let cfg = FerroConfig::load(&config_path).await?;
    let repo: Arc<dyn ferro_storage::Repository> =
        Arc::from(ferro_storage::connect(&cfg.storage).await?);
    let services = Arc::new(Services::new(repo, HookRegistry::new()));
    let runtime = PluginRuntime::new(
        RuntimeConfig {
            max_memory_bytes: cfg.plugins.max_memory_mb * 1024 * 1024,
            fuel_per_request: cfg.plugins.fuel_per_request,
            ..Default::default()
        },
        services,
    )?;
    let grants: Vec<_> = cfg.plugins.grants.iter().map(|g| g.to_grant()).collect();
    let registry = PluginRegistry::new(runtime, cfg.plugins.dir, HookRegistry::new(), &grants);
    registry.scan().await?;

    match cmd {
        Cmd::List | Cmd::Reload => {
            if matches!(cmd, Cmd::Reload) {
                registry.reload().await?;
            }
            for info in registry.describe_all().await {
                println!(
                    "- {} v{}{}",
                    info.name,
                    info.version,
                    if info.enabled { "" } else { " (disabled)" }
                );
            }
        }
        Cmd::Inspect { name } => {
            let info = registry.describe(&name).await?;
            println!("{} v{}", info.name, info.version);
            if let Some(d) = &info.description {
                println!("  {d}");
            }
            println!("  hooks: {}", join_or_dash(&info.hooks));
            println!("  declared capabilities: {}", join_or_dash(&info.declared));
            println!("  granted capabilities:  {}", join_or_dash(&info.granted));
        }
    }
    Ok(())
}

fn join_or_dash(items: &[String]) -> String {
    if items.is_empty() {
        "—".into()
    } else {
        items.join(", ")
    }
}
