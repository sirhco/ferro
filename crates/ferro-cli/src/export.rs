use std::path::PathBuf;

use anyhow::Result;
use clap::Args as ClapArgs;
use ferro_core::{Content, ContentType, Media, Site, User};
use serde::Serialize;

use crate::config::FerroConfig;

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Output bundle path.
    #[arg(long, short, default_value = "ferro.bundle.json")]
    pub out: PathBuf,

    /// Include media file contents (otherwise only metadata is exported).
    #[arg(long)]
    pub include_media: bool,
}

#[derive(Serialize)]
struct Bundle {
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

    let sites = repo.sites().list().await?;
    let mut types = Vec::new();
    let mut content = Vec::new();
    let mut media = Vec::new();
    for s in &sites {
        types.extend(repo.types().list(s.id).await?);
        media.extend(repo.media().list(s.id).await?);
        let page = repo
            .content()
            .list(ferro_core::ContentQuery {
                site_id: Some(s.id),
                per_page: Some(u32::MAX),
                ..Default::default()
            })
            .await?;
        content.extend(page.items);
    }
    let users = repo.users().list().await?;

    let bundle = Bundle { version: 1, sites, types, content, users, media };
    let json = serde_json::to_vec_pretty(&bundle)?;
    tokio::fs::write(&args.out, json).await?;
    println!("✓ wrote {}", args.out.display());

    if args.include_media {
        eprintln!("note: --include-media not yet wired to the media backend");
    }
    Ok(())
}
