use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::error::{CoreError, CoreResult};
use crate::field::{FieldDef, FieldValue};
use crate::id::{ContentId, ContentTypeId, SiteId, UserId};
use crate::locale::Locale;
use crate::validation::validate_slug;

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
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Draft,
    Published,
    Archived,
}

impl Default for Status {
    fn default() -> Self {
        Self::Draft
    }
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
            let def = ty
                .field(key)
                .ok_or_else(|| CoreError::UnknownField(key.clone()))?;
            value.validate_against(def)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::{FieldDef, FieldKind};
    use crate::id::{ContentTypeId, FieldId, SiteId};

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
        let p = ContentPatch {
            slug: Some("hello-world".into()),
            status: None,
            data: Some(data),
        };
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
        let p = ContentPatch {
            slug: Some("Bad Slug!".into()),
            status: None,
            data: None,
        };
        assert!(p.validate(&test_type()).is_err());
    }
}
