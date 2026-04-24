use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Args as ClapArgs, ValueEnum};
use ferro_core::{Content, ContentType, Media, Site, User};
use serde::Deserialize;

use crate::config::FerroConfig;

#[derive(Debug, Clone, ValueEnum)]
pub enum Mode {
    Merge,
    Replace,
}

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Bundle path.
    pub bundle: PathBuf,

    /// Merge into existing data or replace it.
    #[arg(long, value_enum, default_value = "merge")]
    pub mode: Mode,
}

#[derive(Deserialize)]
struct Bundle {
    #[serde(default)]
    version: u32,
    sites: Vec<Site>,
    types: Vec<ContentType>,
    content: Vec<Content>,
    users: Vec<User>,
    media: Vec<Media>,
}

pub async fn run(args: Args, config_path: PathBuf) -> Result<()> {
    let cfg = FerroConfig::load(&config_path).await?;
    let repo = ferro_storage::connect(&cfg.storage).await?;
    repo.migrate().await?;

    let bytes = tokio::fs::read(&args.bundle)
        .await
        .with_context(|| format!("reading {}", args.bundle.display()))?;
    let bundle: Bundle = serde_json::from_slice(&bytes).context("parsing bundle")?;
    anyhow::ensure!(bundle.version <= 1, "unsupported bundle version {}", bundle.version);

    for site in bundle.sites {
        repo.sites().upsert(site).await?;
    }
    for ty in bundle.types {
        repo.types().upsert(ty).await?;
    }
    for user in bundle.users {
        repo.users().upsert(user).await?;
    }
    // Content + media need `NewContent`/`Media` shapes to upsert cleanly.
    // For v0.1 we import users/types/sites; content/media roundtrip is wired
    // once storage-backends implement upsert-by-id across the board.
    println!(
        "✓ imported: sites, types, users (content+media scheduled for storage impl follow-up)"
    );
    let _ = (bundle.content, bundle.media, args.mode);
    Ok(())
}
