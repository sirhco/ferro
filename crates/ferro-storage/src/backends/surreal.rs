//! SurrealDB backend — supports embedded (RocksDB) and remote (ws/http).
//!
//! Storage strategy: tables are SCHEMALESS and Ferro keys every record by an
//! `id_str` field (our prefixed ULID). We ignore Surreal's auto-generated
//! `Thing` ids on the read path so domain types stay free of Surreal-specific
//! shapes. Filtering is done via parameterized SurrealQL (`$param` bindings).

#![allow(clippy::module_name_repetitions)]

use async_trait::async_trait;
use ferro_core::{
    Content, ContentId, ContentPatch, ContentQuery, ContentType, ContentTypeId, Locale, Media,
    MediaId, NewContent, Page, Role, RoleId, Site, SiteId, Status, User, UserId,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
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

fn map_err<E: std::fmt::Display>(e: E) -> StorageError {
    StorageError::Backend(e.to_string())
}

impl SurrealRepo {
    async fn upsert_record<T: Serialize>(
        &self,
        table: &str,
        id_str: &str,
        record: &T,
    ) -> StorageResult<()> {
        // Surreal's bind serializer rejects `serde_json::Value` enum variants,
        // and its `.content(T)` API trips over our untagged `FieldValue`
        // representation. Sidestep both by serializing to a JSON string and
        // letting SurrealDB parse it server-side via `type::object`.
        let mut value = serde_json::to_value(record).map_err(map_err)?;
        if let Some(obj) = value.as_object_mut() {
            obj.remove("id");
            obj.insert("id_str".into(), serde_json::Value::String(id_str.to_string()));
        }
        // SurrealQL accepts JSON object literals inline as expressions, and
        // serde_json's escaping is compatible with SurrealQL string syntax.
        // Inlining sidesteps the bind-serializer's enum-handling limitation.
        let json = serde_json::to_string(&value).map_err(map_err)?;
        let record_id = sanitize_record_id(id_str);
        self.db
            .query(format!(
                "UPSERT type::thing($tbl, $rid) CONTENT {json}"
            ))
            .bind(("tbl", table.to_string()))
            .bind(("rid", record_id))
            .await
            .map_err(map_err)?
            .check()
            .map_err(map_err)?;
        Ok(())
    }

    async fn get_one<T: DeserializeOwned>(
        &self,
        table: &str,
        id_str: &str,
    ) -> StorageResult<Option<T>> {
        let mut resp = self
            .db
            .query(format!("SELECT * OMIT id FROM {table} WHERE id_str = $id LIMIT 1"))
            .bind(("id", id_str.to_string()))
            .await
            .map_err(map_err)?
            .check()
            .map_err(map_err)?;
        let v: Vec<serde_json::Value> = resp.take(0).map_err(map_err)?;
        v.into_iter().next().map(strip_and_decode).transpose()
    }

    async fn delete_record(&self, table: &str, id_str: &str) -> StorageResult<()> {
        self.db
            .query(format!("DELETE FROM {table} WHERE id_str = $id"))
            .bind(("id", id_str.to_string()))
            .await
            .map_err(map_err)?
            .check()
            .map_err(map_err)?;
        Ok(())
    }

    async fn list_all<T: DeserializeOwned>(&self, table: &str) -> StorageResult<Vec<T>> {
        let mut resp = self
            .db
            .query(format!("SELECT * OMIT id FROM {table}"))
            .await
            .map_err(map_err)?
            .check()
            .map_err(map_err)?;
        let rows: Vec<serde_json::Value> = resp.take(0).map_err(map_err)?;
        rows.into_iter().map(strip_and_decode).collect()
    }
}

/// Restore the domain `id` from `id_str` (we strip the original `id` field on
/// write to satisfy SurrealDB's upsert contract). The id_str format is
/// `<prefix>_<ulid>`; domain `FromStr` impls accept that directly.
fn strip_and_decode<T: DeserializeOwned>(mut row: serde_json::Value) -> StorageResult<T> {
    if let Some(obj) = row.as_object_mut() {
        // SurrealDB injects a Thing into `id` on every row. Throw it away and
        // rebuild the typed id from `id_str` so the domain shape is intact.
        obj.remove("id");
        if let Some(id_str) = obj.remove("id_str") {
            if let serde_json::Value::String(s) = id_str {
                // Strip the typed prefix: `site_01HK...` → `"01HK..."`. Domain
                // `FromStr` impls accept either form, but writing the bare ULID
                // keeps the wire shape aligned with serde's `transparent` repr.
                let bare = s.split_once('_').map(|(_, ulid)| ulid).unwrap_or(&s);
                obj.insert("id".into(), serde_json::Value::String(bare.to_string()));
            }
        }
    }
    serde_json::from_value(row).map_err(map_err)
}

/// SurrealDB record ids accept alphanumerics + underscores without escaping.
/// Our typed-id Display form (`<prefix>_<ulid>`) already satisfies that, but
/// guard against future format changes.
fn sanitize_record_id(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect()
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
            .map_err(map_err)?
            .check()
            .map_err(map_err)?;
        Ok(())
    }

    async fn health(&self) -> StorageResult<()> {
        self.db.query("RETURN 1;").await.map_err(map_err)?.check().map_err(map_err)?;
        Ok(())
    }
}

// --- SiteRepo ---

#[async_trait]
impl SiteRepo for SurrealRepo {
    async fn get(&self, id: SiteId) -> StorageResult<Option<Site>> {
        self.get_one("site", &id.to_string()).await
    }

    async fn by_slug(&self, slug: &str) -> StorageResult<Option<Site>> {
        let mut resp = self
            .db
            .query("SELECT * OMIT id FROM site WHERE slug = $slug LIMIT 1")
            .bind(("slug", slug.to_string()))
            .await
            .map_err(map_err)?
            .check()
            .map_err(map_err)?;
        let rows: Vec<serde_json::Value> = resp.take(0).map_err(map_err)?;
        rows.into_iter().next().map(strip_and_decode).transpose()
    }

    async fn list(&self) -> StorageResult<Vec<Site>> {
        self.list_all("site").await
    }

    async fn upsert(&self, site: Site) -> StorageResult<Site> {
        self.upsert_record("site", &site.id.to_string(), &site).await?;
        Ok(site)
    }

    async fn delete(&self, id: SiteId) -> StorageResult<()> {
        self.delete_record("site", &id.to_string()).await
    }
}

// --- ContentTypeRepo ---
//
// Domain typed-ids serialize as bare ULID strings (transparent), so filters
// that reference `site_id`/`type_id` columns must bind the bare ULID — not
// the prefixed Display form.

#[async_trait]
impl ContentTypeRepo for SurrealRepo {
    async fn get(&self, id: ContentTypeId) -> StorageResult<Option<ContentType>> {
        self.get_one("content_type", &id.to_string()).await
    }

    async fn by_slug(&self, site: SiteId, slug: &str) -> StorageResult<Option<ContentType>> {
        let mut resp = self
            .db
            .query(
                "SELECT * OMIT id FROM content_type WHERE site_id = $site AND slug = $slug LIMIT 1",
            )
            .bind(("site", site.0.to_string()))
            .bind(("slug", slug.to_string()))
            .await
            .map_err(map_err)?
            .check()
            .map_err(map_err)?;
        let rows: Vec<serde_json::Value> = resp.take(0).map_err(map_err)?;
        rows.into_iter().next().map(strip_and_decode).transpose()
    }

    async fn list(&self, site: SiteId) -> StorageResult<Vec<ContentType>> {
        let mut resp = self
            .db
            .query("SELECT * OMIT id FROM content_type WHERE site_id = $site")
            .bind(("site", site.0.to_string()))
            .await
            .map_err(map_err)?
            .check()
            .map_err(map_err)?;
        let rows: Vec<serde_json::Value> = resp.take(0).map_err(map_err)?;
        rows.into_iter().map(strip_and_decode).collect()
    }

    async fn upsert(&self, ty: ContentType) -> StorageResult<ContentType> {
        self.upsert_record("content_type", &ty.id.to_string(), &ty).await?;
        Ok(ty)
    }

    async fn delete(&self, id: ContentTypeId) -> StorageResult<()> {
        self.delete_record("content_type", &id.to_string()).await
    }
}

// --- ContentRepo ---

#[async_trait]
impl ContentRepo for SurrealRepo {
    async fn get(&self, id: ContentId) -> StorageResult<Option<Content>> {
        self.get_one("content", &id.to_string()).await
    }

    async fn by_slug(
        &self,
        site: SiteId,
        ty: ContentTypeId,
        slug: &str,
    ) -> StorageResult<Option<Content>> {
        let mut resp = self
            .db
            .query(
                "SELECT * OMIT id FROM content WHERE site_id = $site AND type_id = $ty AND slug = $slug LIMIT 1",
            )
            .bind(("site", site.0.to_string()))
            .bind(("ty", ty.0.to_string()))
            .bind(("slug", slug.to_string()))
            .await
            .map_err(map_err)?
            .check()
            .map_err(map_err)?;
        let rows: Vec<serde_json::Value> = resp.take(0).map_err(map_err)?;
        rows.into_iter().next().map(strip_and_decode).transpose()
    }

    async fn list(&self, q: ContentQuery) -> StorageResult<Page<Content>> {
        let page = q.page.unwrap_or(1).max(1);
        let per_page = q.per_page.unwrap_or(20).max(1);
        let limit = per_page as i64;
        let start = ((page - 1) * per_page) as i64;

        let mut where_clauses: Vec<&str> = Vec::new();
        if q.site_id.is_some() {
            where_clauses.push("site_id = $site");
        }
        if q.type_id.is_some() {
            where_clauses.push("type_id = $ty");
        }
        if q.status.is_some() {
            where_clauses.push("status = $status");
        }
        if q.locale.is_some() {
            where_clauses.push("locale = $locale");
        }
        let where_sql = if where_clauses.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", where_clauses.join(" AND "))
        };

        let mut query = self.db.query(format!(
            "SELECT * OMIT id FROM content{where_sql} ORDER BY created_at DESC LIMIT $limit START $start; \
             SELECT count() FROM content{where_sql} GROUP ALL"
        ));
        if let Some(s) = q.site_id {
            query = query.bind(("site", s.0.to_string()));
        }
        if let Some(t) = q.type_id {
            query = query.bind(("ty", t.0.to_string()));
        }
        if let Some(st) = q.status {
            query = query.bind(("status", status_str(st).to_string()));
        }
        if let Some(l) = q.locale.as_ref() {
            query = query.bind(("locale", l.to_string()));
        }
        query = query.bind(("limit", limit)).bind(("start", start));
        let mut resp = query.await.map_err(map_err)?.check().map_err(map_err)?;

        let rows: Vec<serde_json::Value> = resp.take(0).map_err(map_err)?;
        let items: Vec<Content> =
            rows.into_iter().map(strip_and_decode).collect::<StorageResult<Vec<_>>>()?;
        let counts: Vec<serde_json::Value> = resp.take(1).map_err(map_err)?;
        let total = counts
            .first()
            .and_then(|v| v.get("count"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        Ok(Page { items, total, page, per_page })
    }

    async fn create(&self, site: SiteId, new: NewContent) -> StorageResult<Content> {
        let now = time::OffsetDateTime::now_utc();
        let c = Content {
            id: ContentId::new(),
            site_id: site,
            type_id: new.type_id,
            slug: new.slug,
            locale: new.locale,
            status: Status::Draft,
            data: new.data,
            author_id: new.author_id,
            created_at: now,
            updated_at: now,
            published_at: None,
        };
        self.upsert_record("content", &c.id.to_string(), &c).await?;
        Ok(c)
    }

    async fn update(&self, id: ContentId, patch: ContentPatch) -> StorageResult<Content> {
        let mut current: Content = self
            .get_one("content", &id.to_string())
            .await?
            .ok_or(StorageError::NotFound)?;
        if let Some(slug) = patch.slug {
            current.slug = slug;
        }
        if let Some(status) = patch.status {
            current.status = status;
        }
        if let Some(data) = patch.data {
            current.data = data;
        }
        current.updated_at = time::OffsetDateTime::now_utc();
        self.upsert_record("content", &current.id.to_string(), &current).await?;
        Ok(current)
    }

    async fn publish(&self, id: ContentId) -> StorageResult<Content> {
        let mut current: Content = self
            .get_one("content", &id.to_string())
            .await?
            .ok_or(StorageError::NotFound)?;
        let now = time::OffsetDateTime::now_utc();
        current.status = Status::Published;
        current.published_at = Some(now);
        current.updated_at = now;
        self.upsert_record("content", &current.id.to_string(), &current).await?;
        Ok(current)
    }

    async fn delete(&self, id: ContentId) -> StorageResult<()> {
        self.delete_record("content", &id.to_string()).await
    }

    async fn upsert(&self, c: Content) -> StorageResult<Content> {
        self.upsert_record("content", &c.id.to_string(), &c).await?;
        Ok(c)
    }
}

// --- UserRepo ---

#[async_trait]
impl UserRepo for SurrealRepo {
    async fn get(&self, id: UserId) -> StorageResult<Option<User>> {
        self.get_one("user", &id.to_string()).await
    }

    async fn by_email(&self, email: &str) -> StorageResult<Option<User>> {
        let mut resp = self
            .db
            .query("SELECT * OMIT id FROM user WHERE string::lowercase(email) = string::lowercase($email) LIMIT 1")
            .bind(("email", email.to_string()))
            .await
            .map_err(map_err)?
            .check()
            .map_err(map_err)?;
        let rows: Vec<serde_json::Value> = resp.take(0).map_err(map_err)?;
        rows.into_iter().next().map(strip_and_decode).transpose()
    }

    async fn list(&self) -> StorageResult<Vec<User>> {
        self.list_all("user").await
    }

    async fn upsert(&self, user: User) -> StorageResult<User> {
        self.upsert_record("user", &user.id.to_string(), &user).await?;
        // `User.password_hash` carries `#[serde(skip_serializing)]`, so the
        // upsert above drops it. Persist it via a targeted UPDATE so login
        // flows can read it back.
        if let Some(hash) = &user.password_hash {
            let record_id = sanitize_record_id(&user.id.to_string());
            self.db
                .query("UPDATE type::thing($tbl, $rid) SET password_hash = $hash")
                .bind(("tbl", "user".to_string()))
                .bind(("rid", record_id))
                .bind(("hash", hash.clone()))
                .await
                .map_err(map_err)?
                .check()
                .map_err(map_err)?;
        }
        Ok(user)
    }

    async fn delete(&self, id: UserId) -> StorageResult<()> {
        self.delete_record("user", &id.to_string()).await
    }

    async fn get_role(&self, id: RoleId) -> StorageResult<Option<Role>> {
        self.get_one("role", &id.to_string()).await
    }

    async fn list_roles(&self) -> StorageResult<Vec<Role>> {
        self.list_all("role").await
    }

    async fn upsert_role(&self, role: Role) -> StorageResult<Role> {
        self.upsert_record("role", &role.id.to_string(), &role).await?;
        Ok(role)
    }
}

// --- MediaMetaRepo ---

#[async_trait]
impl MediaMetaRepo for SurrealRepo {
    async fn get(&self, id: MediaId) -> StorageResult<Option<Media>> {
        self.get_one("media", &id.to_string()).await
    }

    async fn list(&self, site: SiteId) -> StorageResult<Vec<Media>> {
        let mut resp = self
            .db
            .query("SELECT * OMIT id FROM media WHERE site_id = $site ORDER BY created_at DESC")
            .bind(("site", site.0.to_string()))
            .await
            .map_err(map_err)?
            .check()
            .map_err(map_err)?;
        let rows: Vec<serde_json::Value> = resp.take(0).map_err(map_err)?;
        rows.into_iter().map(strip_and_decode).collect()
    }

    async fn create(&self, m: Media) -> StorageResult<Media> {
        self.upsert_record("media", &m.id.to_string(), &m).await?;
        Ok(m)
    }

    async fn delete(&self, id: MediaId) -> StorageResult<()> {
        self.delete_record("media", &id.to_string()).await
    }

    async fn upsert(&self, m: Media) -> StorageResult<Media> {
        self.upsert_record("media", &m.id.to_string(), &m).await?;
        Ok(m)
    }
}

fn status_str(s: Status) -> &'static str {
    match s {
        Status::Draft => "draft",
        Status::Published => "published",
        Status::Archived => "archived",
    }
}

// `Locale`, `RoleId` etc. unused in this module; keep imports tidy.
#[allow(dead_code)]
fn _imports_used(_: &Locale) {}
