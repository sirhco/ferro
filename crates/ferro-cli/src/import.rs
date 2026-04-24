use std::path::PathBuf;

use anyhow::{Context, Result};
use base64::Engine;
use clap::{Args as ClapArgs, ValueEnum};
use ferro_core::{Content, ContentType, Media, Role, Site, User};
use ferro_media::store::MediaStore;
use futures::stream;
use serde::Deserialize;

use crate::config::FerroConfig;

#[derive(Debug, Clone, ValueEnum)]
pub enum Mode {
    /// Upsert bundle entities on top of existing data (default).
    Merge,
    /// Wipe existing sites/types/content/users/roles/media, then insert bundle.
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

#[derive(Debug, Deserialize)]
struct Bundle {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    sites: Vec<Site>,
    #[serde(default)]
    types: Vec<ContentType>,
    #[serde(default)]
    content: Vec<Content>,
    #[serde(default)]
    users: Vec<User>,
    #[serde(default)]
    roles: Vec<Role>,
    #[serde(default)]
    media: Vec<Media>,
    #[serde(default)]
    media_blobs: Vec<MediaBlob>,
}

#[derive(Debug, Deserialize)]
struct MediaBlob {
    key: String,
    data_base64: String,
}

pub async fn run(args: Args, config_path: PathBuf) -> Result<()> {
    let cfg = FerroConfig::load(&config_path).await?;
    let repo = ferro_storage::connect(&cfg.storage).await?;
    repo.migrate().await?;

    let bytes = tokio::fs::read(&args.bundle)
        .await
        .with_context(|| format!("reading {}", args.bundle.display()))?;
    let bundle: Bundle = serde_json::from_slice(&bytes).context("parsing bundle")?;
    anyhow::ensure!(bundle.version <= 2, "unsupported bundle version {}", bundle.version);

    if matches!(args.mode, Mode::Replace) {
        wipe(&*repo).await?;
    }

    // FK order: sites → content_types → content; roles → users; media last.
    for site in bundle.sites {
        repo.sites().upsert(site).await?;
    }
    for ty in bundle.types {
        repo.types().upsert(ty).await?;
    }
    for role in bundle.roles {
        repo.users().upsert_role(role).await?;
    }
    for user in bundle.users {
        repo.users().upsert(user).await?;
    }
    for c in bundle.content {
        repo.content().upsert(c).await?;
    }
    for m in bundle.media {
        repo.media().upsert(m).await?;
    }

    if !bundle.media_blobs.is_empty() {
        let media_store = ferro_media::connect(&cfg.media).await?;
        let mut restored = 0usize;
        for blob in bundle.media_blobs {
            write_blob(&*media_store, &blob).await?;
            restored += 1;
        }
        println!("✓ restored {restored} media blob(s)");
    }

    println!("✓ import complete (v{})", bundle.version);
    Ok(())
}

async fn write_blob(store: &dyn MediaStore, blob: &MediaBlob) -> Result<()> {
    let data = base64::engine::general_purpose::STANDARD
        .decode(&blob.data_base64)
        .with_context(|| format!("decode blob {}", blob.key))?;
    let size = data.len() as u64;
    let mime = mime_guess::from_path(&blob.key)
        .first_or_octet_stream()
        .to_string();
    let body = Box::pin(stream::once(async move { Ok::<_, std::io::Error>(bytes::Bytes::from(data)) }));
    store.put(&blob.key, body, &mime, size).await?;
    Ok(())
}

async fn wipe(repo: &dyn ferro_storage::Repository) -> Result<()> {
    // Delete content first (depends on types + sites).
    for c in repo
        .content()
        .list(ferro_core::ContentQuery { per_page: Some(u32::MAX), ..Default::default() })
        .await?
        .items
    {
        repo.content().delete(c.id).await?;
    }
    let sites = repo.sites().list().await?;
    for s in &sites {
        for m in repo.media().list(s.id).await? {
            repo.media().delete(m.id).await?;
        }
        for t in repo.types().list(s.id).await? {
            repo.types().delete(t.id).await?;
        }
    }
    for s in sites {
        repo.sites().delete(s.id).await?;
    }
    for u in repo.users().list().await? {
        repo.users().delete(u.id).await?;
    }
    Ok(())
}
