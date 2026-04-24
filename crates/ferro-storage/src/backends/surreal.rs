//! SurrealDB backend — supports embedded (RocksDB) and remote (ws/http).
//!
//! This is a scaffold: methods compile but return `todo!()` where the mapping
//! between domain types and SurrealDB records is not yet implemented. The
//! schema is created via `migrate()` using a set of `DEFINE` statements.

#![allow(clippy::module_name_repetitions)]

use async_trait::async_trait;
use surrealdb::engine::any::{connect as sdb_connect, Any};
use surrealdb::opt::auth::Root;
use surrealdb::Surreal;

use crate::config::StorageConfig;
use crate::error::{StorageError, StorageResult};
use crate::repo::*;

pub async fn connect(cfg: &StorageConfig) -> StorageResult<Box<dyn Repository>> {
    let (url, ns, db, creds) = match cfg {
        StorageConfig::SurrealEmbedded { path, namespace, database } => (
            format!("rocksdb://{}", path.display()),
            namespace.clone(),
            database.clone(),
            None,
        ),
        StorageConfig::SurrealRemote { url, namespace, database, user, pass } => (
            url.clone(),
            namespace.clone(),
            database.clone(),
            Some((user.clone(), pass.clone())),
        ),
        _ => unreachable!("surreal backend received non-surreal config"),
    };

    let db_conn: Surreal<Any> =
        sdb_connect(url).await.map_err(|e| StorageError::Backend(e.to_string()))?;

    if let Some((user, pass)) = creds {
        db_conn
            .signin(Root { username: &user, password: &pass })
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;
    }

    db_conn.use_ns(&ns).use_db(&db).await.map_err(|e| StorageError::Backend(e.to_string()))?;

    Ok(Box::new(SurrealRepo { db: db_conn }))
}

pub struct SurrealRepo {
    pub(crate) db: Surreal<Any>,
}

impl std::fmt::Debug for SurrealRepo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SurrealRepo").finish_non_exhaustive()
    }
}

#[async_trait]
impl Repository for SurrealRepo {
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
        const SCHEMA: &str = include_str!("./surreal.surql");
        self.db
            .query(SCHEMA)
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn health(&self) -> StorageResult<()> {
        self.db
            .query("RETURN 1;")
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        Ok(())
    }
}

// Repo trait impls are scaffolds — delegate to helpers once record shapes are frozen.

macro_rules! todo_repo_method {
    () => {
        Err(StorageError::Backend("surreal backend not yet implemented".into()))
    };
}

#[async_trait]
impl SiteRepo for SurrealRepo {
    async fn get(&self, _id: ferro_core::SiteId) -> StorageResult<Option<ferro_core::Site>> {
        todo_repo_method!()
    }
    async fn by_slug(&self, _slug: &str) -> StorageResult<Option<ferro_core::Site>> {
        todo_repo_method!()
    }
    async fn list(&self) -> StorageResult<Vec<ferro_core::Site>> {
        todo_repo_method!()
    }
    async fn upsert(&self, _site: ferro_core::Site) -> StorageResult<ferro_core::Site> {
        todo_repo_method!()
    }
    async fn delete(&self, _id: ferro_core::SiteId) -> StorageResult<()> {
        todo_repo_method!()
    }
}

#[async_trait]
impl ContentTypeRepo for SurrealRepo {
    async fn get(
        &self,
        _id: ferro_core::ContentTypeId,
    ) -> StorageResult<Option<ferro_core::ContentType>> {
        todo_repo_method!()
    }
    async fn by_slug(
        &self,
        _site: ferro_core::SiteId,
        _slug: &str,
    ) -> StorageResult<Option<ferro_core::ContentType>> {
        todo_repo_method!()
    }
    async fn list(&self, _site: ferro_core::SiteId) -> StorageResult<Vec<ferro_core::ContentType>> {
        todo_repo_method!()
    }
    async fn upsert(
        &self,
        _ty: ferro_core::ContentType,
    ) -> StorageResult<ferro_core::ContentType> {
        todo_repo_method!()
    }
    async fn delete(&self, _id: ferro_core::ContentTypeId) -> StorageResult<()> {
        todo_repo_method!()
    }
}

#[async_trait]
impl ContentRepo for SurrealRepo {
    async fn get(&self, _id: ferro_core::ContentId) -> StorageResult<Option<ferro_core::Content>> {
        todo_repo_method!()
    }
    async fn by_slug(
        &self,
        _site: ferro_core::SiteId,
        _ty: ferro_core::ContentTypeId,
        _slug: &str,
    ) -> StorageResult<Option<ferro_core::Content>> {
        todo_repo_method!()
    }
    async fn list(
        &self,
        _q: ferro_core::ContentQuery,
    ) -> StorageResult<ferro_core::Page<ferro_core::Content>> {
        todo_repo_method!()
    }
    async fn create(
        &self,
        _site: ferro_core::SiteId,
        _new: ferro_core::NewContent,
    ) -> StorageResult<ferro_core::Content> {
        todo_repo_method!()
    }
    async fn update(
        &self,
        _id: ferro_core::ContentId,
        _patch: ferro_core::ContentPatch,
    ) -> StorageResult<ferro_core::Content> {
        todo_repo_method!()
    }
    async fn publish(
        &self,
        _id: ferro_core::ContentId,
    ) -> StorageResult<ferro_core::Content> {
        todo_repo_method!()
    }
    async fn delete(&self, _id: ferro_core::ContentId) -> StorageResult<()> {
        todo_repo_method!()
    }
}

#[async_trait]
impl UserRepo for SurrealRepo {
    async fn get(&self, _id: ferro_core::UserId) -> StorageResult<Option<ferro_core::User>> {
        todo_repo_method!()
    }
    async fn by_email(&self, _email: &str) -> StorageResult<Option<ferro_core::User>> {
        todo_repo_method!()
    }
    async fn list(&self) -> StorageResult<Vec<ferro_core::User>> {
        todo_repo_method!()
    }
    async fn upsert(&self, _user: ferro_core::User) -> StorageResult<ferro_core::User> {
        todo_repo_method!()
    }
    async fn delete(&self, _id: ferro_core::UserId) -> StorageResult<()> {
        todo_repo_method!()
    }
    async fn get_role(&self, _id: ferro_core::RoleId) -> StorageResult<Option<ferro_core::Role>> {
        todo_repo_method!()
    }
    async fn list_roles(&self) -> StorageResult<Vec<ferro_core::Role>> {
        todo_repo_method!()
    }
    async fn upsert_role(&self, _role: ferro_core::Role) -> StorageResult<ferro_core::Role> {
        todo_repo_method!()
    }
}

#[async_trait]
impl MediaMetaRepo for SurrealRepo {
    async fn get(&self, _id: ferro_core::MediaId) -> StorageResult<Option<ferro_core::Media>> {
        todo_repo_method!()
    }
    async fn list(&self, _site: ferro_core::SiteId) -> StorageResult<Vec<ferro_core::Media>> {
        todo_repo_method!()
    }
    async fn create(&self, _m: ferro_core::Media) -> StorageResult<ferro_core::Media> {
        todo_repo_method!()
    }
    async fn delete(&self, _id: ferro_core::MediaId) -> StorageResult<()> {
        todo_repo_method!()
    }
}
