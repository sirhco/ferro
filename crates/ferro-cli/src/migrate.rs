use std::path::PathBuf;

use anyhow::Result;
use clap::Args as ClapArgs;

use crate::config::FerroConfig;

#[derive(Debug, ClapArgs)]
pub struct Args {}

pub async fn run(_args: Args, config_path: PathBuf) -> Result<()> {
    let cfg = FerroConfig::load(&config_path).await?;
    let repo = ferro_storage::connect(&cfg.storage).await?;
    repo.migrate().await?;
    println!("✓ migrations applied to {}", cfg.storage.backend_name());
    Ok(())
}
