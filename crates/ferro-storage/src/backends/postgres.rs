//! Postgres backend scaffold.
//!
//! Uses `sqlx` with compile-time query checks enabled via `SQLX_OFFLINE` once
//! `sqlx prepare` has been run. Schema lives in `migrations/postgres/*.sql`
//! and is applied by `migrate()`.

use async_trait::async_trait;
use ferro_core::*;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use crate::config::StorageConfig;
use crate::error::{StorageError, StorageResult};
use crate::repo::*;

pub async fn connect(cfg: &StorageConfig) -> StorageResult<Box<dyn Repository>> {
    let StorageConfig::Postgres { url, max_conns } = cfg else {
        unreachable!();
    };
    let pool = PgPoolOptions::new()
        .max_connections(*max_conns)
        .connect(url)
        .await
        .map_err(|e| StorageError::Backend(e.to_string()))?;
    Ok(Box::new(PgRepo { pool }))
}

pub struct PgRepo {
    pub(crate) pool: PgPool,
}

impl std::fmt::Debug for PgRepo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PgRepo").finish_non_exhaustive()
    }
}

#[async_trait]
impl Repository for PgRepo {
    fn sites(&self) -> &dyn SiteRepo {
        self
    }
    fn types(&self) -> &dyn ContentTypeRepo {
        self
    }
    fn content(&self) -> &dyn ContentRepo {
        self
    }
    fn users(&self) -> &dyn UserRepo {
        self
    }
    fn media(&self) -> &dyn MediaMetaRepo {
        self
    }
    async fn migrate(&self) -> StorageResult<()> {
        sqlx::migrate!("./migrations/postgres")
            .run(&self.pool)
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        Ok(())
    }
    async fn health(&self) -> StorageResult<()> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        Ok(())
    }
}

// Repo impls: scaffolds. Implement with compile-checked queries once schema is frozen.
macro_rules! tm {
    () => {
        Err(StorageError::Backend("postgres backend not yet implemented".into()))
    };
}

#[async_trait]
impl SiteRepo for PgRepo {
    async fn get(&self, _id: SiteId) -> StorageResult<Option<Site>> { tm!() }
    async fn by_slug(&self, _slug: &str) -> StorageResult<Option<Site>> { tm!() }
    async fn list(&self) -> StorageResult<Vec<Site>> { tm!() }
    async fn upsert(&self, _s: Site) -> StorageResult<Site> { tm!() }
    async fn delete(&self, _id: SiteId) -> StorageResult<()> { tm!() }
}

#[async_trait]
impl ContentTypeRepo for PgRepo {
    async fn get(&self, _id: ContentTypeId) -> StorageResult<Option<ContentType>> { tm!() }
    async fn by_slug(&self, _s: SiteId, _slug: &str) -> StorageResult<Option<ContentType>> { tm!() }
    async fn list(&self, _s: SiteId) -> StorageResult<Vec<ContentType>> { tm!() }
    async fn upsert(&self, _t: ContentType) -> StorageResult<ContentType> { tm!() }
    async fn delete(&self, _id: ContentTypeId) -> StorageResult<()> { tm!() }
}

#[async_trait]
impl ContentRepo for PgRepo {
    async fn get(&self, _id: ContentId) -> StorageResult<Option<Content>> { tm!() }
    async fn by_slug(&self, _s: SiteId, _t: ContentTypeId, _slug: &str) -> StorageResult<Option<Content>> { tm!() }
    async fn list(&self, _q: ContentQuery) -> StorageResult<Page<Content>> { tm!() }
    async fn create(&self, _s: SiteId, _n: NewContent) -> StorageResult<Content> { tm!() }
    async fn update(&self, _id: ContentId, _p: ContentPatch) -> StorageResult<Content> { tm!() }
    async fn publish(&self, _id: ContentId) -> StorageResult<Content> { tm!() }
    async fn delete(&self, _id: ContentId) -> StorageResult<()> { tm!() }
}

#[async_trait]
impl UserRepo for PgRepo {
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
impl MediaMetaRepo for PgRepo {
    async fn get(&self, _id: MediaId) -> StorageResult<Option<Media>> { tm!() }
    async fn list(&self, _s: SiteId) -> StorageResult<Vec<Media>> { tm!() }
    async fn create(&self, _m: Media) -> StorageResult<Media> { tm!() }
    async fn delete(&self, _id: MediaId) -> StorageResult<()> { tm!() }
}
