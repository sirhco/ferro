//! Postgres backend.
//!
//! Uses `sqlx` with runtime-verified queries (no `query!` macros) so the crate
//! builds without a live DB. Schema lives in `migrations/postgres/*.sql` and
//! is applied by `migrate()`.

use std::str::FromStr;

use async_trait::async_trait;
use ferro_core::*;
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};

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

// --- helpers ---

fn map_sqlx(e: sqlx::Error) -> StorageError {
    match &e {
        sqlx::Error::RowNotFound => StorageError::NotFound,
        sqlx::Error::Database(db) => {
            if db.code().as_deref() == Some("23505") {
                StorageError::UniqueViolation { field: "unique" }
            } else {
                StorageError::Backend(e.to_string())
            }
        }
        _ => StorageError::Backend(e.to_string()),
    }
}

fn map_json<E: std::fmt::Display>(e: E) -> StorageError {
    StorageError::Serde(e.to_string())
}

fn parse_id<T: FromStr>(s: &str) -> StorageResult<T>
where
    T::Err: std::fmt::Display,
{
    T::from_str(s).map_err(|e| StorageError::Backend(format!("invalid id `{s}`: {e}")))
}

fn status_str(s: Status) -> &'static str {
    match s {
        Status::Draft => "draft",
        Status::Published => "published",
        Status::Archived => "archived",
    }
}

fn parse_status(s: &str) -> StorageResult<Status> {
    match s {
        "draft" => Ok(Status::Draft),
        "published" => Ok(Status::Published),
        "archived" => Ok(Status::Archived),
        other => Err(StorageError::Backend(format!("unknown status `{other}`"))),
    }
}

fn parse_media_kind(s: &str) -> StorageResult<MediaKind> {
    match s {
        "image" => Ok(MediaKind::Image),
        "video" => Ok(MediaKind::Video),
        "audio" => Ok(MediaKind::Audio),
        "document" => Ok(MediaKind::Document),
        "other" => Ok(MediaKind::Other),
        o => Err(StorageError::Backend(format!("unknown media kind `{o}`"))),
    }
}

fn media_kind_str(k: MediaKind) -> &'static str {
    match k {
        MediaKind::Image => "image",
        MediaKind::Video => "video",
        MediaKind::Audio => "audio",
        MediaKind::Document => "document",
        MediaKind::Other => "other",
    }
}

// --- row mappers ---

fn row_to_site(row: &sqlx::postgres::PgRow) -> StorageResult<Site> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let primary_url: Option<String> = row.try_get("primary_url").map_err(map_sqlx)?;
    let locales_raw: Vec<String> = row.try_get("locales").map_err(map_sqlx)?;
    let default_locale: String = row.try_get("default_locale").map_err(map_sqlx)?;
    let settings: serde_json::Value = row.try_get("settings").map_err(map_sqlx)?;
    let locales: Vec<Locale> = locales_raw
        .into_iter()
        .map(Locale::new)
        .collect::<Result<_, _>>()
        .map_err(|e| StorageError::Backend(e.to_string()))?;
    Ok(Site {
        id: parse_id(&id)?,
        slug: row.try_get("slug").map_err(map_sqlx)?,
        name: row.try_get("name").map_err(map_sqlx)?,
        description: row.try_get("description").map_err(map_sqlx)?,
        primary_url: primary_url
            .map(|s| url::Url::parse(&s))
            .transpose()
            .map_err(|e| StorageError::Backend(e.to_string()))?,
        locales,
        default_locale: Locale::new(default_locale)
            .map_err(|e| StorageError::Backend(e.to_string()))?,
        settings: serde_json::from_value(settings).map_err(map_json)?,
        created_at: row.try_get("created_at").map_err(map_sqlx)?,
        updated_at: row.try_get("updated_at").map_err(map_sqlx)?,
    })
}

fn row_to_type(row: &sqlx::postgres::PgRow) -> StorageResult<ContentType> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let site_id: String = row.try_get("site_id").map_err(map_sqlx)?;
    let fields: serde_json::Value = row.try_get("fields").map_err(map_sqlx)?;
    Ok(ContentType {
        id: parse_id(&id)?,
        site_id: parse_id(&site_id)?,
        slug: row.try_get("slug").map_err(map_sqlx)?,
        name: row.try_get("name").map_err(map_sqlx)?,
        description: row.try_get("description").map_err(map_sqlx)?,
        fields: serde_json::from_value(fields).map_err(map_json)?,
        singleton: row.try_get("singleton").map_err(map_sqlx)?,
        title_field: row.try_get("title_field").map_err(map_sqlx)?,
        slug_field: row.try_get("slug_field").map_err(map_sqlx)?,
        created_at: row.try_get("created_at").map_err(map_sqlx)?,
        updated_at: row.try_get("updated_at").map_err(map_sqlx)?,
    })
}

fn row_to_content(row: &sqlx::postgres::PgRow) -> StorageResult<Content> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let site_id: String = row.try_get("site_id").map_err(map_sqlx)?;
    let type_id: String = row.try_get("type_id").map_err(map_sqlx)?;
    let locale: String = row.try_get("locale").map_err(map_sqlx)?;
    let status: String = row.try_get("status").map_err(map_sqlx)?;
    let data: serde_json::Value = row.try_get("data").map_err(map_sqlx)?;
    let author_id: Option<String> = row.try_get("author_id").map_err(map_sqlx)?;
    Ok(Content {
        id: parse_id(&id)?,
        site_id: parse_id(&site_id)?,
        type_id: parse_id(&type_id)?,
        slug: row.try_get("slug").map_err(map_sqlx)?,
        locale: Locale::new(locale).map_err(|e| StorageError::Backend(e.to_string()))?,
        status: parse_status(&status)?,
        data: serde_json::from_value(data).map_err(map_json)?,
        author_id: author_id.map(|s| parse_id(&s)).transpose()?,
        created_at: row.try_get("created_at").map_err(map_sqlx)?,
        updated_at: row.try_get("updated_at").map_err(map_sqlx)?,
        published_at: row.try_get("published_at").map_err(map_sqlx)?,
    })
}

fn row_to_user(row: &sqlx::postgres::PgRow) -> StorageResult<User> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let roles_raw: Vec<String> = row.try_get("roles").map_err(map_sqlx)?;
    let roles = roles_raw
        .into_iter()
        .map(|s| parse_id::<RoleId>(&s))
        .collect::<StorageResult<Vec<_>>>()?;
    Ok(User {
        id: parse_id(&id)?,
        email: row.try_get("email").map_err(map_sqlx)?,
        handle: row.try_get("handle").map_err(map_sqlx)?,
        display_name: row.try_get("display_name").map_err(map_sqlx)?,
        password_hash: row.try_get("password_hash").map_err(map_sqlx)?,
        roles,
        active: row.try_get("active").map_err(map_sqlx)?,
        created_at: row.try_get("created_at").map_err(map_sqlx)?,
        last_login: row.try_get("last_login").map_err(map_sqlx)?,
    })
}

fn row_to_role(row: &sqlx::postgres::PgRow) -> StorageResult<Role> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let permissions: serde_json::Value = row.try_get("permissions").map_err(map_sqlx)?;
    Ok(Role {
        id: parse_id(&id)?,
        name: row.try_get("name").map_err(map_sqlx)?,
        description: row.try_get("description").map_err(map_sqlx)?,
        permissions: serde_json::from_value(permissions).map_err(map_json)?,
    })
}

fn row_to_media(row: &sqlx::postgres::PgRow) -> StorageResult<Media> {
    let id: String = row.try_get("id").map_err(map_sqlx)?;
    let site_id: String = row.try_get("site_id").map_err(map_sqlx)?;
    let uploaded_by: Option<String> = row.try_get("uploaded_by").map_err(map_sqlx)?;
    let size: i64 = row.try_get("size").map_err(map_sqlx)?;
    let width: Option<i32> = row.try_get("width").map_err(map_sqlx)?;
    let height: Option<i32> = row.try_get("height").map_err(map_sqlx)?;
    let kind: String = row.try_get("kind").map_err(map_sqlx)?;
    Ok(Media {
        id: parse_id(&id)?,
        site_id: parse_id(&site_id)?,
        key: row.try_get("key").map_err(map_sqlx)?,
        filename: row.try_get("filename").map_err(map_sqlx)?,
        mime: row.try_get("mime").map_err(map_sqlx)?,
        size: size as u64,
        width: width.map(|w| w as u32),
        height: height.map(|h| h as u32),
        alt: row.try_get("alt").map_err(map_sqlx)?,
        kind: parse_media_kind(&kind)?,
        uploaded_by: uploaded_by.map(|s| parse_id(&s)).transpose()?,
        created_at: row.try_get("created_at").map_err(map_sqlx)?,
    })
}

// --- Repository root ---

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
        sqlx::query("SELECT 1").execute(&self.pool).await.map_err(map_sqlx)?;
        Ok(())
    }
}

// --- SiteRepo ---

#[async_trait]
impl SiteRepo for PgRepo {
    async fn get(&self, id: SiteId) -> StorageResult<Option<Site>> {
        let row = sqlx::query("SELECT * FROM sites WHERE id = $1")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(map_sqlx)?;
        row.as_ref().map(row_to_site).transpose()
    }

    async fn by_slug(&self, slug: &str) -> StorageResult<Option<Site>> {
        let row = sqlx::query("SELECT * FROM sites WHERE slug = $1")
            .bind(slug)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_sqlx)?;
        row.as_ref().map(row_to_site).transpose()
    }

    async fn list(&self) -> StorageResult<Vec<Site>> {
        let rows = sqlx::query("SELECT * FROM sites ORDER BY created_at ASC")
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx)?;
        rows.iter().map(row_to_site).collect()
    }

    async fn upsert(&self, site: Site) -> StorageResult<Site> {
        let primary_url = site.primary_url.as_ref().map(url::Url::as_str);
        let locales: Vec<String> = site.locales.iter().map(Locale::to_string).collect();
        let settings = serde_json::to_value(&site.settings).map_err(map_json)?;
        sqlx::query(
            r#"
            INSERT INTO sites (id, slug, name, description, primary_url, locales,
                               default_locale, settings, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (id) DO UPDATE SET
                slug = EXCLUDED.slug,
                name = EXCLUDED.name,
                description = EXCLUDED.description,
                primary_url = EXCLUDED.primary_url,
                locales = EXCLUDED.locales,
                default_locale = EXCLUDED.default_locale,
                settings = EXCLUDED.settings,
                updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(site.id.to_string())
        .bind(&site.slug)
        .bind(&site.name)
        .bind(&site.description)
        .bind(primary_url)
        .bind(&locales)
        .bind(site.default_locale.to_string())
        .bind(&settings)
        .bind(site.created_at)
        .bind(site.updated_at)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(site)
    }

    async fn delete(&self, id: SiteId) -> StorageResult<()> {
        sqlx::query("DELETE FROM sites WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(map_sqlx)?;
        Ok(())
    }
}

// --- ContentTypeRepo ---

#[async_trait]
impl ContentTypeRepo for PgRepo {
    async fn get(&self, id: ContentTypeId) -> StorageResult<Option<ContentType>> {
        let row = sqlx::query("SELECT * FROM content_types WHERE id = $1")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(map_sqlx)?;
        row.as_ref().map(row_to_type).transpose()
    }

    async fn by_slug(&self, site: SiteId, slug: &str) -> StorageResult<Option<ContentType>> {
        let row = sqlx::query("SELECT * FROM content_types WHERE site_id = $1 AND slug = $2")
            .bind(site.to_string())
            .bind(slug)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_sqlx)?;
        row.as_ref().map(row_to_type).transpose()
    }

    async fn list(&self, site: SiteId) -> StorageResult<Vec<ContentType>> {
        let rows = sqlx::query(
            "SELECT * FROM content_types WHERE site_id = $1 ORDER BY created_at ASC",
        )
        .bind(site.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;
        rows.iter().map(row_to_type).collect()
    }

    async fn upsert(&self, ty: ContentType) -> StorageResult<ContentType> {
        let fields = serde_json::to_value(&ty.fields).map_err(map_json)?;
        sqlx::query(
            r#"
            INSERT INTO content_types (id, site_id, slug, name, description, fields,
                                       singleton, title_field, slug_field,
                                       created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            ON CONFLICT (id) DO UPDATE SET
                slug = EXCLUDED.slug,
                name = EXCLUDED.name,
                description = EXCLUDED.description,
                fields = EXCLUDED.fields,
                singleton = EXCLUDED.singleton,
                title_field = EXCLUDED.title_field,
                slug_field = EXCLUDED.slug_field,
                updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(ty.id.to_string())
        .bind(ty.site_id.to_string())
        .bind(&ty.slug)
        .bind(&ty.name)
        .bind(&ty.description)
        .bind(&fields)
        .bind(ty.singleton)
        .bind(&ty.title_field)
        .bind(&ty.slug_field)
        .bind(ty.created_at)
        .bind(ty.updated_at)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(ty)
    }

    async fn delete(&self, id: ContentTypeId) -> StorageResult<()> {
        sqlx::query("DELETE FROM content_types WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(map_sqlx)?;
        Ok(())
    }
}

// --- ContentRepo ---

#[async_trait]
impl ContentRepo for PgRepo {
    async fn get(&self, id: ContentId) -> StorageResult<Option<Content>> {
        let row = sqlx::query("SELECT * FROM content WHERE id = $1")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(map_sqlx)?;
        row.as_ref().map(row_to_content).transpose()
    }

    async fn by_slug(
        &self,
        site: SiteId,
        ty: ContentTypeId,
        slug: &str,
    ) -> StorageResult<Option<Content>> {
        let row = sqlx::query(
            "SELECT * FROM content WHERE site_id = $1 AND type_id = $2 AND slug = $3 LIMIT 1",
        )
        .bind(site.to_string())
        .bind(ty.to_string())
        .bind(slug)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;
        row.as_ref().map(row_to_content).transpose()
    }

    async fn list(&self, q: ContentQuery) -> StorageResult<Page<Content>> {
        let page = q.page.unwrap_or(1).max(1);
        let per_page = q.per_page.unwrap_or(20).max(1);
        let offset = ((page - 1) * per_page) as i64;
        let limit = per_page as i64;

        // Collect bound filters into separate variables so the query string can
        // reference them by `$N` positionally.
        let site_id = q.site_id.map(|s| s.to_string());
        let type_id = q.type_id.map(|t| t.to_string());
        let type_slug = q.type_slug;
        let status = q.status.map(status_str);
        let locale = q.locale.as_ref().map(Locale::to_string);

        // Resolve type_slug to type_id if needed (requires a known site).
        let resolved_type_id = match (&type_id, &type_slug, &site_id) {
            (Some(t), _, _) => Some(t.clone()),
            (None, Some(slug), Some(sid)) => {
                let row = sqlx::query(
                    "SELECT id FROM content_types WHERE site_id = $1 AND slug = $2",
                )
                .bind(sid)
                .bind(slug)
                .fetch_optional(&self.pool)
                .await
                .map_err(map_sqlx)?;
                row.map(|r| r.try_get::<String, _>("id")).transpose().map_err(map_sqlx)?
            }
            (None, Some(slug), None) => {
                // No site filter; look up by slug alone (may be ambiguous but acceptable for v0.3).
                let row = sqlx::query("SELECT id FROM content_types WHERE slug = $1 LIMIT 1")
                    .bind(slug)
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(map_sqlx)?;
                row.map(|r| r.try_get::<String, _>("id")).transpose().map_err(map_sqlx)?
            }
            _ => None,
        };

        let mut where_clauses = Vec::new();
        let mut idx: usize = 1;
        let mut sql = String::from("SELECT * FROM content");
        let mut count_sql = String::from("SELECT COUNT(*) FROM content");

        if site_id.is_some() {
            where_clauses.push(format!("site_id = ${idx}"));
            idx += 1;
        }
        if resolved_type_id.is_some() {
            where_clauses.push(format!("type_id = ${idx}"));
            idx += 1;
        }
        if status.is_some() {
            where_clauses.push(format!("status = ${idx}"));
            idx += 1;
        }
        if locale.is_some() {
            where_clauses.push(format!("locale = ${idx}"));
            idx += 1;
        }

        if !where_clauses.is_empty() {
            let joined = where_clauses.join(" AND ");
            sql.push_str(&format!(" WHERE {joined}"));
            count_sql.push_str(&format!(" WHERE {joined}"));
        }
        sql.push_str(&format!(" ORDER BY created_at DESC LIMIT ${idx} OFFSET ${}", idx + 1));

        fn bind_filters<'a>(
            mut q: sqlx::query::Query<'a, sqlx::Postgres, sqlx::postgres::PgArguments>,
            site_id: &'a Option<String>,
            type_id: &'a Option<String>,
            status: &'a Option<&'a str>,
            locale: &'a Option<String>,
        ) -> sqlx::query::Query<'a, sqlx::Postgres, sqlx::postgres::PgArguments> {
            if let Some(s) = site_id {
                q = q.bind(s);
            }
            if let Some(t) = type_id {
                q = q.bind(t);
            }
            if let Some(s) = status {
                q = q.bind(*s);
            }
            if let Some(l) = locale {
                q = q.bind(l);
            }
            q
        }

        let count_row = {
            let q = sqlx::query(&count_sql);
            let q = bind_filters(q, &site_id, &resolved_type_id, &status, &locale);
            q.fetch_one(&self.pool).await.map_err(map_sqlx)?
        };
        let total: i64 = count_row.try_get(0).map_err(map_sqlx)?;

        let rows = {
            let q = sqlx::query(&sql);
            let q = bind_filters(q, &site_id, &resolved_type_id, &status, &locale);
            let q = q.bind(limit).bind(offset);
            q.fetch_all(&self.pool).await.map_err(map_sqlx)?
        };
        let items = rows.iter().map(row_to_content).collect::<StorageResult<Vec<_>>>()?;
        Ok(Page { items, total: total as u64, page, per_page })
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
        let data = serde_json::to_value(&c.data).map_err(map_json)?;
        sqlx::query(
            r#"
            INSERT INTO content (id, site_id, type_id, slug, locale, status, data,
                                 author_id, created_at, updated_at, published_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
        )
        .bind(c.id.to_string())
        .bind(c.site_id.to_string())
        .bind(c.type_id.to_string())
        .bind(&c.slug)
        .bind(c.locale.to_string())
        .bind(status_str(c.status))
        .bind(&data)
        .bind(c.author_id.map(|a| a.to_string()))
        .bind(c.created_at)
        .bind(c.updated_at)
        .bind(c.published_at)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(c)
    }

    async fn update(&self, id: ContentId, patch: ContentPatch) -> StorageResult<Content> {
        let mut current = ContentRepo::get(self, id)
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
        let data = serde_json::to_value(&current.data).map_err(map_json)?;
        sqlx::query(
            r#"
            UPDATE content SET slug = $2, status = $3, data = $4, updated_at = $5
            WHERE id = $1
            "#,
        )
        .bind(current.id.to_string())
        .bind(&current.slug)
        .bind(status_str(current.status))
        .bind(&data)
        .bind(current.updated_at)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(current)
    }

    async fn publish(&self, id: ContentId) -> StorageResult<Content> {
        let mut current = ContentRepo::get(self, id)
            .await?
            .ok_or(StorageError::NotFound)?;
        let now = time::OffsetDateTime::now_utc();
        current.status = Status::Published;
        current.published_at = Some(now);
        current.updated_at = now;
        sqlx::query(
            "UPDATE content SET status = 'published', published_at = $2, updated_at = $2 WHERE id = $1",
        )
        .bind(current.id.to_string())
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(current)
    }

    async fn delete(&self, id: ContentId) -> StorageResult<()> {
        sqlx::query("DELETE FROM content WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(map_sqlx)?;
        Ok(())
    }
}

// --- UserRepo ---

#[async_trait]
impl UserRepo for PgRepo {
    async fn get(&self, id: UserId) -> StorageResult<Option<User>> {
        let row = sqlx::query("SELECT * FROM users WHERE id = $1")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(map_sqlx)?;
        row.as_ref().map(row_to_user).transpose()
    }

    async fn by_email(&self, email: &str) -> StorageResult<Option<User>> {
        let row = sqlx::query("SELECT * FROM users WHERE LOWER(email) = LOWER($1)")
            .bind(email)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_sqlx)?;
        row.as_ref().map(row_to_user).transpose()
    }

    async fn list(&self) -> StorageResult<Vec<User>> {
        let rows = sqlx::query("SELECT * FROM users ORDER BY created_at ASC")
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx)?;
        rows.iter().map(row_to_user).collect()
    }

    async fn upsert(&self, user: User) -> StorageResult<User> {
        let roles: Vec<String> = user.roles.iter().map(RoleId::to_string).collect();
        sqlx::query(
            r#"
            INSERT INTO users (id, email, handle, display_name, password_hash, roles,
                               active, created_at, last_login)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (id) DO UPDATE SET
                email = EXCLUDED.email,
                handle = EXCLUDED.handle,
                display_name = EXCLUDED.display_name,
                password_hash = EXCLUDED.password_hash,
                roles = EXCLUDED.roles,
                active = EXCLUDED.active,
                last_login = EXCLUDED.last_login
            "#,
        )
        .bind(user.id.to_string())
        .bind(&user.email)
        .bind(&user.handle)
        .bind(&user.display_name)
        .bind(&user.password_hash)
        .bind(&roles)
        .bind(user.active)
        .bind(user.created_at)
        .bind(user.last_login)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(user)
    }

    async fn delete(&self, id: UserId) -> StorageResult<()> {
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(map_sqlx)?;
        Ok(())
    }

    async fn get_role(&self, id: RoleId) -> StorageResult<Option<Role>> {
        let row = sqlx::query("SELECT * FROM roles WHERE id = $1")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(map_sqlx)?;
        row.as_ref().map(row_to_role).transpose()
    }

    async fn list_roles(&self) -> StorageResult<Vec<Role>> {
        let rows = sqlx::query("SELECT * FROM roles ORDER BY name ASC")
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx)?;
        rows.iter().map(row_to_role).collect()
    }

    async fn upsert_role(&self, role: Role) -> StorageResult<Role> {
        let permissions = serde_json::to_value(&role.permissions).map_err(map_json)?;
        sqlx::query(
            r#"
            INSERT INTO roles (id, name, description, permissions)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                description = EXCLUDED.description,
                permissions = EXCLUDED.permissions
            "#,
        )
        .bind(role.id.to_string())
        .bind(&role.name)
        .bind(&role.description)
        .bind(&permissions)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(role)
    }
}

// --- MediaMetaRepo ---

#[async_trait]
impl MediaMetaRepo for PgRepo {
    async fn get(&self, id: MediaId) -> StorageResult<Option<Media>> {
        let row = sqlx::query("SELECT * FROM media WHERE id = $1")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(map_sqlx)?;
        row.as_ref().map(row_to_media).transpose()
    }

    async fn list(&self, site: SiteId) -> StorageResult<Vec<Media>> {
        let rows = sqlx::query("SELECT * FROM media WHERE site_id = $1 ORDER BY created_at DESC")
            .bind(site.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx)?;
        rows.iter().map(row_to_media).collect()
    }

    async fn create(&self, m: Media) -> StorageResult<Media> {
        sqlx::query(
            r#"
            INSERT INTO media (id, site_id, key, filename, mime, size, width, height,
                               alt, kind, uploaded_by, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            "#,
        )
        .bind(m.id.to_string())
        .bind(m.site_id.to_string())
        .bind(&m.key)
        .bind(&m.filename)
        .bind(&m.mime)
        .bind(m.size as i64)
        .bind(m.width.map(|w| w as i32))
        .bind(m.height.map(|h| h as i32))
        .bind(&m.alt)
        .bind(media_kind_str(m.kind))
        .bind(m.uploaded_by.map(|u| u.to_string()))
        .bind(m.created_at)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(m)
    }

    async fn delete(&self, id: MediaId) -> StorageResult<()> {
        sqlx::query("DELETE FROM media WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(map_sqlx)?;
        Ok(())
    }
}
