//! Flat-Markdown backend. Git-friendly content storage.
//!
//! Layout:
//!     <root>/_meta/sites/<site-id>.json
//!     <root>/_meta/types/<type-id>.json
//!     <root>/_meta/users/<user-id>.json
//!     <root>/_meta/roles/<role-id>.json
//!     <root>/_meta/media/<media-id>.json
//!     <root>/<site-slug>/<type-slug>/<content-slug>.<locale>.md
//!
//! Each markdown file carries YAML front-matter holding the full `Content`
//! struct (minus an extracted `body` field, which becomes the file body when a
//! `body` string field is present in `data`). On read we fold the body back
//! into `data["body"]` if it's missing — keeps the on-disk shape ergonomic
//! for human editors while the API still sees a complete `Content` record.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use ferro_core::*;
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::sync::RwLock;

use crate::config::StorageConfig;
use crate::error::{StorageError, StorageResult};
use crate::repo::*;

pub async fn connect(cfg: &StorageConfig) -> StorageResult<Box<dyn Repository>> {
    let StorageConfig::FsMarkdown { path } = cfg else {
        unreachable!();
    };
    for sub in ["sites", "types", "users", "roles", "media"] {
        fs::create_dir_all(path.join("_meta").join(sub)).await?;
    }
    Ok(Box::new(FsMarkdownRepo { root: path.clone(), _guard: RwLock::new(()) }))
}

pub struct FsMarkdownRepo {
    pub(crate) root: PathBuf,
    _guard: RwLock<()>,
}

impl std::fmt::Debug for FsMarkdownRepo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FsMarkdownRepo").field("root", &self.root).finish()
    }
}

impl FsMarkdownRepo {
    fn meta_path(&self, table: &str, id: &str) -> PathBuf {
        self.root.join("_meta").join(table).join(format!("{id}.json"))
    }

    async fn read_json<T: serde::de::DeserializeOwned>(p: &Path) -> StorageResult<Option<T>> {
        match fs::read(p).await {
            Ok(b) => serde_json::from_slice(&b)
                .map(Some)
                .map_err(|e| StorageError::Serde(e.to_string())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(StorageError::Io(e)),
        }
    }

    async fn write_json<T: serde::Serialize>(p: &Path, v: &T) -> StorageResult<()> {
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).await?;
        }
        let bytes = serde_json::to_vec_pretty(v).map_err(|e| StorageError::Serde(e.to_string()))?;
        fs::write(p, bytes).await?;
        Ok(())
    }

    async fn list_meta<T: serde::de::DeserializeOwned>(
        &self,
        table: &str,
    ) -> StorageResult<Vec<T>> {
        let dir = self.root.join("_meta").join(table);
        let mut out = Vec::new();
        let mut rd = fs::read_dir(&dir).await?;
        while let Some(entry) = rd.next_entry().await? {
            if let Some(v) = Self::read_json::<T>(&entry.path()).await? {
                out.push(v);
            }
        }
        Ok(out)
    }

    /// Resolve `(site-slug, type-slug)` from a `Content` so we can build its
    /// markdown path. Returns `Err(NotFound)` if either lookup misses.
    async fn slugs_for(
        &self,
        site_id: SiteId,
        type_id: ContentTypeId,
    ) -> StorageResult<(String, String)> {
        let site: Site = Self::read_json(&self.meta_path("sites", &site_id.to_string()))
            .await?
            .ok_or(StorageError::NotFound)?;
        let ty: ContentType = Self::read_json(&self.meta_path("types", &type_id.to_string()))
            .await?
            .ok_or(StorageError::NotFound)?;
        Ok((site.slug, ty.slug))
    }

    fn content_path(&self, site_slug: &str, type_slug: &str, slug: &str, locale: &str) -> PathBuf {
        self.root
            .join(site_slug)
            .join(type_slug)
            .join(format!("{slug}.{locale}.md"))
    }

    async fn write_content(&self, c: &Content) -> StorageResult<()> {
        let (site_slug, type_slug) = self.slugs_for(c.site_id, c.type_id).await?;
        let p = self.content_path(&site_slug, &type_slug, &c.slug, c.locale.as_str());
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Pull the `body` string out of `data` so it lives in the file body.
        let mut header = c.clone();
        let body_text = match header.data.get("body").cloned() {
            Some(FieldValue::String(s)) => {
                header.data.remove("body");
                s
            }
            _ => String::new(),
        };
        let header_yaml = serde_yaml::to_string(&header)
            .map_err(|e| StorageError::Serde(e.to_string()))?;
        let mut file = String::with_capacity(header_yaml.len() + body_text.len() + 16);
        file.push_str("---\n");
        file.push_str(&header_yaml);
        if !file.ends_with('\n') {
            file.push('\n');
        }
        file.push_str("---\n");
        file.push_str(&body_text);
        fs::write(&p, file).await?;
        Ok(())
    }

    async fn read_content_file(p: &Path) -> StorageResult<Option<Content>> {
        let text = match fs::read_to_string(p).await {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(StorageError::Io(e)),
        };
        let (header, body) = split_front_matter(&text)
            .ok_or_else(|| StorageError::Backend("missing front-matter".into()))?;
        let mut content: Content = serde_yaml::from_str(header)
            .map_err(|e| StorageError::Serde(e.to_string()))?;
        if !body.trim().is_empty() && !content.data.contains_key("body") {
            content
                .data
                .insert("body".into(), FieldValue::String(body.to_string()));
        }
        Ok(Some(content))
    }

    async fn walk_content<F: FnMut(Content) -> bool>(
        &self,
        site_filter: Option<SiteId>,
        type_filter: Option<ContentTypeId>,
        mut keep: F,
    ) -> StorageResult<Vec<Content>> {
        let mut out = Vec::new();
        let sites = self.list_meta::<Site>("sites").await?;
        for site in sites {
            if let Some(filter) = site_filter {
                if site.id != filter {
                    continue;
                }
            }
            let site_dir = self.root.join(&site.slug);
            let Ok(mut sd) = fs::read_dir(&site_dir).await else {
                continue;
            };
            while let Some(type_entry) = sd.next_entry().await? {
                if !type_entry.file_type().await?.is_dir() {
                    continue;
                }
                let mut td = fs::read_dir(type_entry.path()).await?;
                while let Some(file_entry) = td.next_entry().await? {
                    if !file_entry.file_type().await?.is_file() {
                        continue;
                    }
                    if let Some(content) = Self::read_content_file(&file_entry.path()).await? {
                        if let Some(filter) = type_filter {
                            if content.type_id != filter {
                                continue;
                            }
                        }
                        if keep(content.clone()) {
                            out.push(content);
                        }
                    }
                }
            }
        }
        Ok(out)
    }
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
        if self.root.exists() {
            Ok(())
        } else {
            Err(StorageError::Backend(format!("missing root {}", self.root.display())))
        }
    }
}

#[async_trait]
impl SiteRepo for FsMarkdownRepo {
    async fn get(&self, id: SiteId) -> StorageResult<Option<Site>> {
        Self::read_json(&self.meta_path("sites", &id.to_string())).await
    }
    async fn by_slug(&self, slug: &str) -> StorageResult<Option<Site>> {
        for s in self.list_meta::<Site>("sites").await? {
            if s.slug == slug {
                return Ok(Some(s));
            }
        }
        Ok(None)
    }
    async fn list(&self) -> StorageResult<Vec<Site>> {
        self.list_meta("sites").await
    }
    async fn upsert(&self, site: Site) -> StorageResult<Site> {
        Self::write_json(&self.meta_path("sites", &site.id.to_string()), &site).await?;
        Ok(site)
    }
    async fn delete(&self, id: SiteId) -> StorageResult<()> {
        let p = self.meta_path("sites", &id.to_string());
        if p.exists() {
            fs::remove_file(p).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl ContentTypeRepo for FsMarkdownRepo {
    async fn get(&self, id: ContentTypeId) -> StorageResult<Option<ContentType>> {
        Self::read_json(&self.meta_path("types", &id.to_string())).await
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
        let all: Vec<ContentType> = self.list_meta("types").await?;
        Ok(all.into_iter().filter(|t| t.site_id == site).collect())
    }
    async fn upsert(&self, ty: ContentType) -> StorageResult<ContentType> {
        Self::write_json(&self.meta_path("types", &ty.id.to_string()), &ty).await?;
        Ok(ty)
    }
    async fn delete(&self, id: ContentTypeId) -> StorageResult<()> {
        let p = self.meta_path("types", &id.to_string());
        if p.exists() {
            fs::remove_file(p).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl ContentRepo for FsMarkdownRepo {
    async fn get(&self, id: ContentId) -> StorageResult<Option<Content>> {
        let mut found: Option<Content> = None;
        let _ = self
            .walk_content(None, None, |c| {
                if c.id == id {
                    found = Some(c);
                    false
                } else {
                    false
                }
            })
            .await?;
        Ok(found)
    }
    async fn by_slug(
        &self,
        site: SiteId,
        ty: ContentTypeId,
        slug: &str,
    ) -> StorageResult<Option<Content>> {
        let (site_slug, type_slug) = self.slugs_for(site, ty).await?;
        let dir = self.root.join(&site_slug).join(&type_slug);
        let Ok(mut rd) = fs::read_dir(&dir).await else {
            return Ok(None);
        };
        let prefix = format!("{slug}.");
        while let Some(entry) = rd.next_entry().await? {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(&prefix) && name.ends_with(".md") {
                if let Some(c) = Self::read_content_file(&entry.path()).await? {
                    if c.slug == slug {
                        return Ok(Some(c));
                    }
                }
            }
        }
        Ok(None)
    }
    async fn list(&self, q: ContentQuery) -> StorageResult<Page<Content>> {
        let site = q.site_id;
        let ty = q.type_id;
        let needle = q.search.as_deref().map(|s| s.to_lowercase());
        let status = q.status;
        let locale = q.locale.clone();
        let mut items = self
            .walk_content(site, ty, move |c| {
                if let Some(s) = status {
                    if s != c.status {
                        return false;
                    }
                }
                if let Some(l) = locale.as_ref() {
                    if l != &c.locale {
                        return false;
                    }
                }
                if let Some(n) = needle.as_deref() {
                    let hay = serde_json::to_string(&c.data).unwrap_or_default().to_lowercase();
                    if !c.slug.to_lowercase().contains(n) && !hay.contains(n) {
                        return false;
                    }
                }
                true
            })
            .await?;
        items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        let total = items.len() as u64;
        let page = q.page.unwrap_or(1).max(1);
        let per_page = q.per_page.unwrap_or(20).max(1);
        let start = ((page - 1) * per_page) as usize;
        let end = (start + per_page as usize).min(items.len());
        let items = if start < items.len() {
            items[start..end].to_vec()
        } else {
            Vec::new()
        };
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
        self.write_content(&c).await?;
        Ok(c)
    }
    async fn update(&self, id: ContentId, patch: ContentPatch) -> StorageResult<Content> {
        let mut current = ContentRepo::get(self, id)
            .await?
            .ok_or(StorageError::NotFound)?;
        // Path may move if slug changes.
        let (site_slug, type_slug) = self.slugs_for(current.site_id, current.type_id).await?;
        let old_path = self.content_path(&site_slug, &type_slug, &current.slug, current.locale.as_str());
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
        if old_path.exists() {
            let _ = fs::remove_file(&old_path).await;
        }
        self.write_content(&current).await?;
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
        self.write_content(&current).await?;
        Ok(current)
    }
    async fn delete(&self, id: ContentId) -> StorageResult<()> {
        if let Some(c) = ContentRepo::get(self, id).await? {
            let (site_slug, type_slug) = self.slugs_for(c.site_id, c.type_id).await?;
            let p = self.content_path(&site_slug, &type_slug, &c.slug, c.locale.as_str());
            if p.exists() {
                fs::remove_file(p).await?;
            }
        }
        Ok(())
    }
    async fn upsert(&self, content: Content) -> StorageResult<Content> {
        self.write_content(&content).await?;
        Ok(content)
    }
}

#[async_trait]
impl UserRepo for FsMarkdownRepo {
    async fn get(&self, id: UserId) -> StorageResult<Option<User>> {
        Self::read_json(&self.meta_path("users", &id.to_string())).await
    }
    async fn by_email(&self, email: &str) -> StorageResult<Option<User>> {
        for u in self.list_meta::<User>("users").await? {
            if u.email.eq_ignore_ascii_case(email) {
                return Ok(Some(u));
            }
        }
        Ok(None)
    }
    async fn list(&self) -> StorageResult<Vec<User>> {
        self.list_meta("users").await
    }
    async fn upsert(&self, user: User) -> StorageResult<User> {
        Self::write_json(&self.meta_path("users", &user.id.to_string()), &user).await?;
        Ok(user)
    }
    async fn delete(&self, id: UserId) -> StorageResult<()> {
        let p = self.meta_path("users", &id.to_string());
        if p.exists() {
            fs::remove_file(p).await?;
        }
        Ok(())
    }
    async fn get_role(&self, id: RoleId) -> StorageResult<Option<Role>> {
        Self::read_json(&self.meta_path("roles", &id.to_string())).await
    }
    async fn list_roles(&self) -> StorageResult<Vec<Role>> {
        self.list_meta("roles").await
    }
    async fn upsert_role(&self, role: Role) -> StorageResult<Role> {
        Self::write_json(&self.meta_path("roles", &role.id.to_string()), &role).await?;
        Ok(role)
    }
    async fn delete_role(&self, id: RoleId) -> StorageResult<()> {
        let p = self.meta_path("roles", &id.to_string());
        if p.exists() {
            fs::remove_file(p).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl MediaMetaRepo for FsMarkdownRepo {
    async fn get(&self, id: MediaId) -> StorageResult<Option<Media>> {
        Self::read_json(&self.meta_path("media", &id.to_string())).await
    }
    async fn list(&self, site: SiteId) -> StorageResult<Vec<Media>> {
        let all: Vec<Media> = self.list_meta("media").await?;
        Ok(all.into_iter().filter(|m| m.site_id == site).collect())
    }
    async fn create(&self, m: Media) -> StorageResult<Media> {
        Self::write_json(&self.meta_path("media", &m.id.to_string()), &m).await?;
        Ok(m)
    }
    async fn delete(&self, id: MediaId) -> StorageResult<()> {
        let p = self.meta_path("media", &id.to_string());
        if p.exists() {
            fs::remove_file(p).await?;
        }
        Ok(())
    }
    async fn upsert(&self, m: Media) -> StorageResult<Media> {
        Self::write_json(&self.meta_path("media", &m.id.to_string()), &m).await?;
        Ok(m)
    }
}

/// Pull the YAML front-matter out of `text`, returning `(header, body)`.
/// Accepts the conventional `---\n…\n---\n` opener used by static-site
/// generators. Returns `None` if the marker is missing.
fn split_front_matter(text: &str) -> Option<(&str, &str)> {
    let stripped = text.strip_prefix("---")?;
    let stripped = stripped.strip_prefix('\n').unwrap_or(stripped);
    let end = stripped.find("\n---")?;
    let header = &stripped[..end];
    let after = &stripped[end + 4..];
    let body = after.strip_prefix('\n').unwrap_or(after);
    Some((header, body))
}

#[allow(unused_imports)]
use std::collections::BTreeMap as _Used;
#[allow(dead_code)]
fn _silence(_: BTreeMap<String, FieldValue>) {}
