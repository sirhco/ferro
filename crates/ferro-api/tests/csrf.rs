//! CSRF double-submit middleware behavior.

use std::sync::Arc;

use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
};
use ferro_api::AppState;
use ferro_auth::{AuthService, JwtManager, MemorySessionStore};
use ferro_media::MediaConfig;
use ferro_storage::StorageConfig;
use serde_json::{json, Value};
use tower::ServiceExt;

async fn state() -> (tempfile::TempDir, Arc<AppState>) {
    let tmp = tempfile::tempdir().unwrap();
    let storage = StorageConfig::FsJson { path: tmp.path().join("data") };
    let media = MediaConfig::Local { path: tmp.path().join("media"), base_url: None };
    tokio::fs::create_dir_all(tmp.path().join("media")).await.unwrap();
    let repo: Arc<dyn ferro_storage::Repository> =
        Arc::from(ferro_storage::connect(&storage).await.unwrap());
    repo.migrate().await.unwrap();
    let media_store: Arc<dyn ferro_media::MediaStore> =
        Arc::from(ferro_media::connect(&media).await.unwrap());
    let sessions = Arc::new(MemorySessionStore::new());
    let auth = Arc::new(AuthService::new(repo.clone(), sessions));
    let jwt = Arc::new(JwtManager::hs256("ferro-test", b"dev-only-test-jwt-secret-32-bytes"));
    (tmp, Arc::new(AppState::new(repo, media_store, auth, jwt)))
}

#[tokio::test]
async fn mint_endpoint_sets_cookie_and_returns_token() {
    let (_tmp, state) = state().await;
    let app = ferro_api::router(state);
    let resp =
        app.oneshot(Request::get("/api/v1/auth/csrf").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let cookie = resp
        .headers()
        .get(header::SET_COOKIE)
        .expect("Set-Cookie present")
        .to_str()
        .unwrap()
        .to_string();
    assert!(cookie.starts_with("ferro_csrf="), "{cookie}");
    assert!(cookie.contains("SameSite=Strict"), "{cookie}");
    let bytes = to_bytes(resp.into_body(), 4096).await.unwrap();
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    let json_token = v["token"].as_str().unwrap().to_string();
    let cookie_token = cookie.strip_prefix("ferro_csrf=").unwrap().split(';').next().unwrap();
    assert_eq!(json_token, cookie_token);
    assert_eq!(json_token.len(), 64, "expected 32-byte hex");
}

#[tokio::test]
async fn post_without_cookie_passes() {
    let (_tmp, state) = state().await;
    let app = ferro_api::router(state);
    let resp = app
        .oneshot(
            Request::post("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "email": "x@example.com", "password": "wrong" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    // Login itself fails (bad creds) — what matters here is that CSRF didn't
    // block it. 401, not 403.
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn post_with_cookie_and_matching_header_passes() {
    let (_tmp, state) = state().await;
    let app = ferro_api::router(state);
    let token = "deadbeef".repeat(8);
    let resp = app
        .oneshot(
            Request::post("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, format!("ferro_csrf={token}"))
                .header("x-csrf-token", token.clone())
                .body(Body::from(
                    json!({ "email": "x@example.com", "password": "wrong" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn post_with_cookie_but_missing_header_is_forbidden() {
    let (_tmp, state) = state().await;
    let app = ferro_api::router(state);
    let resp = app
        .oneshot(
            Request::post("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, "ferro_csrf=abc123")
                .body(Body::from(
                    json!({ "email": "x@example.com", "password": "wrong" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn post_with_cookie_and_mismatched_header_is_forbidden() {
    let (_tmp, state) = state().await;
    let app = ferro_api::router(state);
    let resp = app
        .oneshot(
            Request::post("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, "ferro_csrf=abc123")
                .header("x-csrf-token", "different")
                .body(Body::from(
                    json!({ "email": "x@example.com", "password": "wrong" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn bearer_request_bypasses_csrf_even_with_bad_cookie() {
    let (_tmp, state) = state().await;
    let app = ferro_api::router(state);
    let resp = app
        .oneshot(
            Request::post("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, "Bearer not-a-real-jwt")
                .header(header::COOKIE, "ferro_csrf=abc123")
                .header("x-csrf-token", "wrong")
                .body(Body::from(
                    json!({ "email": "x@example.com", "password": "wrong" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    // Bypassed CSRF ⇒ login handler runs ⇒ 401 (bad creds), not 403.
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_request_bypasses_csrf() {
    let (_tmp, state) = state().await;
    let app = ferro_api::router(state);
    let resp = app
        .oneshot(
            Request::get("/healthz")
                .header(header::COOKIE, "ferro_csrf=abc123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
