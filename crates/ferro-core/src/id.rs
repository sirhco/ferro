//! Typed identifiers. ULID under the hood — lexicographic sort = creation order.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use ulid::Ulid;

macro_rules! typed_id {
    ($name:ident, $prefix:literal) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Ulid);

        impl $name {
            #[must_use]
            pub fn new() -> Self {
                Self(Ulid::new())
            }

            #[must_use]
            pub fn prefix() -> &'static str {
                $prefix
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}_{}", $prefix, self.0)
            }
        }

        impl FromStr for $name {
            type Err = crate::error::CoreError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let body = s.strip_prefix(concat!($prefix, "_")).unwrap_or(s);
                Ulid::from_str(body)
                    .map(Self)
                    .map_err(|e| crate::error::CoreError::InvalidId(e.to_string()))
            }
        }
    };
}

typed_id!(SiteId, "site");
typed_id!(ContentTypeId, "type");
typed_id!(ContentId, "entry");
typed_id!(UserId, "user");
typed_id!(RoleId, "role");
typed_id!(MediaId, "media");
typed_id!(FieldId, "field");
