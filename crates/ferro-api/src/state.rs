use std::sync::Arc;

use ferro_auth::{AuthService, JwtManager};
use ferro_media::MediaStore;
use ferro_plugin::{HookRegistry, PluginRegistry};
use ferro_storage::Repository;

use crate::rate_limit::{RateLimitConfig, RateLimiter};

/// Operator-tunable flags surfaced through `ferro.toml [auth]`.
#[derive(Debug, Clone, Copy, Default)]
pub struct AuthOptions {
    /// When `true`, `POST /api/v1/auth/signup` is reachable. Off by default
    /// because most Ferro deployments are private CMS instances; flip to
    /// `true` only when you intend to accept public registrations.
    pub allow_public_signup: bool,
}

pub struct AppState {
    pub repo: Arc<dyn Repository>,
    pub media: Arc<dyn MediaStore>,
    pub auth: Arc<AuthService>,
    pub jwt: Arc<JwtManager>,
    pub hooks: HookRegistry,
    pub options: AuthOptions,
    /// Token-bucket rate limiter shared across the auth endpoints. Keyed by
    /// peer IP; tests can opt out by passing `RateLimitConfig` with a huge
    /// burst (`u32::MAX`) so checks always succeed.
    pub auth_rate_limit: Arc<RateLimiter>,
    /// WASM plugin registry. `None` when the binary was started without
    /// plugin support (e.g. older test fixtures); REST plugin endpoints
    /// return `503 Service Unavailable` in that case.
    pub plugins: Option<Arc<PluginRegistry>>,
}

impl AppState {
    pub fn new(
        repo: Arc<dyn Repository>,
        media: Arc<dyn MediaStore>,
        auth: Arc<AuthService>,
        jwt: Arc<JwtManager>,
    ) -> Self {
        Self {
            repo,
            media,
            auth,
            jwt,
            hooks: HookRegistry::new(),
            options: AuthOptions::default(),
            auth_rate_limit: Arc::new(RateLimiter::new(RateLimitConfig::default())),
            plugins: None,
        }
    }

    /// Construct an `AppState` with pre-populated hooks + auth options.
    #[must_use]
    pub fn with_hooks(
        repo: Arc<dyn Repository>,
        media: Arc<dyn MediaStore>,
        auth: Arc<AuthService>,
        jwt: Arc<JwtManager>,
        hooks: HookRegistry,
    ) -> Self {
        Self {
            repo,
            media,
            auth,
            jwt,
            hooks,
            options: AuthOptions::default(),
            auth_rate_limit: Arc::new(RateLimiter::new(RateLimitConfig::default())),
            plugins: None,
        }
    }

    /// Builder-style override for the WASM plugin registry.
    #[must_use]
    pub fn with_plugins(mut self, plugins: Arc<PluginRegistry>) -> Self {
        self.plugins = Some(plugins);
        self
    }

    /// Builder-style override for the auth rate limiter. Tests use this to
    /// crank the burst high enough that checks never trip.
    #[must_use]
    pub fn with_rate_limit(mut self, cfg: RateLimitConfig) -> Self {
        self.auth_rate_limit = Arc::new(RateLimiter::new(cfg));
        self
    }

    /// Builder-style override for `AuthOptions`.
    #[must_use]
    pub fn with_options(mut self, options: AuthOptions) -> Self {
        self.options = options;
        self
    }
}
