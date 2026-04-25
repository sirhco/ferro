use async_trait::async_trait;
use ferro_core::{
    Content, ContentId, ContentPatch, ContentQuery, ContentType, ContentTypeId, Media, MediaId,
    NewContent, Page, Role, RoleId, Site, SiteId, User, UserId,
};

use crate::error::StorageResult;

#[async_trait]
pub trait Repository: Send + Sync + 'static {
    fn sites(&self) -> &dyn SiteRepo;
    fn types(&self) -> &dyn ContentTypeRepo;
    fn content(&self) -> &dyn ContentRepo;
    fn users(&self) -> &dyn UserRepo;
    fn media(&self) -> &dyn MediaMetaRepo;

    async fn migrate(&self) -> StorageResult<()>;
    async fn health(&self) -> StorageResult<()>;
}

#[async_trait]
pub trait SiteRepo: Send + Sync {
    async fn get(&self, id: SiteId) -> StorageResult<Option<Site>>;
    async fn by_slug(&self, slug: &str) -> StorageResult<Option<Site>>;
    async fn list(&self) -> StorageResult<Vec<Site>>;
    async fn upsert(&self, site: Site) -> StorageResult<Site>;
    async fn delete(&self, id: SiteId) -> StorageResult<()>;
}

#[async_trait]
pub trait ContentTypeRepo: Send + Sync {
    async fn get(&self, id: ContentTypeId) -> StorageResult<Option<ContentType>>;
    async fn by_slug(&self, site: SiteId, slug: &str) -> StorageResult<Option<ContentType>>;
    async fn list(&self, site: SiteId) -> StorageResult<Vec<ContentType>>;
    async fn upsert(&self, ty: ContentType) -> StorageResult<ContentType>;
    async fn delete(&self, id: ContentTypeId) -> StorageResult<()>;
}

#[async_trait]
pub trait ContentRepo: Send + Sync {
    async fn get(&self, id: ContentId) -> StorageResult<Option<Content>>;
    async fn by_slug(
        &self,
        site: SiteId,
        ty: ContentTypeId,
        slug: &str,
    ) -> StorageResult<Option<Content>>;
    async fn list(&self, q: ContentQuery) -> StorageResult<Page<Content>>;
    async fn create(&self, site: SiteId, new: NewContent) -> StorageResult<Content>;
    async fn update(&self, id: ContentId, patch: ContentPatch) -> StorageResult<Content>;
    async fn publish(&self, id: ContentId) -> StorageResult<Content>;
    async fn delete(&self, id: ContentId) -> StorageResult<()>;

    /// Insert or replace a full `Content` record verbatim (preserving ids and
    /// timestamps). Used by import/migration tooling; user-facing writes should
    /// go through `create`/`update`.
    async fn upsert(&self, content: Content) -> StorageResult<Content>;
}

#[async_trait]
pub trait UserRepo: Send + Sync {
    async fn get(&self, id: UserId) -> StorageResult<Option<User>>;
    async fn by_email(&self, email: &str) -> StorageResult<Option<User>>;
    async fn list(&self) -> StorageResult<Vec<User>>;
    async fn upsert(&self, user: User) -> StorageResult<User>;
    async fn delete(&self, id: UserId) -> StorageResult<()>;

    async fn get_role(&self, id: RoleId) -> StorageResult<Option<Role>>;
    async fn list_roles(&self) -> StorageResult<Vec<Role>>;
    async fn upsert_role(&self, role: Role) -> StorageResult<Role>;
    async fn delete_role(&self, id: RoleId) -> StorageResult<()>;
}

#[async_trait]
pub trait MediaMetaRepo: Send + Sync {
    async fn get(&self, id: MediaId) -> StorageResult<Option<Media>>;
    async fn list(&self, site: SiteId) -> StorageResult<Vec<Media>>;
    async fn create(&self, m: Media) -> StorageResult<Media>;
    async fn delete(&self, id: MediaId) -> StorageResult<()>;

    /// Insert or replace a media record verbatim. Used by import tooling.
    async fn upsert(&self, m: Media) -> StorageResult<Media>;
}
