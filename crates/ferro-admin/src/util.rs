//! Tiny helpers that don't belong to any single route.

use leptos::prelude::*;

/// Navigate to `path` using the leptos_router history API. Falls back to a
/// full-page `location.assign` when called outside the router context (e.g.
/// before App mounts).
pub fn navigate_to(path: &str) {
    #[cfg(feature = "hydrate")]
    {
        if let Some(window) = web_sys::window() {
            let _ = window.location().set_href(path);
        }
    }
    #[cfg(not(feature = "hydrate"))]
    {
        let _ = path;
    }
}

/// Read a URL-decoded route param, returning an empty string when absent.
pub fn param(map: &leptos_router::params::ParamsMap, key: &str) -> String {
    map.get(key).unwrap_or_default()
}

/// Format an RFC3339 string for human display: trims fractional seconds,
/// drops the trailing `Z`, and joins date+time with a space. Returns the
/// input verbatim if it doesn't parse as RFC3339-shaped.
pub fn format_dt(s: &str) -> String {
    let trimmed = s.trim_end_matches('Z');
    let mut parts = trimmed.splitn(2, 'T');
    match (parts.next(), parts.next()) {
        (Some(date), Some(time)) => {
            let time_short = time.split('.').next().unwrap_or(time);
            format!("{date} {time_short}")
        }
        _ => s.to_string(),
    }
}
