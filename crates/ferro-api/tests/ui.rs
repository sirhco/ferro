//! Smoke test that the operator HTML routes are wired and serve content.

use std::sync::Arc;

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use ferro_api::AppState;
use ferro_auth::{AuthService, JwtManager, MemorySessionStore};
use ferro_media::MediaConfig;
use ferro_storage::StorageConfig;
use tower::ServiceExt;

async fn fixture() -> (tempfile::TempDir, Arc<AppState>) {
    let tmp = tempfile::tempdir().unwrap();
    let storage = StorageConfig::FsJson { path: tmp.path().join("data") };
    let media = MediaConfig::Local {
        path: tmp.path().join("media"),
        base_url: Some("http://localhost/media".into()),
    };
    tokio::fs::create_dir_all(tmp.path().join("media")).await.unwrap();
    let repo: Arc<dyn ferro_storage::Repository> =
        Arc::from(ferro_storage::connect(&storage).await.unwrap());
    repo.migrate().await.unwrap();
    let media_store: Arc<dyn ferro_media::MediaStore> =
        Arc::from(ferro_media::connect(&media).await.unwrap());
    let sessions = Arc::new(MemorySessionStore::new());
    let auth = Arc::new(AuthService::new(repo.clone(), sessions));
    let jwt = Arc::new(JwtManager::hs256("ferro-test", b"dev-only-test-jwt-secret-32-bytes"));
    let state = Arc::new(AppState::new(repo, media_store, auth, jwt));
    (tmp, state)
}

#[tokio::test]
async fn root_serves_landing_html() {
    let (_tmp, state) = fixture().await;
    let app = ferro_api::router(state);
    let resp = app.oneshot(Request::get("/").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(ct.starts_with("text/html"), "wrong content-type: {ct}");
    let body = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
    let body = std::str::from_utf8(&body).unwrap();
    assert!(body.contains("Ferro"));
    assert!(body.contains("/admin"));
}

#[tokio::test]
async fn admin_route_not_owned_by_ferro_api() {
    // The Leptos SSR app in `ferro-admin` owns `/admin/*` now; ferro-cli
    // mounts it. ferro_api::router intentionally returns 404 here so the
    // CLI's `leptos_routes` layer is the single source of truth.
    let (_tmp, state) = fixture().await;
    let app = ferro_api::router(state);
    let resp = app.oneshot(Request::get("/admin").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
