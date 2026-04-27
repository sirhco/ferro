use std::path::PathBuf;

use anyhow::Result;
use base64::Engine;
use clap::Args as ClapArgs;
use ferro_core::{Content, ContentType, Media, Role, Site, User};
use futures::StreamExt;
use serde::{Deserialize, Serialize};

use crate::config::FerroConfig;

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Output bundle path.
    #[arg(long, short, default_value = "ferro.bundle.json")]
    pub out: PathBuf,

    /// Include media file contents (base64-encoded) alongside metadata.
    #[arg(long)]
    pub include_media: bool,
}

/// Bundle format. Bump `version` on breaking changes.
///
/// - `version = 1` — sites/types/content/users/roles/media metadata
/// - `version = 2` — optional `media_blobs` (base64) for portable full dumps
#[derive(Debug, Serialize, Deserialize)]
pub struct Bundle {
    pub version: u32,
    #[serde(default)]
    pub sites: Vec<Site>,
    #[serde(default)]
    pub types: Vec<ContentType>,
    #[serde(default)]
    pub content: Vec<Content>,
    #[serde(default)]
    pub users: Vec<User>,
    #[serde(default)]
    pub roles: Vec<Role>,
    #[serde(default)]
    pub media: Vec<Media>,
    /// Keyed by media key, base64 file contents. Only populated when the
    /// export is run with `--include-media`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media_blobs: Vec<MediaBlob>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MediaBlob {
    pub key: String,
    /// Base64-encoded file contents (standard alphabet, no padding stripped).
    pub data_base64: String,
}

pub async fn run(args: Args, config_path: PathBuf) -> Result<()> {
    let cfg = FerroConfig::load(&config_path).await?;
    let repo = ferro_storage::connect(&cfg.storage).await?;
    let media_store =
        if args.include_media { Some(ferro_media::connect(&cfg.media).await?) } else { None };

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
    let roles = repo.users().list_roles().await?;

    let mut media_blobs = Vec::new();
    if let Some(store) = media_store.as_ref() {
        for m in &media {
            let mut stream = store.get(&m.key).await?;
            let mut buf = Vec::with_capacity(m.size as usize);
            while let Some(chunk) = stream.next().await {
                buf.extend_from_slice(&chunk?);
            }
            media_blobs.push(MediaBlob {
                key: m.key.clone(),
                data_base64: base64::engine::general_purpose::STANDARD.encode(&buf),
            });
        }
    }

    let version = if media_blobs.is_empty() { 1 } else { 2 };
    let bundle = Bundle { version, sites, types, content, users, roles, media, media_blobs };
    let json = serde_json::to_vec_pretty(&bundle)?;
    tokio::fs::write(&args.out, json).await?;
    println!(
        "✓ wrote {} (v{version}, {} content, {} media{})",
        args.out.display(),
        bundle.content.len(),
        bundle.media.len(),
        if bundle.media_blobs.is_empty() {
            String::new()
        } else {
            format!(", {} blobs", bundle.media_blobs.len())
        }
    );
    Ok(())
}
