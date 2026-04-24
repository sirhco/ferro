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
