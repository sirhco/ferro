//! End-to-end TOTP / 2FA flows over REST.

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use ferro_api::AppState;
use ferro_auth::{hash_password, AuthService, JwtManager, MemorySessionStore};
use ferro_core::{Locale, Site, SiteSettings, User, UserId};
use ferro_media::MediaConfig;
use ferro_storage::StorageConfig;
use serde_json::{json, Value};
use time::OffsetDateTime;
use tower::ServiceExt;

const EMAIL: &str = "u@example.com";
const PASSWORD: &str = "original-password";

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

    let user = User {
        id: UserId::new(),
        email: EMAIL.into(),
        handle: "u".into(),
        display_name: None,
        password_hash: Some(hash_password(PASSWORD).unwrap()),
        roles: Vec::new(),
        active: true,
        created_at: now,
        last_login: None,
        password_changed_at: None,
        totp_secret: None,
    };
    repo.users().upsert(user).await.unwrap();

    let sessions = Arc::new(MemorySessionStore::new());
    let auth = Arc::new(AuthService::new(repo.clone(), sessions));
    let jwt = Arc::new(JwtManager::hs256("ferro-test", b"dev-only-test-jwt-secret-32-bytes"));
    let state = Arc::new(AppState::new(repo, media_store, auth, jwt));
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
async fn totp_setup_enable_login_full_flow() {
    let (_tmp, state) = fixture().await;

    // Login (no TOTP yet) — should return tokens directly.
    let (s, v) = req_json(
        state.clone(),
        json_post(
            "/api/v1/auth/login",
            json!({ "email": EMAIL, "password": PASSWORD }),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let token = v["token"].as_str().expect("plain login should give token").to_string();

    // Setup → mints fresh secret.
    let (s, setup) = req_json(
        state.clone(),
        Request::post("/api/v1/auth/totp/setup")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let secret = setup["secret"].as_str().unwrap().to_string();
    assert!(setup["otpauth_uri"].as_str().unwrap().starts_with("otpauth://totp/"));

    // Enable with current code.
    let now = OffsetDateTime::now_utc();
    let code = ferro_auth::totp::generate(&secret, now).unwrap();
    let (s, _) = req_json(
        state.clone(),
        json_post(
            "/api/v1/auth/totp/enable",
            json!({ "secret": secret, "code": code }),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);

    // Login again — now MFA-required.
    let (s, v) = req_json(
        state.clone(),
        json_post(
            "/api/v1/auth/login",
            json!({ "email": EMAIL, "password": PASSWORD }),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["mfa_required"], true);
    let mfa_token = v["mfa_token"].as_str().unwrap().to_string();
    assert!(v.get("token").is_none() || v["token"].is_null());

    // Wrong code → 401.
    let (s, _) = req_json(
        state.clone(),
        json_post(
            "/api/v1/auth/totp/login",
            json!({ "mfa_token": mfa_token.clone(), "code": "000000" }),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    // Same mfa_token can't be reused (one-shot).
    // Need a fresh challenge for the success case.
    let (_s, v) = req_json(
        state.clone(),
        json_post(
            "/api/v1/auth/login",
            json!({ "email": EMAIL, "password": PASSWORD }),
            None,
        ),
    )
    .await;
    let mfa_token2 = v["mfa_token"].as_str().unwrap().to_string();
    let code2 = ferro_auth::totp::generate(&secret, OffsetDateTime::now_utc()).unwrap();
    let (s, v) = req_json(
        state.clone(),
        json_post(
            "/api/v1/auth/totp/login",
            json!({ "mfa_token": mfa_token2, "code": code2 }),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "got: {v}");
    assert!(v["token"].as_str().unwrap().len() > 20);
    assert!(v["refresh_token"].as_str().unwrap().len() > 20);
}

#[tokio::test]
async fn totp_setup_rejected_when_already_enabled() {
    let (_tmp, state) = fixture().await;

    // Login → setup → enable.
    let (_s, v) = req_json(
        state.clone(),
        json_post(
            "/api/v1/auth/login",
            json!({ "email": EMAIL, "password": PASSWORD }),
            None,
        ),
    )
    .await;
    let token = v["token"].as_str().unwrap().to_string();

    let (_, setup) = req_json(
        state.clone(),
        Request::post("/api/v1/auth/totp/setup")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let secret = setup["secret"].as_str().unwrap().to_string();
    let code = ferro_auth::totp::generate(&secret, OffsetDateTime::now_utc()).unwrap();
    req_json(
        state.clone(),
        json_post(
            "/api/v1/auth/totp/enable",
            json!({ "secret": secret, "code": code }),
            Some(&token),
        ),
    )
    .await;

    // Second setup call → 400.
    let (s, _) = req_json(
        state,
        Request::post("/api/v1/auth/totp/setup")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn totp_disable_clears_requirement() {
    let (_tmp, state) = fixture().await;

    let (_s, v) = req_json(
        state.clone(),
        json_post(
            "/api/v1/auth/login",
            json!({ "email": EMAIL, "password": PASSWORD }),
            None,
        ),
    )
    .await;
    let token = v["token"].as_str().unwrap().to_string();
    let (_, setup) = req_json(
        state.clone(),
        Request::post("/api/v1/auth/totp/setup")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let secret = setup["secret"].as_str().unwrap().to_string();
    let code = ferro_auth::totp::generate(&secret, OffsetDateTime::now_utc()).unwrap();
    req_json(
        state.clone(),
        json_post(
            "/api/v1/auth/totp/enable",
            json!({ "secret": secret.clone(), "code": code }),
            Some(&token),
        ),
    )
    .await;

    // Disable with valid code.
    let code = ferro_auth::totp::generate(&secret, OffsetDateTime::now_utc()).unwrap();
    let (s, _) = req_json(
        state.clone(),
        json_post(
            "/api/v1/auth/totp/disable",
            json!({ "code": code }),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);

    // Login no longer requires MFA.
    let (_s, v) = req_json(
        state,
        json_post(
            "/api/v1/auth/login",
            json!({ "email": EMAIL, "password": PASSWORD }),
            None,
        ),
    )
    .await;
    assert!(v["token"].as_str().is_some());
    assert!(v.get("mfa_required").is_none() || !v["mfa_required"].as_bool().unwrap_or(false));
}
