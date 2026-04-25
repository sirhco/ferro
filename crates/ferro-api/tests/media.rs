//! Multipart upload + list + raw stream + delete round-trip.

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use ferro_api::AppState;
use ferro_auth::{hash_password, AuthService, JwtManager, MemorySessionStore};
use ferro_core::{Locale, Permission, Role, RoleId, Site, SiteSettings, User, UserId};
use ferro_media::MediaConfig;
use ferro_storage::StorageConfig;
use serde_json::{json, Value};
use time::OffsetDateTime;
use tower::ServiceExt;

const ADMIN_EMAIL: &str = "admin@example.com";
const ADMIN_PASSWORD: &str = "correct-horse-battery-staple";

async fn fixture() -> (tempfile::TempDir, Arc<AppState>) {
    let tmp = tempfile::tempdir().unwrap();
    let storage = StorageConfig::FsJson { path: tmp.path().join("data") };
    let media_cfg = MediaConfig::Local {
        path: tmp.path().join("media"),
        base_url: Some("http://localhost/media".into()),
    };
    tokio::fs::create_dir_all(tmp.path().join("media")).await.unwrap();
    let repo: Arc<dyn ferro_storage::Repository> =
        Arc::from(ferro_storage::connect(&storage).await.unwrap());
    repo.migrate().await.unwrap();
    let media_store: Arc<dyn ferro_media::MediaStore> =
        Arc::from(ferro_media::connect(&media_cfg).await.unwrap());

    let now = OffsetDateTime::now_utc();
    repo.sites()
        .upsert(Site {
            id: ferro_core::SiteId::new(),
            slug: "default".into(),
            name: "Default".into(),
            description: None,
            primary_url: None,
            locales: vec![Locale::default()],
            default_locale: Locale::default(),
            settings: SiteSettings::default(),
            created_at: now,
            updated_at: now,
        })
        .await
        .unwrap();

    let admin_role = Role {
        id: RoleId::new(),
        name: "admin".into(),
        description: None,
        permissions: vec![Permission::ManageUsers],
    };
    repo.users().upsert_role(admin_role.clone()).await.unwrap();
    let user = User {
        id: UserId::new(),
        email: ADMIN_EMAIL.into(),
        handle: "admin".into(),
        display_name: None,
        password_hash: Some(hash_password(ADMIN_PASSWORD).unwrap()),
        roles: vec![admin_role.id],
        active: true,
        created_at: now,
        last_login: None,
    };
    repo.users().upsert(user).await.unwrap();

    let sessions = Arc::new(MemorySessionStore::new());
    let auth = Arc::new(AuthService::new(repo.clone(), sessions));
    let jwt = Arc::new(JwtManager::hs256("ferro-test", b"dev-only-test-jwt-secret-32-bytes"));
    let state = Arc::new(AppState::new(repo, media_store, auth, jwt));
    (tmp, state)
}

async fn login(state: Arc<AppState>) -> String {
    let body = json!({ "email": ADMIN_EMAIL, "password": ADMIN_PASSWORD }).to_string();
    let app = ferro_api::router(state);
    let resp = app
        .oneshot(
            Request::post("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    v["token"].as_str().unwrap().to_string()
}

fn multipart_body(boundary: &str, file_name: &str, mime: &str, content: &[u8]) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    buf.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"file\"; filename=\"{file_name}\"\r\n")
            .as_bytes(),
    );
    buf.extend_from_slice(format!("Content-Type: {mime}\r\n\r\n").as_bytes());
    buf.extend_from_slice(content);
    buf.extend_from_slice(b"\r\n");
    buf.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    buf.extend_from_slice(b"Content-Disposition: form-data; name=\"alt\"\r\n\r\n");
    buf.extend_from_slice(b"a tiny test image");
    buf.extend_from_slice(b"\r\n");
    buf.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    buf
}

#[tokio::test]
async fn upload_list_get_raw_delete_round_trip() {
    let (_tmp, state) = fixture().await;
    let token = login(state.clone()).await;

    // 1×1 PNG (8 bytes signature + IHDR/IDAT/IEND minimal). Use a fixed binary
    // payload — exact content doesn't matter as long as it round-trips.
    let png: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x00, 0x00, 0x00, 0x00, 0x3A,
        0x7E, 0x9B, 0x55,
    ];
    let boundary = "ferroBoundary123";
    let body = multipart_body(boundary, "tiny.png", "image/png", png);

    let app = ferro_api::router(state.clone());
    let resp = app
        .oneshot(
            Request::post("/api/v1/media")
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    let id = v["id"].as_str().unwrap().to_string();
    assert_eq!(v["filename"], "tiny.png");
    assert_eq!(v["mime"], "image/png");
    assert_eq!(v["kind"], "image");
    assert_eq!(v["alt"], "a tiny test image");

    // List
    let app = ferro_api::router(state.clone());
    let resp = app
        .oneshot(
            Request::get("/api/v1/media")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
    let arr: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(arr.as_array().unwrap().len(), 1);

    // Raw stream
    let app = ferro_api::router(state.clone());
    let resp = app
        .oneshot(
            Request::get(format!("/api/v1/media/{id}/raw"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get(header::CONTENT_TYPE).unwrap().to_str().unwrap(),
        "image/png"
    );
    let raw = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
    assert_eq!(raw.as_ref(), png);

    // Delete
    let app = ferro_api::router(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/media/{id}"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // List is now empty
    let app = ferro_api::router(state);
    let resp = app
        .oneshot(
            Request::get("/api/v1/media")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
    let arr: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(arr.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn upload_requires_auth() {
    let (_tmp, state) = fixture().await;
    let app = ferro_api::router(state);
    let resp = app
        .oneshot(
            Request::post("/api/v1/media")
                .header(header::CONTENT_TYPE, "multipart/form-data; boundary=x")
                .body(Body::from("--x--\r\n"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
