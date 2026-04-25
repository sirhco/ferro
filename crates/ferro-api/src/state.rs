use std::sync::Arc;

use ferro_auth::{AuthService, JwtManager};
use ferro_media::MediaStore;
use ferro_plugin::HookRegistry;
use ferro_storage::Repository;

pub struct AppState {
    pub repo: Arc<dyn Repository>,
    pub media: Arc<dyn MediaStore>,
    pub auth: Arc<AuthService>,
    pub jwt: Arc<JwtManager>,
    pub hooks: HookRegistry,
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
        }
    }

    /// Construct an `AppState` with a pre-populated hook registry. Used by
    /// the CLI when extra plugins are wired in at startup, and by tests that
    /// need to assert hook fan-out.
    #[must_use]
    pub fn with_hooks(
        repo: Arc<dyn Repository>,
        media: Arc<dyn MediaStore>,
        auth: Arc<AuthService>,
        jwt: Arc<JwtManager>,
        hooks: HookRegistry,
    ) -> Self {
        Self { repo, media, auth, jwt, hooks }
    }
}
