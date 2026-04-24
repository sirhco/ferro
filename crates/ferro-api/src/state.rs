use std::sync::Arc;

use ferro_auth::AuthService;
use ferro_media::MediaStore;
use ferro_storage::Repository;

pub struct AppState {
    pub repo: Arc<dyn Repository>,
    pub media: Arc<dyn MediaStore>,
    pub auth: Arc<AuthService>,
}

impl AppState {
    pub fn new(
        repo: Arc<dyn Repository>,
        media: Arc<dyn MediaStore>,
        auth: Arc<AuthService>,
    ) -> Self {
        Self { repo, media, auth }
    }
}
