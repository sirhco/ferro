use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
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
    /// Build a plugin crate (cargo build --target wasm32-wasip2 --release)
    /// and copy `plugin.wasm` + `plugin.toml` into the configured plugins
    /// directory under `<install_name>/`.
    Install {
        /// Path to the plugin crate (directory containing `Cargo.toml` + `plugin.toml`).
        crate_path: PathBuf,
        /// Override the install directory name. Defaults to the plugin's `name` field.
        #[arg(long)]
        as_name: Option<String>,
        /// Skip the `cargo build` step (useful when the wasm is already built).
        #[arg(long)]
        no_build: bool,
    },
}

pub async fn run(cmd: Cmd, config_path: PathBuf) -> Result<()> {
    let cfg = FerroConfig::load(&config_path).await?;
    if let Cmd::Install { crate_path, as_name, no_build } = &cmd {
        return install(crate_path, as_name.as_deref(), *no_build, &cfg.plugins.dir).await;
    }

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
        Cmd::Install { .. } => unreachable!(),
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

async fn install(
    crate_path: &PathBuf,
    as_name: Option<&str>,
    no_build: bool,
    plugins_dir: &PathBuf,
) -> Result<()> {
    let crate_path = crate_path
        .canonicalize()
        .with_context(|| format!("crate path not found: {}", crate_path.display()))?;
    let cargo_toml = crate_path.join("Cargo.toml");
    let plugin_toml = crate_path.join("plugin.toml");
    if !cargo_toml.exists() {
        return Err(anyhow!("missing Cargo.toml at {}", cargo_toml.display()));
    }
    if !plugin_toml.exists() {
        return Err(anyhow!(
            "missing plugin.toml at {}; required to know install name + entry",
            plugin_toml.display()
        ));
    }

    let manifest_str = tokio::fs::read_to_string(&plugin_toml).await?;
    let manifest: PluginManifestSlim = toml::from_str(&manifest_str)
        .with_context(|| format!("parsing {}", plugin_toml.display()))?;
    let install_name = as_name.unwrap_or(&manifest.name).to_string();

    let cargo_manifest_str = tokio::fs::read_to_string(&cargo_toml).await?;
    let cargo_manifest: CargoSlim = toml::from_str(&cargo_manifest_str)
        .with_context(|| format!("parsing {}", cargo_toml.display()))?;
    let pkg_name = cargo_manifest.package.name;
    let wasm_basename = pkg_name.replace('-', "_");

    if !no_build {
        println!("Building {pkg_name} for wasm32-wasip2...");
        let status = Command::new("cargo")
            .args(["build", "--release", "--target", "wasm32-wasip2", "--manifest-path"])
            .arg(&cargo_toml)
            .status()
            .with_context(|| "spawning cargo build")?;
        if !status.success() {
            return Err(anyhow!("cargo build failed (exit {status})"));
        }
    }

    let wasm_src = crate_path
        .join("target/wasm32-wasip2/release")
        .join(format!("{wasm_basename}.wasm"));
    if !wasm_src.exists() {
        return Err(anyhow!(
            "expected wasm artifact at {} (run without --no-build, or check Cargo.toml [package].name)",
            wasm_src.display()
        ));
    }

    let dest_dir = plugins_dir.join(&install_name);
    tokio::fs::create_dir_all(&dest_dir).await?;
    tokio::fs::copy(&wasm_src, dest_dir.join("plugin.wasm")).await?;
    tokio::fs::copy(&plugin_toml, dest_dir.join("plugin.toml")).await?;

    println!("Installed {install_name} → {}", dest_dir.display());
    println!("Restart 'ferro serve' or POST /api/v1/plugins/reload to pick up.");
    Ok(())
}

#[derive(serde::Deserialize)]
struct PluginManifestSlim {
    name: String,
}

#[derive(serde::Deserialize)]
struct CargoSlim {
    package: CargoPkg,
}

#[derive(serde::Deserialize)]
struct CargoPkg {
    name: String,
}

fn join_or_dash(items: &[String]) -> String {
    if items.is_empty() {
        "—".into()
    } else {
        items.join(", ")
    }
}
