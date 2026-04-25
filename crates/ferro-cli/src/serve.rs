use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use clap::Args as ClapArgs;
use ferro_api::AppState;
use ferro_auth::{AuthService, JwtManager, MemorySessionStore};
use ferro_plugin::{HookRegistry, LoggingHook};

use crate::config::FerroConfig;

#[derive(Debug, ClapArgs)]
pub struct Args {
    #[arg(long)]
    pub bind: Option<String>,
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

    let state = Arc::new(AppState::with_hooks(repo, media, auth, jwt, hooks));
    let app = ferro_api::router(state);

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!("ferro listening on http://{bind}");
    axum::serve(listener, app).await?;
    Ok(())
}

// Note: the admin Leptos app is wired in via `leptos_axum::LeptosRoutes` in
// a follow-up once `ferro-admin` compiles in SSR mode. We keep the API-only
// variant here so `ferro serve` works against all backends out of the box.

mod _leptos_admin_notes {
    //! Hooking up admin UI:
    //!
    //! ```ignore
    //! use leptos::config::get_configuration;
    //! use leptos_axum::{generate_route_list, LeptosRoutes};
    //! use ferro_admin::{shell, App};
    //!
    //! let leptos_options = get_configuration(None)?.leptos_options;
    //! let routes = generate_route_list(App);
    //! let admin = Router::new().leptos_routes(&leptos_options, routes, {
    //!     let o = leptos_options.clone();
    //!     move || shell(o.clone())
    //! });
    //! app = app.merge(admin).with_state(leptos_options);
    //! ```
}
