//! Ferro core domain model.
//!
//! Pure data + validation. No storage, no HTTP, no UI. Everything that talks
//! about "what content is" lives here so every other crate depends on a single
//! source of truth.

#![deny(rust_2018_idioms, unreachable_pub)]
#![warn(missing_debug_implementations, clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

pub mod content;
pub mod error;
pub mod field;
pub mod id;
pub mod locale;
pub mod media;
pub mod query;
pub mod role;
pub mod site;
pub mod user;
pub mod validation;

pub use content::{Content, ContentPatch, ContentType, ContentVersion, FieldChange, NewContent, Status};
pub use error::{CoreError, CoreResult};
pub use field::{FieldDef, FieldKind, FieldValue, RichFormat};
pub use id::{ContentId, ContentTypeId, ContentVersionId, FieldId, MediaId, RoleId, SiteId, UserId};
pub use locale::Locale;
pub use media::{Media, MediaKind};
pub use query::{ContentQuery, Order, Page, SortDir};
pub use role::{Permission, Role, Scope};
pub use site::{Site, SiteSettings};
pub use user::User;
