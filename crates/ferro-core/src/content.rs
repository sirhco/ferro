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
