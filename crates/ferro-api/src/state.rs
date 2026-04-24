use std::sync::Arc;

use ferro_auth::{AuthService, JwtManager};
use ferro_media::MediaStore;
use ferro_storage::Repository;

pub struct AppState {
    pub repo: Arc<dyn Repository>,
    pub media: Arc<dyn MediaStore>,
    pub auth: Arc<AuthService>,
    pub jwt: Arc<JwtManager>,
}

impl AppState {
    pub fn new(
        repo: Arc<dyn Repository>,
        media: Arc<dyn MediaStore>,
        auth: Arc<AuthService>,
        jwt: Arc<JwtManager>,
    ) -> Self {
        Self { repo, media, auth, jwt }
    }
}
