use std::net::SocketAddr;
use std::path::PathBuf;
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

    /// Directory containing the cargo-leptos build output (defaults to
    /// `target/site`). Used to serve `/pkg/*` for the admin SPA.
    #[arg(long, default_value = "target/site")]
    pub site_dir: PathBuf,
}

pub async fn run(args: Args, config_path: PathBuf) -> Result<()> {
    let cfg = FerroConfig::load(&config_path).await?;
    let bind = args.bind.unwrap_or(cfg.server.bind.clone());

    let repo: Arc<dyn ferro_storage::Repository> =
        Arc::from(ferro_storage::connect(&cfg.storage).await?);
    repo.migrate().await?;
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

    // --- Compose: API + admin SSR + /pkg/ static ---------------------------
    let api_app = ferro_api::router(state);
    let admin_app = build_admin_router(&bind, &args.site_dir);
    let pkg_dir = args.site_dir.join("pkg");
    let pkg_service = ServeDir::new(&pkg_dir).precompressed_br();
    let app: Router = Router::new()
        .nest_service("/pkg", pkg_service)
        .merge(admin_app)
        .merge(api_app);

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!(
        "ferro listening on http://{bind} (admin SPA hydrates from {})",
        pkg_dir.display()
    );
    axum::serve(listener, app).await?;
    Ok(())
}

/// Build the Leptos SSR sub-router for the admin app. The admin's
/// `package.metadata.leptos` dictates `output-name` and `site-pkg-dir`; we
/// mirror those values here so the hydrate scripts emitted in the shell
/// resolve to the same `/pkg/ferro_admin.{js,wasm,css}` paths the static
/// service hands out.
fn build_admin_router(bind: &str, site_dir: &PathBuf) -> Router {
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
