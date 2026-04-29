use std::{path::PathBuf, process::Command, sync::Arc};

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
    /// Scaffold a new plugin crate (Cargo.toml + plugin.toml + src/lib.rs +
    /// vendored WIT). The result builds with
    /// `cargo build --release --target wasm32-wasip2`.
    New {
        /// Plugin name. Used for the manifest, the cargo crate (`plugin-<name>`),
        /// and the directory if `--path` is omitted.
        name: String,
        /// Where to create the plugin crate. Defaults to `./<name>` under the
        /// current directory.
        #[arg(long)]
        path: Option<PathBuf>,
        /// Capabilities to declare in `plugin.toml` (comma-separated, e.g.
        /// `logs,content.read`). Operators must still grant these via
        /// `[[plugins.grants]]` in `ferro.toml`.
        #[arg(long, value_delimiter = ',')]
        capabilities: Vec<String>,
        /// Hook events to subscribe to (comma-separated, e.g.
        /// `content.created,content.published`).
        #[arg(long, value_delimiter = ',')]
        hooks: Vec<String>,
    },
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

/// Vendored copy of the host WIT, embedded at CLI compile time. The scaffolder
/// writes this verbatim into the new plugin's `wit/` directory so the plugin
/// builds without any path-relative reference to the Ferro source tree.
const VENDORED_WIT: &str = include_str!("../../ferro-plugin/wit/ferro.wit");

pub async fn run(cmd: Cmd, config_path: PathBuf) -> Result<()> {
    // `new` does not touch storage or runtime — handle before loading the config.
    if let Cmd::New { name, path, capabilities, hooks } = &cmd {
        return scaffold_new(name, path.as_deref(), capabilities, hooks).await;
    }

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
        Cmd::Install { .. } | Cmd::New { .. } => unreachable!(),
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

    let wasm_src =
        crate_path.join("target/wasm32-wasip2/release").join(format!("{wasm_basename}.wasm"));
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

/// Validate a plugin name. Allowed: ASCII alphanumerics, `-`, `_`. Must not
/// start with a digit or `-`. Keeps the cargo crate name (`plugin-<name>`)
/// + the manifest `name` field + the install directory all consistent.
fn validate_plugin_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(anyhow!("plugin name must not be empty"));
    }
    if name.starts_with(|c: char| c.is_ascii_digit() || c == '-') {
        return Err(anyhow!("plugin name `{name}` must not start with a digit or `-`"));
    }
    for c in name.chars() {
        if !(c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Err(anyhow!(
                "plugin name `{name}` contains invalid character `{c}`; allowed: A-Z a-z 0-9 - _"
            ));
        }
    }
    Ok(())
}

async fn scaffold_new(
    name: &str,
    path: Option<&std::path::Path>,
    capabilities: &[String],
    hooks: &[String],
) -> Result<()> {
    validate_plugin_name(name)?;

    let dest = match path {
        Some(p) => p.to_path_buf(),
        None => std::env::current_dir()?.join(name),
    };
    if dest.exists() {
        return Err(anyhow!(
            "destination already exists: {} (refusing to overwrite)",
            dest.display()
        ));
    }

    let crate_name = format!("plugin-{name}");
    let cap_array = toml_string_array(capabilities);
    let hook_array = toml_string_array(hooks);

    let cargo_toml = format!(
        r#"[package]
name = "{crate_name}"
version = "0.1.0"
edition = "2021"
publish = false
description = "Ferro WASM plugin: {name}"

[lib]
crate-type = ["cdylib"]

[dependencies]
wit-bindgen = "0.35"

# Standalone — not a workspace member. Build with:
#   cargo build --release --target wasm32-wasip2
[workspace]
"#
    );

    let plugin_toml = format!(
        r#"name = "{name}"
version = "0.1.0"
description = "Ferro WASM plugin: {name}"
entry = "plugin.wasm"
capabilities = {cap_array}
hooks = {hook_array}
"#
    );

    let lib_rs = scaffold_lib_rs(name, hooks);

    let readme = format!(
        r#"# plugin-{name}

Ferro WASM plugin scaffolded by `ferro plugin new`.

## Build

```sh
cargo build --release --target wasm32-wasip2
```

The resulting `target/wasm32-wasip2/release/plugin_{name_underscore}.wasm` is
the plugin component.

## Install

```sh
ferro --config /path/to/ferro.toml plugin install .
```

This builds + copies `plugin.wasm` + `plugin.toml` into the configured
`[plugins].dir`. Restart `ferro serve` (or `POST /api/v1/plugins/reload`) to
pick up the change.

## Grants

`plugin.toml` declares which host capabilities this plugin needs. The
operator still has to grant them in their `ferro.toml`:

```toml
[[plugins.grants]]
name = "{name}"
capabilities = {cap_array}
```
"#,
        name = name,
        name_underscore = name.replace('-', "_"),
        cap_array = cap_array,
    );

    tokio::fs::create_dir_all(dest.join("src")).await?;
    tokio::fs::create_dir_all(dest.join("wit")).await?;
    tokio::fs::write(dest.join("Cargo.toml"), cargo_toml).await?;
    tokio::fs::write(dest.join("plugin.toml"), plugin_toml).await?;
    tokio::fs::write(dest.join("src/lib.rs"), lib_rs).await?;
    tokio::fs::write(dest.join("wit/ferro.wit"), VENDORED_WIT).await?;
    tokio::fs::write(dest.join("README.md"), readme).await?;

    println!("Created plugin scaffold at {}", dest.display());
    println!("Next:");
    println!("  cd {}", dest.display());
    println!("  cargo build --release --target wasm32-wasip2");
    println!("  ferro plugin install .");
    Ok(())
}

fn toml_string_array(items: &[String]) -> String {
    if items.is_empty() {
        return "[]".into();
    }
    let inner = items.iter().map(|s| format!("\"{s}\"")).collect::<Vec<_>>().join(", ");
    format!("[{inner}]")
}

fn scaffold_lib_rs(name: &str, hooks: &[String]) -> String {
    let init_msg = format!("plugin-{name} loaded");
    let on_event = if hooks.iter().any(|h| h == "content.published") {
        r#"        if let HookEvent::ContentPublished(c) = evt {
            log(LogLevel::Info, NAME, &format!("published {}", c.slug));
        }
"#
    } else if hooks.iter().any(|h| h == "content.created") {
        r#"        if let HookEvent::ContentCreated(c) = evt {
            log(LogLevel::Info, NAME, &format!("created {}", c.slug));
        }
"#
    } else {
        r#"        // Hook events arrive here. Match on the variant you care about.
        let _ = evt;
"#
    };

    format!(
        r#"//! Ferro WASM plugin: {name}.
//!
//! Generated by `ferro plugin new`. Edit `on_event` to handle hook events,
//! and declare any new capabilities in both `plugin.toml` (this crate) and
//! `[[plugins.grants]]` (the operator's `ferro.toml`).

wit_bindgen::generate!({{
    world: "plugin",
    path: "wit",
}});

use exports::ferro::cms::guest::Guest;
use ferro::cms::host::{{log, LogLevel}};
use ferro::cms::types::HookEvent;

const NAME: &str = "plugin-{name}";

struct Component;

impl Guest for Component {{
    fn init() -> Result<(), String> {{
        log(LogLevel::Info, NAME, "{init_msg}");
        Ok(())
    }}

    fn on_event(evt: HookEvent) -> Result<(), String> {{
{on_event}        Ok(())
    }}
}}

export!(Component);
"#
    )
}
