//! Double-submit CSRF protection for cookie-based sessions.
//!
//! Per DESIGN §11 we issue a `ferro_csrf` cookie that the browser must mirror
//! in an `X-CSRF-Token` header on mutating requests. The middleware enforces
//! that match for any cookie-bearing call. Bearer-token API clients are
//! exempt — `Authorization` is not auto-attached cross-site, so it can't be
//! forged via a CSRF flow.
//!
//! The mint endpoint (`GET /api/v1/auth/csrf`) returns `{ "token": "..." }`
//! and sets the cookie. SPAs read the JSON for the header value; classic
//! form-post flows can rely on the cookie alone since the same value is
//! reflected in both places.
//!
//! Tokens themselves are 256-bit hex strings minted by
//! [`ferro_auth::session::new_token`] — opaque, single-use is *not* required
//! (the per-session lifetime is fine for double-submit).

use axum::{
    extract::Request,
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

pub const COOKIE_NAME: &str = "ferro_csrf";
pub const HEADER_NAME: &str = "x-csrf-token";

/// Constant-time byte equality. Avoid `==` so token comparison doesn't leak
/// length-of-match timing.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn extract_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in raw.split(';') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix(&format!("{name}=")) {
            return Some(rest.to_string());
        }
    }
    None
}

fn has_bearer(headers: &HeaderMap) -> bool {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|v| {
            let trimmed = v.trim_start();
            trimmed.starts_with("Bearer ") || trimmed.starts_with("bearer ")
        })
        .unwrap_or(false)
}

/// Axum middleware enforcing the double-submit invariant on mutating requests
/// that carry a session cookie.
pub async fn enforce(req: Request, next: Next) -> Result<Response, StatusCode> {
    if matches!(req.method(), &Method::GET | &Method::HEAD | &Method::OPTIONS) {
        return Ok(next.run(req).await);
    }
    if has_bearer(req.headers()) {
        return Ok(next.run(req).await);
    }
    let Some(cookie_token) = extract_cookie(req.headers(), COOKIE_NAME) else {
        // No session cookie ⇒ no CSRF target. Pure unauthenticated POSTs (e.g.
        // login from a fresh browser) flow through.
        return Ok(next.run(req).await);
    };
    let header_token =
        req.headers().get(HEADER_NAME).and_then(|v| v.to_str().ok()).unwrap_or_default();
    if header_token.is_empty() || !ct_eq(header_token.as_bytes(), cookie_token.as_bytes()) {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(next.run(req).await)
}

#[derive(Debug, Serialize)]
pub(crate) struct TokenResponse {
    pub token: String,
}

/// `GET /api/v1/auth/csrf` — mint a fresh token, set the cookie, echo the
/// value in the JSON body so SPAs can stash it in a non-cookie store and
/// supply it via `X-CSRF-Token`.
pub async fn mint() -> Response {
    let token = ferro_auth::session::new_token();
    let cookie = format!("{COOKIE_NAME}={token}; Path=/; SameSite=Strict",);
    let mut resp = Json(TokenResponse { token }).into_response();
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        resp.headers_mut().insert(header::SET_COOKIE, value);
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ct_eq_handles_length_mismatch() {
        assert!(!ct_eq(b"abc", b"abcd"));
        assert!(ct_eq(b"abc", b"abc"));
        assert!(!ct_eq(b"abc", b"abd"));
    }

    #[test]
    fn cookie_extract_picks_named_segment() {
        let mut h = HeaderMap::new();
        h.insert(header::COOKIE, HeaderValue::from_static("foo=bar; ferro_csrf=token123; baz=qux"));
        assert_eq!(extract_cookie(&h, COOKIE_NAME).as_deref(), Some("token123"));
    }

    #[test]
    fn cookie_extract_misses_when_absent() {
        let mut h = HeaderMap::new();
        h.insert(header::COOKIE, HeaderValue::from_static("foo=bar"));
        assert!(extract_cookie(&h, COOKIE_NAME).is_none());
    }

    #[test]
    fn bearer_detection_handles_case_and_whitespace() {
        let mut h = HeaderMap::new();
        h.insert(header::AUTHORIZATION, HeaderValue::from_static(" Bearer xyz"));
        assert!(has_bearer(&h));
        h.insert(header::AUTHORIZATION, HeaderValue::from_static("Basic xyz"));
        assert!(!has_bearer(&h));
    }
}
