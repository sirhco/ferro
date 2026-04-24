use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::id::{MediaId, SiteId, UserId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Media {
    pub id: MediaId,
    pub site_id: SiteId,
    pub key: String,
    pub filename: String,
    pub mime: String,
    pub size: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub alt: Option<String>,
    pub kind: MediaKind,
    pub uploaded_by: Option<UserId>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Image,
    Video,
    Audio,
    Document,
    Other,
}

impl MediaKind {
    #[must_use]
    pub fn from_mime(mime: &str) -> Self {
        let top = mime.split('/').next().unwrap_or("");
        match top {
            "image" => Self::Image,
            "video" => Self::Video,
            "audio" => Self::Audio,
            "application" | "text" => Self::Document,
            _ => Self::Other,
        }
    }
}
