//! User + role management REST endpoints.

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use ferro_api::AppState;
use ferro_auth::{hash_password, AuthService, JwtManager, MemorySessionStore};
use ferro_core::{
    Locale, Permission, Role, RoleId, Site, SiteSettings, User, UserId,
};
use ferro_media::MediaConfig;
use ferro_storage::StorageConfig;
use serde_json::{json, Value};
use time::OffsetDateTime;
use tower::ServiceExt;

const ADMIN_EMAIL: &str = "admin@example.com";
const ADMIN_PASSWORD: &str = "correct-horse-battery-staple";
const NON_ADMIN_EMAIL: &str = "viewer@example.com";
const NON_ADMIN_PASSWORD: &str = "weak-tea-but-fine-for-tests";

struct Fixture {
    _tmp: tempfile::TempDir,
    state: Arc<AppState>,
}

async fn fixture(non_admin_perms: Vec<Permission>) -> Fixture {
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
    repo.sites().upsert(site.clone()).await.unwrap();

    // Two roles: admin (ManageUsers) + viewer (whatever caller passes).
    let admin_role = Role {
        id: RoleId::new(),
        name: "admin".into(),
        description: None,
        permissions: vec![Permission::ManageUsers],
    };
    repo.users().upsert_role(admin_role.clone()).await.unwrap();

    let viewer_role = Role {
        id: RoleId::new(),
        name: "viewer".into(),
        description: None,
        permissions: non_admin_perms,
    };
    repo.users().upsert_role(viewer_role.clone()).await.unwrap();

    seed_user(&repo, &tmp, ADMIN_EMAIL, "admin", ADMIN_PASSWORD, vec![admin_role.id]).await;
    seed_user(
        &repo,
        &tmp,
        NON_ADMIN_EMAIL,
        "viewer",
        NON_ADMIN_PASSWORD,
        vec![viewer_role.id],
    )
    .await;

    let sessions = Arc::new(MemorySessionStore::new());
    let auth = Arc::new(AuthService::new(repo.clone(), sessions));
    let jwt = Arc::new(JwtManager::hs256("ferro-test", b"dev-only-test-jwt-secret-32-bytes"));
    let state = Arc::new(AppState::new(repo, media_store, auth, jwt));
    Fixture { _tmp: tmp, state }
}

async fn seed_user(
    repo: &Arc<dyn ferro_storage::Repository>,
    tmp: &tempfile::TempDir,
    email: &str,
    handle: &str,
    password: &str,
    roles: Vec<RoleId>,
) {
    let user = User {
        id: UserId::new(),
        email: email.into(),
        handle: handle.into(),
        display_name: None,
        password_hash: Some(hash_password(password).unwrap()),
        roles,
        active: true,
        created_at: OffsetDateTime::now_utc(),
        last_login: None,
        password_changed_at: None,
        totp_secret: None,
    };
    repo.users().upsert(user.clone()).await.unwrap();
    // Re-write directly to disk to preserve the password_hash field
    // (`#[serde(skip_serializing)]` drops it on the upsert path for fs-json).
    let path = tmp.path().join("data/users").join(format!("{}.json", user.id));
    let mut value = serde_json::to_value(&user).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .insert("password_hash".into(), serde_json::json!(user.password_hash));
    tokio::fs::write(&path, serde_json::to_vec_pretty(&value).unwrap())
        .await
        .unwrap();
}

async fn login(state: Arc<AppState>, email: &str, password: &str) -> String {
    let body = json!({ "email": email, "password": password }).to_string();
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

async fn req_json(state: Arc<AppState>, req: Request<Body>) -> (StatusCode, Value) {
    let app = ferro_api::router(state);
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), 256 * 1024).await.unwrap();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

#[tokio::test]
async fn admin_can_create_role_and_user() {
    let fx = fixture(Vec::new()).await;
    let token = login(fx.state.clone(), ADMIN_EMAIL, ADMIN_PASSWORD).await;

    // Create a role
    let (s, role_v) = req_json(
        fx.state.clone(),
        Request::post("/api/v1/roles")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::from(
                json!({
                    "name": "editor",
                    "description": "edits content",
                    "permissions": []
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "create role: {role_v}");
    let role_id = role_v["id"].as_str().unwrap().to_string();

    // Create a user with that role
    let (s, user_v) = req_json(
        fx.state.clone(),
        Request::post("/api/v1/users")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::from(
                json!({
                    "email": "new@example.com",
                    "handle": "newbie",
                    "password": "another-strong-password",
                    "roles": [role_id]
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "create user: {user_v}");
    assert_eq!(user_v["email"], "new@example.com");
    assert_eq!(user_v["active"], true);

    // The new user can log in with the password we set.
    let new_token = login(fx.state.clone(), "new@example.com", "another-strong-password").await;
    assert!(!new_token.is_empty());

    // List users — admin sees themselves + the seeded viewer + the new user.
    let (s, users_v) = req_json(
        fx.state.clone(),
        Request::get("/api/v1/users")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let arr = users_v.as_array().unwrap();
    assert_eq!(arr.len(), 3, "got {arr:#?}");

    // Patch the new user to deactivate
    let new_user_id = user_v["id"].as_str().unwrap();
    let (s, patched) = req_json(
        fx.state.clone(),
        Request::builder()
            .method("PATCH")
            .uri(format!("/api/v1/users/{new_user_id}"))
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::from(json!({ "active": false }).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(patched["active"], false);

    // Delete the role
    let (s, _) = req_json(
        fx.state.clone(),
        Request::builder()
            .method("DELETE")
            .uri(format!("/api/v1/roles/{role_id}"))
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn non_admin_cannot_manage_users() {
    let fx = fixture(Vec::new()).await;
    let token = login(fx.state.clone(), NON_ADMIN_EMAIL, NON_ADMIN_PASSWORD).await;

    let (s, _) = req_json(
        fx.state.clone(),
        Request::get("/api/v1/users")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    let (s, _) = req_json(
        fx.state.clone(),
        Request::post("/api/v1/roles")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::from(json!({ "name": "evil" }).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admin_cannot_delete_self() {
    let fx = fixture(Vec::new()).await;
    let token = login(fx.state.clone(), ADMIN_EMAIL, ADMIN_PASSWORD).await;

    // Look up admin user id.
    let (_s, users) = req_json(
        fx.state.clone(),
        Request::get("/api/v1/users")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let admin_id = users
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["email"] == ADMIN_EMAIL)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let (s, _) = req_json(
        fx.state.clone(),
        Request::builder()
            .method("DELETE")
            .uri(format!("/api/v1/users/{admin_id}"))
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_user_rejects_duplicate_email() {
    let fx = fixture(Vec::new()).await;
    let token = login(fx.state.clone(), ADMIN_EMAIL, ADMIN_PASSWORD).await;

    let (s, _) = req_json(
        fx.state.clone(),
        Request::post("/api/v1/users")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::from(
                json!({
                    "email": ADMIN_EMAIL,
                    "handle": "dup",
                    "password": "whatever-doesnt-matter"
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}
