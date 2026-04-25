use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use zeroize_hack::ZeroizeHash;

mod zeroize_hack {
    /// Marker type for hashes that should be zeroized when dropped.
    /// Actual zeroize wiring lives in `ferro-auth`; here we just model the shape.
    pub(super) type ZeroizeHash = String;
}

use crate::id::{RoleId, UserId};

/// Persisted user record. Round-trips through serde with `password_hash`
/// included so storage backends can preserve the hash. **API layers must call
/// [`User::redact_secrets`] before returning a `User` over the wire** — there
/// is no automatic skip-on-serialize because doing so would silently strip the
/// hash on every storage write too.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub email: String,
    pub handle: String,
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password_hash: Option<ZeroizeHash>,
    pub roles: Vec<RoleId>,
    #[serde(default)]
    pub active: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option", default)]
    pub last_login: Option<OffsetDateTime>,
    /// Last time the password (or any other event that should invalidate
    /// outstanding JWTs) changed. The auth middleware rejects tokens whose
    /// `iat` precedes this timestamp, providing logout-all-sessions semantics
    /// without a stateful denylist.
    #[serde(with = "time::serde::rfc3339::option", default)]
    pub password_changed_at: Option<OffsetDateTime>,
}

impl User {
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Drop secrets that must never leave the API boundary (currently just
    /// the password hash). Call this on every code path that returns a
    /// `User` to a client — REST handlers, GraphQL resolvers, export bundles.
    pub fn redact_secrets(&mut self) {
        self.password_hash = None;
    }

    /// Convenience: return a redacted clone.
    #[must_use]
    pub fn redacted(mut self) -> Self {
        self.redact_secrets();
        self
    }
}
