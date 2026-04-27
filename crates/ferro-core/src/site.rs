use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use url::Url;

use crate::{id::SiteId, locale::Locale};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Site {
    pub id: SiteId,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub primary_url: Option<Url>,
    pub locales: Vec<Locale>,
    pub default_locale: Locale,
    pub settings: SiteSettings,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SiteSettings {
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub preview_secret: Option<String>,
    #[serde(default)]
    pub allow_public_signup: bool,
    #[serde(default)]
    pub extras: serde_json::Value,
}
