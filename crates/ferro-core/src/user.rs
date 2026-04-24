use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use zeroize_hack::ZeroizeHash;

mod zeroize_hack {
    /// Marker type for hashes that should be zeroized when dropped.
    /// Actual zeroize wiring lives in `ferro-auth`; here we just model the shape.
    pub type ZeroizeHash = String;
}

use crate::id::{RoleId, UserId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub email: String,
    pub handle: String,
    pub display_name: Option<String>,
    #[serde(skip_serializing)]
    pub password_hash: Option<ZeroizeHash>,
    pub roles: Vec<RoleId>,
    #[serde(default)]
    pub active: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option", default)]
    pub last_login: Option<OffsetDateTime>,
}

impl User {
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.active
    }
}
