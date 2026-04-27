//! Build pipeline.
//!
//! 1. Run `cargo leptos build --project ferro-admin --release --split`.
//! 2. Walk the output (`target/site` + `pkg/`), brotli-compress
//!    `*.wasm` / `*.js` / `*.css` / `*.svg` at quality 11, keep originals.
//!    Axum serves the `.br` variants via content-negotiation.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Args as ClapArgs;
use tokio::process::Command;

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Skip `cargo leptos build` and only recompress existing assets.
    #[arg(long)]
    pub skip_leptos: bool,

    /// cargo-leptos project name (matches `[[workspace.metadata.leptos]] name`).
    /// Defaults to `ferro-admin`. Use `starter-site` for the public site.
    #[arg(long, default_value = "ferro-admin")]
    pub project: String,

    /// Brotli quality (0–11).
    #[arg(long, default_value_t = 11)]
    pub quality: u8,

    /// Asset output directory (cargo-leptos default = target/site).
    /// For starter-site, override with `--site-dir target/starter-site`.
    #[arg(long, default_value = "target/site")]
    pub site_dir: PathBuf,
}

pub async fn run(args: Args) -> Result<()> {
    if !args.skip_leptos {
        let status = Command::new("cargo")
            .args(["leptos", "build", "--project", &args.project, "--release", "--split"])
            .status()
            .await
            .context("running cargo leptos; install with `cargo install cargo-leptos`")?;
        anyhow::ensure!(status.success(), "cargo leptos build failed");
    }

    let n = compress_tree(&args.site_dir, args.quality).await?;
    println!("✓ brotli-compressed {n} files under {}", args.site_dir.display());
    Ok(())
}

async fn compress_tree(root: &Path, q: u8) -> Result<usize> {
    if !root.exists() {
        return Ok(0);
    }
    let mut count = 0;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut entries = tokio::fs::read_dir(&dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let p = entry.path();
            if entry.file_type().await?.is_dir() {
                stack.push(p);
                continue;
            }
            if should_compress(&p) {
                compress_file(&p, q).await?;
                count += 1;
            }
        }
    }
    Ok(count)
}

fn should_compress(p: &Path) -> bool {
    let Some(ext) = p.extension().and_then(|s| s.to_str()) else {
        return false;
    };
    matches!(ext, "wasm" | "js" | "css" | "svg" | "html" | "json")
        && !p.with_extension(format!("{ext}.br")).exists()
}

async fn compress_file(p: &Path, q: u8) -> Result<()> {
    use std::io::{BufReader, Cursor};

    let bytes = tokio::fs::read(p).await?;
    let out_path = {
        let mut s = p.as_os_str().to_owned();
        s.push(".br");
        PathBuf::from(s)
    };
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let params =
        brotli::enc::BrotliEncoderParams { quality: q as i32, lgwin: 22, ..Default::default() };
    brotli::BrotliCompress(&mut BufReader::new(Cursor::new(&bytes)), &mut out, &params)
        .context("brotli encode")?;
    tokio::fs::write(out_path, out).await?;
    Ok(())
}
