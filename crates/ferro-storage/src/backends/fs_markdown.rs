//! Flat-Markdown backend. Git-friendly content.
//!
//! Each entry is a file:
//!     <root>/<site-slug>/<type-slug>/<slug>.<locale>.md
//! Front-matter YAML carries metadata; body carries the primary rich-text field.
//! Sites, types, users, roles, media metadata live in `<root>/_meta/*.json`.
//!
//! v0.1: scaffold. Read paths first; write/query come next.

use async_trait::async_trait;
use ferro_core::*;

use crate::config::StorageConfig;
use crate::error::{StorageError, StorageResult};
use crate::repo::*;

pub async fn connect(cfg: &StorageConfig) -> StorageResult<Box<dyn Repository>> {
    let StorageConfig::FsMarkdown { path } = cfg else {
        unreachable!();
    };
    tokio::fs::create_dir_all(path.join("_meta")).await?;
    Ok(Box::new(FsMarkdownRepo { root: path.clone() }))
}

pub struct FsMarkdownRepo {
    pub(crate) root: std::path::PathBuf,
}

impl std::fmt::Debug for FsMarkdownRepo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FsMarkdownRepo").field("root", &self.root).finish()
    }
}

macro_rules! tm {
    () => {
        Err(StorageError::Backend("fs-markdown backend not yet implemented".into()))
    };
}

#[async_trait]
impl Repository for FsMarkdownRepo {
    fn sites(&self) -> &dyn SiteRepo { self }
    fn types(&self) -> &dyn ContentTypeRepo { self }
    fn content(&self) -> &dyn ContentRepo { self }
    fn users(&self) -> &dyn UserRepo { self }
    fn media(&self) -> &dyn MediaMetaRepo { self }
    async fn migrate(&self) -> StorageResult<()> { Ok(()) }
    async fn health(&self) -> StorageResult<()> {
        if self.root.exists() { Ok(()) } else {
            Err(StorageError::Backend(format!("missing root {}", self.root.display())))
        }
    }
}

#[async_trait]
impl SiteRepo for FsMarkdownRepo {
    async fn get(&self, _id: SiteId) -> StorageResult<Option<Site>> { tm!() }
    async fn by_slug(&self, _slug: &str) -> StorageResult<Option<Site>> { tm!() }
    async fn list(&self) -> StorageResult<Vec<Site>> { tm!() }
    async fn upsert(&self, _s: Site) -> StorageResult<Site> { tm!() }
    async fn delete(&self, _id: SiteId) -> StorageResult<()> { tm!() }
}

#[async_trait]
impl ContentTypeRepo for FsMarkdownRepo {
    async fn get(&self, _id: ContentTypeId) -> StorageResult<Option<ContentType>> { tm!() }
    async fn by_slug(&self, _s: SiteId, _slug: &str) -> StorageResult<Option<ContentType>> { tm!() }
    async fn list(&self, _s: SiteId) -> StorageResult<Vec<ContentType>> { tm!() }
    async fn upsert(&self, _t: ContentType) -> StorageResult<ContentType> { tm!() }
    async fn delete(&self, _id: ContentTypeId) -> StorageResult<()> { tm!() }
}

#[async_trait]
impl ContentRepo for FsMarkdownRepo {
    async fn get(&self, _id: ContentId) -> StorageResult<Option<Content>> { tm!() }
    async fn by_slug(&self, _s: SiteId, _t: ContentTypeId, _slug: &str) -> StorageResult<Option<Content>> { tm!() }
    async fn list(&self, _q: ContentQuery) -> StorageResult<Page<Content>> { tm!() }
    async fn create(&self, _s: SiteId, _n: NewContent) -> StorageResult<Content> { tm!() }
    async fn update(&self, _id: ContentId, _p: ContentPatch) -> StorageResult<Content> { tm!() }
    async fn publish(&self, _id: ContentId) -> StorageResult<Content> { tm!() }
    async fn delete(&self, _id: ContentId) -> StorageResult<()> { tm!() }
    async fn upsert(&self, _c: Content) -> StorageResult<Content> { tm!() }
}

#[async_trait]
impl UserRepo for FsMarkdownRepo {
    async fn get(&self, _id: UserId) -> StorageResult<Option<User>> { tm!() }
    async fn by_email(&self, _email: &str) -> StorageResult<Option<User>> { tm!() }
    async fn list(&self) -> StorageResult<Vec<User>> { tm!() }
    async fn upsert(&self, _u: User) -> StorageResult<User> { tm!() }
    async fn delete(&self, _id: UserId) -> StorageResult<()> { tm!() }
    async fn get_role(&self, _id: RoleId) -> StorageResult<Option<Role>> { tm!() }
    async fn list_roles(&self) -> StorageResult<Vec<Role>> { tm!() }
    async fn upsert_role(&self, _r: Role) -> StorageResult<Role> { tm!() }
}

#[async_trait]
impl MediaMetaRepo for FsMarkdownRepo {
    async fn get(&self, _id: MediaId) -> StorageResult<Option<Media>> { tm!() }
    async fn list(&self, _s: SiteId) -> StorageResult<Vec<Media>> { tm!() }
    async fn create(&self, _m: Media) -> StorageResult<Media> { tm!() }
    async fn delete(&self, _id: MediaId) -> StorageResult<()> { tm!() }
    async fn upsert(&self, _m: Media) -> StorageResult<Media> { tm!() }
}
