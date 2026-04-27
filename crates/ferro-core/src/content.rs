use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::{
    error::{CoreError, CoreResult},
    field::{FieldDef, FieldKind, FieldValue},
    id::{ContentId, ContentTypeId, SiteId, UserId},
    locale::Locale,
    validation::validate_slug,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentType {
    pub id: ContentTypeId,
    pub site_id: SiteId,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub fields: Vec<FieldDef>,
    #[serde(default)]
    pub singleton: bool,
    #[serde(default)]
    pub title_field: Option<String>,
    #[serde(default)]
    pub slug_field: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

impl ContentType {
    #[must_use]
    pub fn field(&self, slug: &str) -> Option<&FieldDef> {
        self.fields.iter().find(|f| f.slug == slug)
    }

    pub fn validate_data(&self, data: &BTreeMap<String, FieldValue>) -> CoreResult<()> {
        for def in &self.fields {
            let v = data.get(&def.slug).unwrap_or(&FieldValue::Null);
            v.validate_against(def)?;
        }
        for key in data.keys() {
            if self.field(key).is_none() {
                return Err(CoreError::UnknownField(key.clone()));
            }
        }
        Ok(())
    }

    /// Compare two versions of a `ContentType` and return the field-level
    /// changes needed to migrate existing content data from `old` to `new`.
    ///
    /// Field matching is keyed by `FieldDef.id` — same id + different slug is
    /// a `Rename`, same id + different kind is `KindChanged`, missing-in-new
    /// is `Removed`, missing-in-old is `Added`.
    #[must_use]
    pub fn diff(old: &ContentType, new: &ContentType) -> Vec<FieldChange> {
        use std::collections::HashMap;

        let mut out = Vec::new();
        let old_by_id: HashMap<_, _> = old.fields.iter().map(|f| (f.id, f)).collect();
        let new_by_id: HashMap<_, _> = new.fields.iter().map(|f| (f.id, f)).collect();

        for (id, nf) in &new_by_id {
            match old_by_id.get(id) {
                None => out.push(FieldChange::Added(nf.slug.clone())),
                Some(of) => {
                    if of.slug != nf.slug {
                        out.push(FieldChange::Renamed {
                            from: of.slug.clone(),
                            to: nf.slug.clone(),
                        });
                    }
                    if !field_kind_compatible(&of.kind, &nf.kind) {
                        out.push(FieldChange::KindChanged { slug: nf.slug.clone() });
                    }
                }
            }
        }
        for (id, of) in &old_by_id {
            if !new_by_id.contains_key(id) {
                out.push(FieldChange::Removed(of.slug.clone()));
            }
        }
        out
    }
}

/// A single field-level change between two `ContentType` versions. Consumed by
/// the storage-layer schema migrator to rewrite existing content data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum FieldChange {
    /// New field; existing rows get a `Null` until edited.
    Added(String),
    /// Field no longer in schema; drop value from content data.
    Removed(String),
    /// Field kept, slug changed. Move value across keys.
    Renamed { from: String, to: String },
    /// Field kind changed; value left as-is, validator will flag on next edit.
    KindChanged { slug: String },
}

/// Two `FieldKind`s are "compatible" (no migration needed) when they have the
/// same discriminant. Inner constraints (min/max, enum options, etc.) tighten
/// validation but don't break stored values.
fn field_kind_compatible(a: &FieldKind, b: &FieldKind) -> bool {
    use FieldKind as K;
    matches!(
        (a, b),
        (K::Text { .. }, K::Text { .. })
            | (K::RichText { .. }, K::RichText { .. })
            | (K::Number { .. }, K::Number { .. })
            | (K::Boolean, K::Boolean)
            | (K::Date, K::Date)
            | (K::DateTime, K::DateTime)
            | (K::Enum { .. }, K::Enum { .. })
            | (K::Reference { .. }, K::Reference { .. })
            | (K::Media { .. }, K::Media { .. })
            | (K::Json, K::Json)
            | (K::Slug { .. }, K::Slug { .. })
    )
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum Status {
    #[default]
    Draft,
    Published,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Content {
    pub id: ContentId,
    pub site_id: SiteId,
    pub type_id: ContentTypeId,
    pub slug: String,
    pub locale: Locale,
    pub status: Status,
    pub data: BTreeMap<String, FieldValue>,
    pub author_id: Option<UserId>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option", default)]
    pub published_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewContent {
    pub type_id: ContentTypeId,
    pub slug: String,
    pub locale: Locale,
    pub data: BTreeMap<String, FieldValue>,
    pub author_id: Option<UserId>,
}

impl NewContent {
    pub fn validate(&self, ty: &ContentType) -> CoreResult<()> {
        validate_slug(&self.slug)?;
        ty.validate_data(&self.data)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContentPatch {
    pub slug: Option<String>,
    pub status: Option<Status>,
    pub data: Option<BTreeMap<String, FieldValue>>,
}

/// Immutable snapshot of a content row at a given revision. Captured
/// automatically by the API layer before destructive mutations (update,
/// publish) so operators can list and restore prior states.
///
/// `parent_version` chains back through the history so callers can build a
/// timeline; for the root revision it's `None`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentVersion {
    pub id: crate::id::ContentVersionId,
    pub content_id: ContentId,
    pub site_id: SiteId,
    pub type_id: ContentTypeId,
    pub slug: String,
    pub locale: crate::locale::Locale,
    pub status: Status,
    pub data: BTreeMap<String, FieldValue>,
    pub author_id: Option<UserId>,
    #[serde(with = "time::serde::rfc3339")]
    pub captured_at: OffsetDateTime,
    pub parent_version: Option<crate::id::ContentVersionId>,
}

impl ContentVersion {
    /// Snapshot the current state of `content` for archival.
    #[must_use]
    pub fn from_content(
        content: &Content,
        captured_by: Option<UserId>,
        parent: Option<crate::id::ContentVersionId>,
    ) -> Self {
        Self {
            id: crate::id::ContentVersionId::new(),
            content_id: content.id,
            site_id: content.site_id,
            type_id: content.type_id,
            slug: content.slug.clone(),
            locale: content.locale.clone(),
            status: content.status,
            data: content.data.clone(),
            author_id: captured_by.or(content.author_id),
            captured_at: OffsetDateTime::now_utc(),
            parent_version: parent,
        }
    }
}

impl ContentPatch {
    /// Validate the patch against its target `ContentType`.
    ///
    /// Only fields supplied in the patch are checked. Required-but-missing
    /// fields are ignored here — the full-document invariant is the
    /// responsibility of the storage backend when the patch is merged.
    pub fn validate(&self, ty: &ContentType) -> CoreResult<()> {
        if let Some(slug) = &self.slug {
            validate_slug(slug)?;
        }
        let Some(data) = &self.data else {
            return Ok(());
        };
        for (key, value) in data {
            let def = ty.field(key).ok_or_else(|| CoreError::UnknownField(key.clone()))?;
            value.validate_against(def)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        field::{FieldDef, FieldKind},
        id::{ContentTypeId, FieldId, SiteId},
    };

    fn test_type() -> ContentType {
        let now = OffsetDateTime::now_utc();
        ContentType {
            id: ContentTypeId::new(),
            site_id: SiteId::new(),
            slug: "post".into(),
            name: "Post".into(),
            description: None,
            fields: vec![
                FieldDef {
                    id: FieldId::new(),
                    slug: "title".into(),
                    name: "Title".into(),
                    help: None,
                    kind: FieldKind::Text { multiline: false, max: Some(200) },
                    required: true,
                    localized: false,
                    unique: false,
                    hidden: false,
                },
                FieldDef {
                    id: FieldId::new(),
                    slug: "count".into(),
                    name: "Count".into(),
                    help: None,
                    kind: FieldKind::Number { int: true, min: Some(0.0), max: Some(10.0) },
                    required: false,
                    localized: false,
                    unique: false,
                    hidden: false,
                },
            ],
            singleton: false,
            title_field: None,
            slug_field: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn patch_validate_empty_ok() {
        let p = ContentPatch::default();
        assert!(p.validate(&test_type()).is_ok());
    }

    #[test]
    fn patch_validate_good_slug_and_data() {
        let mut data = BTreeMap::new();
        data.insert("title".into(), FieldValue::String("Hi".into()));
        data.insert("count".into(), FieldValue::Number(3.0));
        let p = ContentPatch { slug: Some("hello-world".into()), status: None, data: Some(data) };
        assert!(p.validate(&test_type()).is_ok());
    }

    #[test]
    fn patch_validate_rejects_unknown_field() {
        let mut data = BTreeMap::new();
        data.insert("nope".into(), FieldValue::String("x".into()));
        let p = ContentPatch { slug: None, status: None, data: Some(data) };
        assert!(matches!(p.validate(&test_type()), Err(CoreError::UnknownField(_))));
    }

    #[test]
    fn patch_validate_rejects_out_of_range_number() {
        let mut data = BTreeMap::new();
        data.insert("count".into(), FieldValue::Number(99.0));
        let p = ContentPatch { slug: None, status: None, data: Some(data) };
        assert!(p.validate(&test_type()).is_err());
    }

    #[test]
    fn patch_validate_rejects_bad_slug() {
        let p = ContentPatch { slug: Some("Bad Slug!".into()), status: None, data: None };
        assert!(p.validate(&test_type()).is_err());
    }

    #[test]
    fn diff_detects_added_field() {
        let old = test_type();
        let mut new = test_type();
        // Preserve the same ids so only the new field is treated as added.
        new.fields = old.fields.clone();
        new.fields.push(FieldDef {
            id: FieldId::new(),
            slug: "body".into(),
            name: "Body".into(),
            help: None,
            kind: FieldKind::RichText { format: crate::field::RichFormat::Markdown },
            required: false,
            localized: false,
            unique: false,
            hidden: false,
        });
        let changes = ContentType::diff(&old, &new);
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], FieldChange::Added(s) if s == "body"));
    }

    #[test]
    fn diff_detects_removed_field() {
        let old = test_type();
        let mut new = test_type();
        new.fields = old.fields.iter().take(1).cloned().collect();
        let changes = ContentType::diff(&old, &new);
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], FieldChange::Removed(s) if s == "count"));
    }

    #[test]
    fn diff_detects_rename() {
        let old = test_type();
        let mut new = old.clone();
        new.fields[0].slug = "headline".into();
        let changes = ContentType::diff(&old, &new);
        assert_eq!(changes.len(), 1);
        assert!(matches!(
            &changes[0],
            FieldChange::Renamed { from, to } if from == "title" && to == "headline"
        ));
    }

    #[test]
    fn diff_detects_kind_change() {
        let old = test_type();
        let mut new = old.clone();
        new.fields[1].kind = FieldKind::Text { multiline: false, max: None };
        let changes = ContentType::diff(&old, &new);
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], FieldChange::KindChanged { slug } if slug == "count"));
    }

    #[test]
    fn diff_ignores_inner_constraint_change() {
        let old = test_type();
        let mut new = old.clone();
        // Tightening bounds isn't a schema-migration event.
        new.fields[1].kind = FieldKind::Number { int: true, min: Some(1.0), max: Some(5.0) };
        let changes = ContentType::diff(&old, &new);
        assert!(changes.is_empty(), "got {changes:?}");
    }
}
