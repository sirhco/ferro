//! Ferro CLI entrypoint.
//!
//! Subcommands: init, serve, migrate, export, import, plugin, build.

use anyhow::Result;
use clap::{Parser, Subcommand};
use ferro_cli::{admin, build, export, import, init, migrate, plugin, serve};

/// Ferro — Rust-powered content engine.
#[derive(Debug, Parser)]
#[command(name = "ferro", version, about)]
pub struct Cli {
    /// Path to ferro.toml. Defaults to `./ferro.toml`.
    #[arg(long, global = true, default_value = "ferro.toml")]
    pub config: std::path::PathBuf,

    #[command(subcommand)]
    pub command: Cmd,
}

#[derive(Debug, Subcommand)]
pub enum Cmd {
    /// Scaffold a new Ferro project in the current directory.
    Init(init::Args),
    /// Start the Ferro server (admin + API).
    Serve(serve::Args),
    /// Apply storage migrations.
    Migrate(migrate::Args),
    /// Export a site bundle (content + schema + users + media manifest) as JSON.
    Export(export::Args),
    /// Import a site bundle produced by `ferro export`.
    Import(import::Args),
    /// Build production assets (runs cargo-leptos and brotli-compresses output).
    Build(build::Args),
    /// Plugin management.
    #[command(subcommand)]
    Plugin(plugin::Cmd),
    /// Operator tooling: bootstrap admin users + roles directly against the repo.
    #[command(subcommand)]
    Admin(admin::Cmd),
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,ferro=debug")),
        )
        .init();

    let cli = Cli::parse();
    match cli.command {
        Cmd::Init(a) => init::run(a).await,
        Cmd::Serve(a) => serve::run(a, cli.config).await,
        Cmd::Migrate(a) => migrate::run(a, cli.config).await,
        Cmd::Export(a) => export::run(a, cli.config).await,
        Cmd::Import(a) => import::run(a, cli.config).await,
        Cmd::Build(a) => build::run(a).await,
        Cmd::Plugin(a) => plugin::run(a, cli.config).await,
        Cmd::Admin(a) => admin::run(a, cli.config).await,
    }
}
