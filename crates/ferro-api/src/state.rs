use std::sync::Arc;

use ferro_auth::{AuthService, JwtManager};
use ferro_media::MediaStore;
use ferro_plugin::HookRegistry;
use ferro_storage::Repository;

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
        }
    }

    /// Builder-style override for `AuthOptions`.
    #[must_use]
    pub fn with_options(mut self, options: AuthOptions) -> Self {
        self.options = options;
        self
    }
}
