//! Flat-JSON backend. Single directory, one file per entity. Dev/demo use.
//!
//! Layout:
//!   <root>/sites/<id>.json
//!   <root>/types/<id>.json
//!   <root>/content/<id>.json
//!   <root>/users/<id>.json
//!   <root>/roles/<id>.json
//!   <root>/media/<id>.json

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use ferro_core::*;
use tokio::fs;
use tokio::sync::RwLock;

use crate::config::StorageConfig;
use crate::error::{StorageError, StorageResult};
use crate::repo::*;

pub async fn connect(cfg: &StorageConfig) -> StorageResult<Box<dyn Repository>> {
    let StorageConfig::FsJson { path } = cfg else {
        unreachable!();
    };
    for sub in ["sites", "types", "content", "users", "roles", "media"] {
        fs::create_dir_all(path.join(sub)).await?;
    }
    Ok(Box::new(FsJsonRepo { root: path.clone(), _guard: RwLock::new(()) }))
}

pub struct FsJsonRepo {
    root: PathBuf,
    _guard: RwLock<()>,
}

impl std::fmt::Debug for FsJsonRepo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FsJsonRepo").field("root", &self.root).finish()
    }
}

impl FsJsonRepo {
    fn path(&self, sub: &str, id: &str) -> PathBuf {
        self.root.join(sub).join(format!("{id}.json"))
    }

    async fn read<T: serde::de::DeserializeOwned>(path: &Path) -> StorageResult<Option<T>> {
        match fs::read(path).await {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|e| StorageError::Serde(e.to_string())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(StorageError::Io(e)),
        }
    }

    async fn write<T: serde::Serialize>(path: &Path, v: &T) -> StorageResult<()> {
        let bytes = serde_json::to_vec_pretty(v).map_err(|e| StorageError::Serde(e.to_string()))?;
        fs::write(path, bytes).await?;
        Ok(())
    }
}

#[async_trait]
impl Repository for FsJsonRepo {
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
        Ok(())
    }

    async fn health(&self) -> StorageResult<()> {
        if self.root.exists() {
            Ok(())
        } else {
            Err(StorageError::Backend(format!("missing root {}", self.root.display())))
        }
    }
}

#[async_trait]
impl SiteRepo for FsJsonRepo {
    async fn get(&self, id: SiteId) -> StorageResult<Option<Site>> {
        Self::read(&self.path("sites", &id.to_string())).await
    }
    async fn by_slug(&self, slug: &str) -> StorageResult<Option<Site>> {
        let mut dir = fs::read_dir(self.root.join("sites")).await?;
        while let Some(entry) = dir.next_entry().await? {
            if let Some(site) = Self::read::<Site>(&entry.path()).await? {
                if site.slug == slug {
                    return Ok(Some(site));
                }
            }
        }
        Ok(None)
    }
    async fn list(&self) -> StorageResult<Vec<Site>> {
        let mut out = Vec::new();
        let mut dir = fs::read_dir(self.root.join("sites")).await?;
        while let Some(entry) = dir.next_entry().await? {
            if let Some(s) = Self::read::<Site>(&entry.path()).await? {
                out.push(s);
            }
        }
        Ok(out)
    }
    async fn upsert(&self, site: Site) -> StorageResult<Site> {
        Self::write(&self.path("sites", &site.id.to_string()), &site).await?;
        Ok(site)
    }
    async fn delete(&self, id: SiteId) -> StorageResult<()> {
        let p = self.path("sites", &id.to_string());
        if p.exists() {
            fs::remove_file(p).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl ContentTypeRepo for FsJsonRepo {
    async fn get(&self, id: ContentTypeId) -> StorageResult<Option<ContentType>> {
        Self::read(&self.path("types", &id.to_string())).await
    }
    async fn by_slug(&self, site: SiteId, slug: &str) -> StorageResult<Option<ContentType>> {
        for t in ContentTypeRepo::list(self, site).await? {
            if t.slug == slug {
                return Ok(Some(t));
            }
        }
        Ok(None)
    }
    async fn list(&self, site: SiteId) -> StorageResult<Vec<ContentType>> {
        let mut out = Vec::new();
        let mut dir = fs::read_dir(self.root.join("types")).await?;
        while let Some(entry) = dir.next_entry().await? {
            if let Some(t) = Self::read::<ContentType>(&entry.path()).await? {
                if t.site_id == site {
                    out.push(t);
                }
            }
        }
        Ok(out)
    }
    async fn upsert(&self, ty: ContentType) -> StorageResult<ContentType> {
        Self::write(&self.path("types", &ty.id.to_string()), &ty).await?;
        Ok(ty)
    }
    async fn delete(&self, id: ContentTypeId) -> StorageResult<()> {
        let p = self.path("types", &id.to_string());
        if p.exists() {
            fs::remove_file(p).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl ContentRepo for FsJsonRepo {
    async fn get(&self, id: ContentId) -> StorageResult<Option<Content>> {
        Self::read(&self.path("content", &id.to_string())).await
    }
    async fn by_slug(
        &self,
        site: SiteId,
        ty: ContentTypeId,
        slug: &str,
    ) -> StorageResult<Option<Content>> {
        let mut dir = fs::read_dir(self.root.join("content")).await?;
        while let Some(entry) = dir.next_entry().await? {
            if let Some(c) = Self::read::<Content>(&entry.path()).await? {
                if c.site_id == site && c.type_id == ty && c.slug == slug {
                    return Ok(Some(c));
                }
            }
        }
        Ok(None)
    }
    async fn list(&self, q: ContentQuery) -> StorageResult<Page<Content>> {
        let mut dir = fs::read_dir(self.root.join("content")).await?;
        let mut items = Vec::new();
        while let Some(entry) = dir.next_entry().await? {
            if let Some(c) = Self::read::<Content>(&entry.path()).await? {
                if q.site_id.is_some_and(|s| s != c.site_id) {
                    continue;
                }
                if q.type_id.is_some_and(|t| t != c.type_id) {
                    continue;
                }
                if q.status.is_some_and(|s| s != c.status) {
                    continue;
                }
                if q.locale.as_ref().is_some_and(|l| l != &c.locale) {
                    continue;
                }
                items.push(c);
            }
        }
        let total = items.len() as u64;
        let page = q.page.unwrap_or(1).max(1);
        let per_page = q.per_page.unwrap_or(20).max(1);
        let start = ((page - 1) * per_page) as usize;
        let end = (start + per_page as usize).min(items.len());
        let items = if start < items.len() { items[start..end].to_vec() } else { Vec::new() };
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
        Self::write(&self.path("content", &c.id.to_string()), &c).await?;
        Ok(c)
    }
    async fn update(&self, id: ContentId, patch: ContentPatch) -> StorageResult<Content> {
        let path = self.path("content", &id.to_string());
        let mut c: Content = Self::read(&path).await?.ok_or(StorageError::NotFound)?;
        if let Some(slug) = patch.slug {
            c.slug = slug;
        }
        if let Some(status) = patch.status {
            c.status = status;
        }
        if let Some(data) = patch.data {
            c.data = data;
        }
        c.updated_at = time::OffsetDateTime::now_utc();
        Self::write(&path, &c).await?;
        Ok(c)
    }
    async fn publish(&self, id: ContentId) -> StorageResult<Content> {
        let path = self.path("content", &id.to_string());
        let mut c: Content = Self::read(&path).await?.ok_or(StorageError::NotFound)?;
        c.status = Status::Published;
        c.published_at = Some(time::OffsetDateTime::now_utc());
        c.updated_at = c.published_at.unwrap();
        Self::write(&path, &c).await?;
        Ok(c)
    }
    async fn delete(&self, id: ContentId) -> StorageResult<()> {
        let p = self.path("content", &id.to_string());
        if p.exists() {
            fs::remove_file(p).await?;
        }
        Ok(())
    }

    async fn upsert(&self, content: Content) -> StorageResult<Content> {
        Self::write(&self.path("content", &content.id.to_string()), &content).await?;
        Ok(content)
    }
}

#[async_trait]
impl UserRepo for FsJsonRepo {
    async fn get(&self, id: UserId) -> StorageResult<Option<User>> {
        Self::read(&self.path("users", &id.to_string())).await
    }
    async fn by_email(&self, email: &str) -> StorageResult<Option<User>> {
        let mut dir = fs::read_dir(self.root.join("users")).await?;
        while let Some(entry) = dir.next_entry().await? {
            if let Some(u) = Self::read::<User>(&entry.path()).await? {
                if u.email.eq_ignore_ascii_case(email) {
                    return Ok(Some(u));
                }
            }
        }
        Ok(None)
    }
    async fn list(&self) -> StorageResult<Vec<User>> {
        let mut out = Vec::new();
        let mut dir = fs::read_dir(self.root.join("users")).await?;
        while let Some(entry) = dir.next_entry().await? {
            if let Some(u) = Self::read::<User>(&entry.path()).await? {
                out.push(u);
            }
        }
        Ok(out)
    }
    async fn upsert(&self, user: User) -> StorageResult<User> {
        Self::write(&self.path("users", &user.id.to_string()), &user).await?;
        Ok(user)
    }
    async fn delete(&self, id: UserId) -> StorageResult<()> {
        let p = self.path("users", &id.to_string());
        if p.exists() {
            fs::remove_file(p).await?;
        }
        Ok(())
    }
    async fn get_role(&self, id: RoleId) -> StorageResult<Option<Role>> {
        Self::read(&self.path("roles", &id.to_string())).await
    }
    async fn list_roles(&self) -> StorageResult<Vec<Role>> {
        let mut out = Vec::new();
        let mut dir = fs::read_dir(self.root.join("roles")).await?;
        while let Some(entry) = dir.next_entry().await? {
            if let Some(r) = Self::read::<Role>(&entry.path()).await? {
                out.push(r);
            }
        }
        Ok(out)
    }
    async fn upsert_role(&self, role: Role) -> StorageResult<Role> {
        Self::write(&self.path("roles", &role.id.to_string()), &role).await?;
        Ok(role)
    }
    async fn delete_role(&self, id: RoleId) -> StorageResult<()> {
        let p = self.path("roles", &id.to_string());
        if p.exists() {
            fs::remove_file(p).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl MediaMetaRepo for FsJsonRepo {
    async fn get(&self, id: MediaId) -> StorageResult<Option<Media>> {
        Self::read(&self.path("media", &id.to_string())).await
    }
    async fn list(&self, site: SiteId) -> StorageResult<Vec<Media>> {
        let mut out = Vec::new();
        let mut dir = fs::read_dir(self.root.join("media")).await?;
        while let Some(entry) = dir.next_entry().await? {
            if let Some(m) = Self::read::<Media>(&entry.path()).await? {
                if m.site_id == site {
                    out.push(m);
                }
            }
        }
        Ok(out)
    }
    async fn create(&self, m: Media) -> StorageResult<Media> {
        Self::write(&self.path("media", &m.id.to_string()), &m).await?;
        Ok(m)
    }
    async fn delete(&self, id: MediaId) -> StorageResult<()> {
        let p = self.path("media", &id.to_string());
        if p.exists() {
            fs::remove_file(p).await?;
        }
        Ok(())
    }
    async fn upsert(&self, m: Media) -> StorageResult<Media> {
        Self::write(&self.path("media", &m.id.to_string()), &m).await?;
        Ok(m)
    }
}
