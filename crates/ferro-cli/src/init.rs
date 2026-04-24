use std::path::PathBuf;

use anyhow::Result;
use clap::Args as ClapArgs;
use console::style;

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Directory to initialize. Defaults to the current directory.
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Pick a storage backend non-interactively.
    #[arg(long, value_parser = ["surreal", "fs-json", "fs-markdown", "postgres"])]
    pub storage: Option<String>,
}

pub async fn run(args: Args) -> Result<()> {
    let dir = args.path;
    tokio::fs::create_dir_all(&dir).await?;
    let cfg_path = dir.join("ferro.toml");
    if cfg_path.exists() {
        anyhow::bail!("{} already exists", cfg_path.display());
    }
    let backend = args
        .storage
        .unwrap_or_else(|| "surreal".into());
    let cfg = starter_config(&backend);
    tokio::fs::write(&cfg_path, cfg).await?;
    tokio::fs::create_dir_all(dir.join("data")).await?;
    tokio::fs::create_dir_all(dir.join("media-store")).await?;
    tokio::fs::create_dir_all(dir.join("plugins")).await?;
    tokio::fs::create_dir_all(dir.join("content")).await?;
    println!("{} wrote {}", style("✓").green(), cfg_path.display());
    println!("Next: {}", style("ferro serve").cyan());
    Ok(())
}

fn starter_config(backend: &str) -> String {
    let storage = match backend {
        "surreal" => r#"[storage]
kind = "surreal-embedded"
path = "./data/ferro.db"
namespace = "ferro"
database = "main"
"#,
        "fs-json" => r#"[storage]
kind = "fs-json"
path = "./data/json"
"#,
        "fs-markdown" => r#"[storage]
kind = "fs-markdown"
path = "./content"
"#,
        "postgres" => r#"[storage]
kind = "postgres"
url = "postgres://ferro:ferro@localhost/ferro"
max_conns = 10
"#,
        _ => unreachable!(),
    };
    format!(
        r#"[server]
bind = "0.0.0.0:8080"
public_url = "http://localhost:8080"
admin_enabled = true

{storage}
[media]
kind = "local"
path = "./media-store"
base_url = "http://localhost:8080/media"

[auth]
session_secret = "CHANGE_ME_{random}"
jwt_issuer = "ferro"
# Override via env: FERRO_JWT_SECRET=...
jwt_secret = "CHANGE_ME_JWT_{random}"
allow_public_signup = false

[plugins]
dir = "./plugins"
max_memory_mb = 128
fuel_per_request = 10_000_000
"#,
        storage = storage,
        random = time::OffsetDateTime::now_utc().unix_timestamp()
    )
}
