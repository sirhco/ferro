use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use axum::Router;
use clap::Args as ClapArgs;
use ferro_api::{AppState, AuthOptions};
use ferro_auth::{AuthService, JwtManager, MemorySessionStore};
use ferro_plugin::{HookRegistry, LoggingHook, WebhookHook};
use leptos::prelude::LeptosOptions;
use leptos_axum::{generate_route_list, LeptosRoutes};
use tower_http::services::ServeDir;

use crate::config::FerroConfig;

#[derive(Debug, ClapArgs)]
pub struct Args {
    #[arg(long)]
    pub bind: Option<String>,

    /// Directory containing the cargo-leptos build output. When omitted,
    /// resolves to `./target/site` relative to the cwd, then falls back to
    /// `<workspace>/target/site` (baked in at compile time) so a developer
    /// running `ferro serve` from any directory still picks up the SPA.
    /// Override with `FERRO_SITE_DIR` or this flag.
    #[arg(long)]
    pub site_dir: Option<PathBuf>,
}

pub async fn run(args: Args, config_path: PathBuf) -> Result<()> {
    let cfg = FerroConfig::load(&config_path).await?;
    let bind = args.bind.unwrap_or(cfg.server.bind.clone());

    let repo: Arc<dyn ferro_storage::Repository> =
        Arc::from(ferro_storage::connect(&cfg.storage).await?);
    repo.migrate().await?;
    ensure_default_site(&*repo).await?;
    let media: Arc<dyn ferro_media::MediaStore> =
        Arc::from(ferro_media::connect(&cfg.media).await?);
    let sessions = Arc::new(MemorySessionStore::new());
    let auth = Arc::new(AuthService::new(repo.clone(), sessions));

    let jwt_secret = cfg.auth.resolve_jwt_secret();
    let jwt = Arc::new(JwtManager::hs256(cfg.auth.jwt_issuer.clone(), jwt_secret.as_bytes()));

    let hooks = HookRegistry::new();
    hooks.register(Arc::new(LoggingHook)).await;
    for webhook_cfg in &cfg.webhooks {
        match WebhookHook::new(webhook_cfg.clone()) {
            Ok(h) => {
                hooks.register(Arc::new(h)).await;
                tracing::info!(target: "ferro::webhook", url = %webhook_cfg.url, "registered");
            }
            Err(e) => tracing::warn!(target: "ferro::webhook", url = %webhook_cfg.url, error = %e, "failed to register"),
        }
    }

    let options = AuthOptions {
        allow_public_signup: cfg.auth.allow_public_signup,
    };
    let state = Arc::new(
        AppState::with_hooks(repo, media, auth, jwt, hooks).with_options(options),
    );

    let site_dir = resolve_site_dir(args.site_dir.as_deref());
    let pkg_dir = site_dir.join("pkg");
    if !pkg_dir.exists() {
        tracing::warn!(
            "admin SPA assets not found at {}; run `cargo leptos build --project ferro-admin` (or pass --site-dir)",
            pkg_dir.display()
        );
    } else {
        // cargo-leptos 0.3 strips the `_bg` suffix wasm-bindgen emits, but
        // leptos 0.8's `HydrationScripts` still requests `<name>_bg.wasm`.
        // Mirror the file under both names so ServeDir handles either.
        ensure_bg_wasm_alias(&pkg_dir, "ferro_admin").await;
    }

    // --- Compose: API + admin SSR + /pkg/ static ---------------------------
    let api_app = ferro_api::router(state);
    let admin_app = build_admin_router(&bind, &site_dir);
    let pkg_service = ServeDir::new(&pkg_dir).precompressed_br();
    let favicon_path = site_dir.join("favicon.svg");
    // Order matters: API routes are exact (`/api/v1/...`, `/healthz`, etc.) so
    // they must match BEFORE the admin SPA fallback, which catches any
    // remaining path so client-side routing works for deep links like
    // `/admin/content/post/edit/foo`.
    let app: Router = Router::new()
        .nest_service("/pkg", pkg_service)
        .route(
            "/favicon.svg",
            axum::routing::get(move || serve_file(favicon_path.clone(), "image/svg+xml")),
        )
        .merge(api_app)
        .merge(admin_app);

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!(
        "ferro listening on http://{bind} (admin SPA hydrates from {})",
        pkg_dir.display()
    );
    axum::serve(listener, app).await?;
    Ok(())
}

/// Pick the first existing site dir from: explicit flag → `FERRO_SITE_DIR` env
/// → `./target/site` → `<compile-time workspace>/target/site`. Returns the
/// first candidate that exists; if none do, returns the cwd-relative default
/// so the warning log shows where we looked.
fn resolve_site_dir(explicit: Option<&Path>) -> PathBuf {
    if let Some(p) = explicit {
        return p.to_path_buf();
    }
    if let Ok(env) = std::env::var("FERRO_SITE_DIR") {
        let p = PathBuf::from(env);
        if p.exists() {
            return p;
        }
    }
    let cwd = PathBuf::from("target/site");
    if cwd.exists() {
        return cwd;
    }
    // CARGO_MANIFEST_DIR points at crates/ferro-cli at compile time.
    // Workspace root is one parent up.
    let baked = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|w| w.join("target").join("site"));
    if let Some(p) = baked {
        if p.exists() {
            return p;
        }
    }
    cwd
}

/// Seed a single-tenant `default` site if the repo has none. Most admin
/// flows (media upload, content list, schema designer) resolve "the site"
/// from the first row in `sites`; without one they 404. Idempotent.
async fn ensure_default_site(repo: &dyn ferro_storage::Repository) -> Result<()> {
    use ferro_core::{Locale, Site, SiteId, SiteSettings};
    use time::OffsetDateTime;

    if !repo.sites().list().await?.is_empty() {
        return Ok(());
    }
    let now = OffsetDateTime::now_utc();
    let site = Site {
        id: SiteId::new(),
        slug: "default".into(),
        name: "Default".into(),
        description: None,
        primary_url: None,
        locales: vec![Locale::default()],
        default_locale: Locale::default(),
        settings: SiteSettings::default(),
        created_at: now,
        updated_at: now,
    };
    repo.sites().upsert(site).await?;
    tracing::info!("seeded default site");
    Ok(())
}

/// Read a file off disk and emit it with `content-type`. Returns 404 if the
/// file is missing — used for one-off static routes (e.g. favicon) where
/// `ServeDir` would be overkill.
async fn serve_file(path: PathBuf, content_type: &'static str) -> axum::response::Response {
    use axum::body::Body;
    use axum::http::{header, StatusCode};
    use axum::response::Response;

    match tokio::fs::read(&path).await {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CACHE_CONTROL, "public, max-age=86400")
            .body(Body::from(bytes))
            .unwrap_or_else(|_| {
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::empty())
                    .unwrap()
            }),
        Err(_) => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap(),
    }
}

/// Symlink (or copy on platforms that need it) `<name>.wasm` → `<name>_bg.wasm`
/// inside `pkg_dir` so requests with either filename resolve. Re-runs every
/// boot to keep the alias fresh after rebuilds; falls back to a hard copy if
/// the symlink call fails (e.g. read-only mounts, Windows without privileges).
async fn ensure_bg_wasm_alias(pkg_dir: &Path, output_name: &str) {
    let canonical = pkg_dir.join(format!("{output_name}.wasm"));
    let alias = pkg_dir.join(format!("{output_name}_bg.wasm"));
    if !canonical.exists() {
        return;
    }
    // Refresh stale aliases when the canonical file is newer.
    if let (Ok(c_meta), Ok(a_meta)) = (
        tokio::fs::metadata(&canonical).await,
        tokio::fs::metadata(&alias).await,
    ) {
        if let (Ok(c_t), Ok(a_t)) = (c_meta.modified(), a_meta.modified()) {
            if a_t >= c_t {
                return;
            }
        }
        let _ = tokio::fs::remove_file(&alias).await;
    }
    if let Err(e) = tokio::fs::copy(&canonical, &alias).await {
        tracing::warn!("could not write wasm alias at {}: {e}", alias.display());
    }
}

/// Build the Leptos SSR sub-router for the admin app. The admin's
/// `package.metadata.leptos` dictates `output-name` and `site-pkg-dir`; we
/// mirror those values here so the hydrate scripts emitted in the shell
/// resolve to the same `/pkg/ferro_admin.{js,wasm,css}` paths the static
/// service hands out.
fn build_admin_router(bind: &str, site_dir: &Path) -> Router {
    let leptos_options = LeptosOptions::builder()
        .output_name("ferro_admin")
        .site_root(site_dir.to_string_lossy().to_string())
        .site_pkg_dir("pkg")
        .site_addr(
            bind.parse::<SocketAddr>()
                .unwrap_or_else(|_| "127.0.0.1:3000".parse().unwrap()),
        )
        .reload_port(3002)
        .env(leptos::config::Env::PROD)
        .build();

    let routes = generate_route_list(ferro_admin::App);
    let opts_for_shell = leptos_options.clone();
    Router::new()
        .leptos_routes(&leptos_options, routes, move || {
            ferro_admin::shell(opts_for_shell.clone())
        })
        .with_state(leptos_options)
}
