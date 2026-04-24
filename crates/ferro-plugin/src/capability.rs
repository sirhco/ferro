use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::PluginError;

/// Explicit, enumerable plugin capabilities. Nothing is ambient.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// Read content of any type. Refine per-type in future.
    ContentRead,
    /// Write (draft) content.
    ContentWrite,
    /// Publish content.
    ContentPublish,
    /// Emit log lines.
    Logs,
    /// Arbitrary HTTP fetches (outbound). Requires allowlist.
    HttpFetch { host: String },
    /// Serve HTTP under a path prefix (e.g. `/sitemap.xml`).
    HttpServe { prefix: String },
    /// Read media files.
    MediaRead,
    /// Write media files.
    MediaWrite,
}

impl FromStr for Capability {
    type Err = PluginError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Accept short forms like `content.read` or `http.serve:/sitemap.xml`.
        let (head, arg) = match s.split_once(':') {
            Some((h, a)) => (h, Some(a.to_string())),
            None => (s, None),
        };
        Ok(match (head, arg) {
            ("content.read", _) => Self::ContentRead,
            ("content.write", _) => Self::ContentWrite,
            ("content.publish", _) => Self::ContentPublish,
            ("logs", _) => Self::Logs,
            ("http.fetch", Some(host)) => Self::HttpFetch { host },
            ("http.serve", Some(prefix)) => Self::HttpServe { prefix },
            ("media.read", _) => Self::MediaRead,
            ("media.write", _) => Self::MediaWrite,
            _ => return Err(PluginError::Manifest(format!("unknown capability `{s}`"))),
        })
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ContentRead => f.write_str("content.read"),
            Self::ContentWrite => f.write_str("content.write"),
            Self::ContentPublish => f.write_str("content.publish"),
            Self::Logs => f.write_str("logs"),
            Self::HttpFetch { host } => write!(f, "http.fetch:{host}"),
            Self::HttpServe { prefix } => write!(f, "http.serve:{prefix}"),
            Self::MediaRead => f.write_str("media.read"),
            Self::MediaWrite => f.write_str("media.write"),
        }
    }
}
