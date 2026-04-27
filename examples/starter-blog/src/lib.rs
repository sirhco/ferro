//! Minimal Ferro example: a blog with `Post` and `Author` content types
//! declared in Rust via `#[derive(ContentType)]`.
//!
//! In practice, schemas are usually authored via the admin UI's schema
//! builder and stored in the repo. This example shows the macro path for
//! teams that prefer a code-first workflow.

use ferro_macros::ContentType;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, ContentType)]
#[ferro(slug = "post", name = "Blog post")]
pub struct Post {
    pub title: String,
    pub slug: String,
    pub excerpt: Option<String>,
    pub body: String,
    pub cover_image_id: Option<String>,
    pub tags: Vec<String>,
    pub published_at: Option<time::OffsetDateTime>,
    pub author_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ContentType)]
#[ferro(slug = "author", name = "Author")]
pub struct Author {
    pub name: String,
    pub handle: String,
    pub bio: Option<String>,
    pub avatar_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ContentType)]
#[ferro(slug = "page", name = "Page")]
pub struct Page {
    pub title: String,
    pub slug: String,
    pub seo_description: Option<String>,
    pub blocks: serde_json::Value,
    pub cover_image_id: Option<String>,
    pub published_at: Option<time::OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ContentType)]
#[ferro(slug = "product", name = "Product")]
pub struct Product {
    pub name: String,
    pub slug: String,
    pub price_cents: i64,
    pub currency: String,
    pub blocks: serde_json::Value,
    pub gallery_ids: Vec<String>,
    pub in_stock: bool,
    pub published_at: Option<time::OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ContentType)]
#[ferro(slug = "event", name = "Event")]
pub struct Event {
    pub title: String,
    pub slug: String,
    pub starts_at: time::OffsetDateTime,
    pub ends_at: time::OffsetDateTime,
    pub venue: Option<String>,
    pub blocks: serde_json::Value,
    pub cover_image_id: Option<String>,
    pub published_at: Option<time::OffsetDateTime>,
}
