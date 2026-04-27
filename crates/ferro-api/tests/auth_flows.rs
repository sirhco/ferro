//! Public signup + change-password.

use std::sync::Arc;

use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
};
use ferro_api::{AppState, AuthOptions};
use ferro_auth::{hash_password, AuthService, JwtManager, MemorySessionStore};
use ferro_core::{Locale, Site, SiteSettings, User, UserId};
use ferro_media::MediaConfig;
use ferro_storage::StorageConfig;
use serde_json::{json, Value};
use time::OffsetDateTime;
use tower::ServiceExt;

async fn fixture(allow_public_signup: bool) -> (tempfile::TempDir, Arc<AppState>) {
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

    let now = OffsetDateTime::now_utc();
    let site = Site {
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
    };
    repo.sites().upsert(site).await.unwrap();

    let sessions = Arc::new(MemorySessionStore::new());
    let auth = Arc::new(AuthService::new(repo.clone(), sessions));
    let jwt = Arc::new(JwtManager::hs256("ferro-test", b"dev-only-test-jwt-secret-32-bytes"));

    let state = Arc::new(
        AppState::new(repo, media_store, auth, jwt)
            .with_options(AuthOptions { allow_public_signup }),
    );
    (tmp, state)
}

async fn req_json(state: Arc<AppState>, req: Request<Body>) -> (StatusCode, Value) {
    let app = ferro_api::router(state);
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
    let v = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, v)
}

fn json_post(path: &str, body: Value, bearer: Option<&str>) -> Request<Body> {
    let mut b = Request::post(path).header(header::CONTENT_TYPE, "application/json");
    if let Some(t) = bearer {
        b = b.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    b.body(Body::from(body.to_string())).unwrap()
}

#[tokio::test]
async fn signup_disabled_returns_403() {
    let (_tmp, state) = fixture(false).await;
    let (s, _) = req_json(
        state,
        json_post(
            "/api/v1/auth/signup",
            json!({
                "email": "new@example.com",
                "handle": "newbie",
                "password": "strong-pw-please"
            }),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn signup_enabled_creates_user_and_returns_token() {
    let (_tmp, state) = fixture(true).await;
    let (s, v) = req_json(
        state.clone(),
        json_post(
            "/api/v1/auth/signup",
            json!({
                "email": "new@example.com",
                "handle": "newbie",
                "password": "strong-pw-please"
            }),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "got: {v}");
    assert!(v["token"].as_str().unwrap().len() > 20);
    assert_eq!(v["user"]["email"], "new@example.com");
    assert!(v["user"]["password_hash"].is_null(), "hash leaked");

    // Login with the new password works.
    let (s, login) = req_json(
        state,
        json_post(
            "/api/v1/auth/login",
            json!({ "email": "new@example.com", "password": "strong-pw-please" }),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(!login["token"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn signup_rejects_weak_password() {
    let (_tmp, state) = fixture(true).await;
    let (s, _) = req_json(
        state,
        json_post(
            "/api/v1/auth/signup",
            json!({ "email": "x@y.z", "handle": "h", "password": "short" }),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn signup_rejects_duplicate_email() {
    let (tmp, state) = fixture(true).await;
    seed_user(&tmp, &state, "taken@example.com", "old-password-here").await;

    let (s, _) = req_json(
        state,
        json_post(
            "/api/v1/auth/signup",
            json!({
                "email": "taken@example.com",
                "handle": "x",
                "password": "different-password"
            }),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn change_password_rotates_hash() {
    let (tmp, state) = fixture(false).await;
    seed_user(&tmp, &state, "u@example.com", "original-password").await;

    // Login with original
    let (_s, v) = req_json(
        state.clone(),
        json_post(
            "/api/v1/auth/login",
            json!({ "email": "u@example.com", "password": "original-password" }),
            None,
        ),
    )
    .await;
    let token = v["token"].as_str().unwrap().to_string();

    // Change password
    let (s, _) = req_json(
        state.clone(),
        json_post(
            "/api/v1/auth/change-password",
            json!({
                "current_password": "original-password",
                "new_password": "rotated-password"
            }),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);

    // Old password no longer logs in
    let (s, _) = req_json(
        state.clone(),
        json_post(
            "/api/v1/auth/login",
            json!({ "email": "u@example.com", "password": "original-password" }),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    // New password works
    let (s, _) = req_json(
        state,
        json_post(
            "/api/v1/auth/login",
            json!({ "email": "u@example.com", "password": "rotated-password" }),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
}

#[tokio::test]
async fn old_jwt_rejected_after_password_change() {
    let (tmp, state) = fixture(false).await;
    seed_user(&tmp, &state, "u@example.com", "original-password").await;

    // Mint old token via login.
    let (_s, v) = req_json(
        state.clone(),
        json_post(
            "/api/v1/auth/login",
            json!({ "email": "u@example.com", "password": "original-password" }),
            None,
        ),
    )
    .await;
    let old_token = v["token"].as_str().unwrap().to_string();

    // The /me endpoint accepts the token.
    let req = Request::get("/api/v1/auth/me")
        .header(header::AUTHORIZATION, format!("Bearer {old_token}"))
        .body(Body::empty())
        .unwrap();
    let (s, _) = req_json(state.clone(), req).await;
    assert_eq!(s, StatusCode::OK);

    // Sleep one full second so the new password_changed_at timestamp is
    // strictly *after* the JWT's `iat` (`iat` has 1-second resolution).
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

    // Rotate password.
    let (s, _) = req_json(
        state.clone(),
        json_post(
            "/api/v1/auth/change-password",
            json!({
                "current_password": "original-password",
                "new_password": "rotated-password"
            }),
            Some(&old_token),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);

    // Old token now rejected.
    let req = Request::get("/api/v1/auth/me")
        .header(header::AUTHORIZATION, format!("Bearer {old_token}"))
        .body(Body::empty())
        .unwrap();
    let (s, _) = req_json(state, req).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn change_password_rejects_wrong_current() {
    let (tmp, state) = fixture(false).await;
    seed_user(&tmp, &state, "u@example.com", "original-password").await;

    let (_s, v) = req_json(
        state.clone(),
        json_post(
            "/api/v1/auth/login",
            json!({ "email": "u@example.com", "password": "original-password" }),
            None,
        ),
    )
    .await;
    let token = v["token"].as_str().unwrap().to_string();

    let (s, _) = req_json(
        state,
        json_post(
            "/api/v1/auth/change-password",
            json!({
                "current_password": "wrong-current",
                "new_password": "rotated-password"
            }),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn refresh_token_rotates_pair_and_invalidates_old() {
    let (tmp, state) = fixture(false).await;
    seed_user(&tmp, &state, "u@example.com", "original-password").await;

    // Login → get refresh1
    let (s, v) = req_json(
        state.clone(),
        json_post(
            "/api/v1/auth/login",
            json!({ "email": "u@example.com", "password": "original-password" }),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let access1 = v["token"].as_str().unwrap().to_string();
    let refresh1 = v["refresh_token"].as_str().unwrap().to_string();
    assert!(!access1.is_empty());
    assert!(!refresh1.is_empty());
    assert_ne!(access1, refresh1, "access and refresh must differ");

    // Refresh → new pair
    let (s, v) = req_json(
        state.clone(),
        json_post("/api/v1/auth/refresh", json!({ "refresh_token": refresh1 }), None),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let _access2 = v["token"].as_str().unwrap().to_string();
    let refresh2 = v["refresh_token"].as_str().unwrap().to_string();
    // Access JWT may collide if rotation lands within the same `iat` second;
    // the refresh side is random so it's the meaningful uniqueness check.
    assert_ne!(refresh1, refresh2);

    // Re-using refresh1 must fail (one-shot rotation).
    let (s, _) = req_json(
        state.clone(),
        json_post("/api/v1/auth/refresh", json!({ "refresh_token": refresh1 }), None),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    // refresh2 is still valid → can rotate again.
    let (s, _) = req_json(
        state.clone(),
        json_post("/api/v1/auth/refresh", json!({ "refresh_token": refresh2 }), None),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
}

#[tokio::test]
async fn refresh_rejects_garbage_token() {
    let (_tmp, state) = fixture(false).await;
    let (s, _) = req_json(
        state,
        json_post("/api/v1/auth/refresh", json!({ "refresh_token": "not-a-real-token" }), None),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn logout_revokes_refresh_token() {
    let (tmp, state) = fixture(false).await;
    seed_user(&tmp, &state, "u@example.com", "original-password").await;

    let (_s, v) = req_json(
        state.clone(),
        json_post(
            "/api/v1/auth/login",
            json!({ "email": "u@example.com", "password": "original-password" }),
            None,
        ),
    )
    .await;
    let access = v["token"].as_str().unwrap().to_string();
    let refresh = v["refresh_token"].as_str().unwrap().to_string();

    // Logout with refresh in body.
    let (s, _) = req_json(
        state.clone(),
        json_post("/api/v1/auth/logout", json!({ "refresh_token": refresh }), Some(&access)),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);

    // Refresh now rejected.
    let (s, _) = req_json(
        state,
        json_post(
            "/api/v1/auth/refresh",
            json!({ "refresh_token": v["refresh_token"].as_str().unwrap() }),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

async fn seed_user(tmp: &tempfile::TempDir, state: &Arc<AppState>, email: &str, password: &str) {
    let user = User {
        id: UserId::new(),
        email: email.into(),
        handle: email.split('@').next().unwrap().into(),
        display_name: None,
        password_hash: Some(hash_password(password).unwrap()),
        roles: Vec::new(),
        active: true,
        created_at: OffsetDateTime::now_utc(),
        last_login: None,
        password_changed_at: None,
        totp_secret: None,
    };
    state.repo.users().upsert(user.clone()).await.unwrap();
    // Belt-and-braces: also write the JSON manually so older tests that
    // share this seed pattern don't regress if something changes.
    let path = tmp.path().join("data/users").join(format!("{}.json", user.id));
    let mut value = serde_json::to_value(&user).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .insert("password_hash".into(), serde_json::json!(user.password_hash));
    tokio::fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).await.unwrap();
}
