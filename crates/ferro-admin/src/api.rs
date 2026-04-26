//! Browser-side REST client. Mirrors the auth + retry semantics of the
//! legacy `admin.html` SPA: tokens in `localStorage`, automatic one-shot
//! refresh-token rotation on 401, JSON in/out.
//!
//! The whole module is feature-gated to `hydrate` because the `gloo-net`,
//! `web-sys`, and `wasm-bindgen-futures` crates only build under
//! `target_arch = wasm32`. Server-side render paths never call these
//! functions; they emit empty placeholders that the WASM bundle hydrates.

#![cfg(feature = "hydrate")]

use gloo_net::http::{Method, Request, RequestBuilder};
use wasm_bindgen::JsCast;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use std::cell::RefCell;
use thiserror::Error;
use wasm_bindgen::JsValue;

const TOKEN_KEY: &str = "ferro.admin.token";
const REFRESH_KEY: &str = "ferro.admin.refresh";

thread_local! {
    /// Single-flight latch so concurrent 401 handlers coalesce into one
    /// refresh exchange.
    static REFRESH_INFLIGHT: RefCell<bool> = const { RefCell::new(false) };
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("network: {0}")]
    Network(String),
    #[error("http {status}: {message}")]
    Http {
        status: u16,
        message: String,
        body: Option<Value>,
    },
}

impl ApiError {
    pub fn message(&self) -> String {
        match self {
            ApiError::Network(s) => s.clone(),
            ApiError::Http { message, .. } => message.clone(),
        }
    }
    pub fn status(&self) -> Option<u16> {
        match self {
            ApiError::Http { status, .. } => Some(*status),
            _ => None,
        }
    }
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window().and_then(|w| w.local_storage().ok().flatten())
}

pub fn get_token() -> Option<String> {
    storage().and_then(|s| s.get_item(TOKEN_KEY).ok().flatten())
}

pub fn get_refresh() -> Option<String> {
    storage().and_then(|s| s.get_item(REFRESH_KEY).ok().flatten())
}

pub fn set_tokens(access: Option<&str>, refresh: Option<&str>) {
    let Some(s) = storage() else { return };
    match access {
        Some(t) => {
            let _ = s.set_item(TOKEN_KEY, t);
        }
        None => {
            let _ = s.remove_item(TOKEN_KEY);
        }
    }
    match refresh {
        Some(t) => {
            let _ = s.set_item(REFRESH_KEY, t);
        }
        None => {
            let _ = s.remove_item(REFRESH_KEY);
        }
    }
}

pub fn clear_tokens() {
    set_tokens(None, None);
}

fn build(method: Method, path: &str) -> RequestBuilder {
    let req = RequestBuilder::new(path).method(method);
    if let Some(tok) = get_token() {
        req.header("Authorization", &format!("Bearer {tok}"))
    } else {
        req
    }
}

async fn try_refresh() -> bool {
    // Coalesce concurrent refresh attempts. If one is already in flight,
    // bail — the original caller will pick up the rotated tokens.
    let already = REFRESH_INFLIGHT.with(|c| {
        let mut b = c.borrow_mut();
        if *b {
            true
        } else {
            *b = true;
            false
        }
    });
    if already {
        return get_token().is_some();
    }
    let outcome = perform_refresh().await;
    REFRESH_INFLIGHT.with(|c| *c.borrow_mut() = false);
    outcome
}

async fn perform_refresh() -> bool {
    let Some(refresh) = get_refresh() else {
        return false;
    };
    let body = serde_json::json!({ "refresh_token": refresh });
    let req = match RequestBuilder::new("/api/v1/auth/refresh")
        .method(Method::POST)
        .header("Content-Type", "application/json")
        .body(body.to_string())
    {
        Ok(r) => r,
        Err(_) => return false,
    };
    let res = match req.send().await {
        Ok(r) => r,
        Err(_) => {
            clear_tokens();
            return false;
        }
    };
    if !res.ok() {
        clear_tokens();
        return false;
    }
    let parsed: Value = match res.json().await {
        Ok(v) => v,
        Err(_) => return false,
    };
    let access = parsed.get("token").and_then(|v| v.as_str());
    let new_refresh = parsed.get("refresh_token").and_then(|v| v.as_str());
    set_tokens(access, new_refresh);
    access.is_some()
}

async fn send_once(
    method: Method,
    path: &str,
    body: Option<&str>,
) -> Result<gloo_net::http::Response, ApiError> {
    let req = build(method, path).header("Accept", "application/json");
    let req = if let Some(b) = body {
        req.header("Content-Type", "application/json")
            .body(b.to_string())
            .map_err(|e| ApiError::Network(format!("build: {e:?}")))?
    } else {
        req.build().map_err(|e| ApiError::Network(format!("build: {e:?}")))?
    };
    req.send()
        .await
        .map_err(|e| ApiError::Network(format!("{e:?}")))
}

/// Send a JSON-serialized request, automatically retrying once after a
/// refresh-token rotation when the server returns 401.
pub async fn request<T: DeserializeOwned>(
    method: Method,
    path: &str,
    body: Option<&Value>,
) -> Result<T, ApiError> {
    let body_str = body.map(|v| v.to_string());
    let body_ref = body_str.as_deref();
    let mut res = send_once(method.clone(), path, body_ref).await?;
    if res.status() == 401 && get_refresh().is_some() {
        if try_refresh().await {
            res = send_once(method, path, body_ref).await?;
        }
    }
    let status = res.status();
    let text = res.text().await.unwrap_or_default();
    let parsed: Option<Value> = if text.is_empty() {
        None
    } else {
        serde_json::from_str(&text).ok()
    };
    if !res.ok() {
        let message = parsed
            .as_ref()
            .and_then(|v| v.get("message").and_then(|m| m.as_str()))
            .map(String::from)
            .unwrap_or_else(|| format!("{status}"));
        return Err(ApiError::Http { status, message, body: parsed });
    }
    if text.is_empty() {
        // Synthesize null for endpoints that return 204.
        return serde_json::from_value(Value::Null)
            .map_err(|e| ApiError::Network(format!("decode: {e}")));
    }
    serde_json::from_str(&text).map_err(|e| ApiError::Network(format!("decode: {e}")))
}

pub async fn get<T: DeserializeOwned>(path: &str) -> Result<T, ApiError> {
    request(Method::GET, path, None).await
}

pub async fn post<T, B>(path: &str, body: &B) -> Result<T, ApiError>
where
    T: DeserializeOwned,
    B: Serialize,
{
    let v = serde_json::to_value(body).map_err(|e| ApiError::Network(format!("encode: {e}")))?;
    request(Method::POST, path, Some(&v)).await
}

pub async fn post_empty<T: DeserializeOwned>(path: &str) -> Result<T, ApiError> {
    request(Method::POST, path, None).await
}

pub async fn patch<T, B>(path: &str, body: &B) -> Result<T, ApiError>
where
    T: DeserializeOwned,
    B: Serialize,
{
    let v = serde_json::to_value(body).map_err(|e| ApiError::Network(format!("encode: {e}")))?;
    request(Method::PATCH, path, Some(&v)).await
}

pub async fn delete<T: DeserializeOwned>(path: &str) -> Result<T, ApiError> {
    request(Method::DELETE, path, None).await
}

/// Multipart upload helper for the `/api/v1/media` endpoint. Builds a
/// FormData payload and posts it via `web_sys::Fetch` because `gloo-net`
/// doesn't expose multipart bodies directly.
pub async fn upload_media(
    file: web_sys::File,
    alt: Option<&str>,
) -> Result<Value, ApiError> {
    let form = web_sys::FormData::new()
        .map_err(|e| ApiError::Network(format!("formdata: {e:?}")))?;
    form.append_with_blob("file", &file)
        .map_err(|e| ApiError::Network(format!("append file: {e:?}")))?;
    if let Some(a) = alt {
        if !a.is_empty() {
            form.append_with_str("alt", a)
                .map_err(|e| ApiError::Network(format!("append alt: {e:?}")))?;
        }
    }
    let init = web_sys::RequestInit::new();
    init.set_method("POST");
    init.set_body(&JsValue::from(form));
    let request = web_sys::Request::new_with_str_and_init("/api/v1/media", &init)
        .map_err(|e| ApiError::Network(format!("request: {e:?}")))?;
    if let Some(tok) = get_token() {
        request
            .headers()
            .set("Authorization", &format!("Bearer {tok}"))
            .map_err(|e| ApiError::Network(format!("auth header: {e:?}")))?;
    }
    let window = web_sys::window().ok_or_else(|| ApiError::Network("no window".into()))?;
    let promise = window.fetch_with_request(&request);
    let response = wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .map_err(|e| ApiError::Network(format!("fetch: {e:?}")))?;
    let response: web_sys::Response = response
        .dyn_into()
        .map_err(|_| ApiError::Network("not a Response".into()))?;
    let text_promise = response
        .text()
        .map_err(|e| ApiError::Network(format!("text: {e:?}")))?;
    let text_js = wasm_bindgen_futures::JsFuture::from(text_promise)
        .await
        .map_err(|e| ApiError::Network(format!("text resolve: {e:?}")))?;
    let text = text_js.as_string().unwrap_or_default();
    let parsed: Option<Value> = serde_json::from_str(&text).ok();
    if !response.ok() {
        let message = parsed
            .as_ref()
            .and_then(|v| v.get("message").and_then(|m| m.as_str()))
            .map(String::from)
            .unwrap_or_else(|| format!("HTTP {}", response.status()));
        return Err(ApiError::Http { status: response.status(), message, body: parsed });
    }
    parsed.ok_or_else(|| ApiError::Network("empty response".into()))
}
