//! End-to-end test for the REST auth + write path against the fs-json backend.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use ferro_api::AppState;
use ferro_auth::{hash_password, AuthService, JwtManager, MemorySessionStore};
use ferro_core::{
    Content, ContentType, FieldDef, FieldId, FieldKind, FieldValue, Locale, NewContent,
    Permission, Role, RoleId, Scope, Site, SiteSettings, User, UserId,
};
use ferro_media::MediaConfig;
use ferro_storage::StorageConfig;
use serde_json::{json, Value};
use time::OffsetDateTime;
use tower::ServiceExt;

const EMAIL: &str = "admin@example.com";
const PASSWORD: &str = "correct-horse-battery-staple";

struct Fixture {
    _tmp: tempfile::TempDir,
    state: Arc<AppState>,
    site: Site,
    post_type: ContentType,
    #[allow(dead_code)]
    user: User,
}

async fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().expect("tempdir");
    let storage_cfg = StorageConfig::FsJson { path: tmp.path().join("data") };
    let media_cfg = MediaConfig::Local {
        path: tmp.path().join("media"),
        base_url: Some("http://localhost/media".into()),
    };
    tokio::fs::create_dir_all(tmp.path().join("media")).await.unwrap();

    let repo: Arc<dyn ferro_storage::Repository> =
        Arc::from(ferro_storage::connect(&storage_cfg).await.expect("connect fs-json"));
    repo.migrate().await.unwrap();
    let media: Arc<dyn ferro_media::MediaStore> =
        Arc::from(ferro_media::connect(&media_cfg).await.expect("connect local media"));

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

    let post_type = ContentType {
        id: ferro_core::ContentTypeId::new(),
        site_id: site.id,
        slug: "post".into(),
        name: "Post".into(),
        description: None,
        fields: vec![FieldDef {
            id: FieldId::new(),
            slug: "title".into(),
            name: "Title".into(),
            help: None,
            kind: FieldKind::Text { multiline: false, max: Some(200) },
            required: true,
            localized: false,
            unique: false,
            hidden: false,
        }],
        singleton: false,
        title_field: Some("title".into()),
        slug_field: None,
        created_at: now,
        updated_at: now,
    };
    repo.types().upsert(post_type.clone()).await.unwrap();

    let role = Role {
        id: RoleId::new(),
        name: "editor".into(),
        description: None,
        permissions: vec![
            Permission::Write(Scope::Type { id: post_type.id }),
            Permission::Publish(Scope::Type { id: post_type.id }),
        ],
    };
    repo.users().upsert_role(role.clone()).await.unwrap();

    let user = User {
        id: UserId::new(),
        email: EMAIL.into(),
        handle: "admin".into(),
        display_name: Some("Admin".into()),
        password_hash: Some(hash_password(PASSWORD).unwrap()),
        roles: vec![role.id],
        active: true,
        created_at: now,
        last_login: None,
    };
    // `User.password_hash` is `#[serde(skip_serializing)]`, so going through
    // the repo would drop the hash on disk. Write the JSON directly so the
    // fs-json backend can read it back with `by_email`.
    let user_path = tmp
        .path()
        .join("data/users")
        .join(format!("{}.json", user.id));
    let mut user_value = serde_json::to_value(&user).unwrap();
    user_value
        .as_object_mut()
        .unwrap()
        .insert("password_hash".into(), serde_json::json!(user.password_hash));
    tokio::fs::write(
        &user_path,
        serde_json::to_vec_pretty(&user_value).unwrap(),
    )
    .await
    .unwrap();

    let sessions = Arc::new(MemorySessionStore::new());
    let auth = Arc::new(AuthService::new(repo.clone(), sessions));
    let jwt = Arc::new(JwtManager::hs256("ferro-test", b"dev-only-test-jwt-secret-32-bytes"));

    let state = Arc::new(AppState::new(repo, media, auth, jwt));
    Fixture { _tmp: tmp, state, site, post_type, user }
}

async fn json_body(req: Request<Body>, state: Arc<AppState>) -> (StatusCode, Value) {
    let app = ferro_api::router(state);
    let resp = app.oneshot(req).await.expect("router oneshot");
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), 64 * 1024).await.expect("body bytes");
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

fn post_json(path: &str, body: Value, bearer: Option<&str>) -> Request<Body> {
    let mut builder = Request::post(path).header(header::CONTENT_TYPE, "application/json");
    if let Some(t) = bearer {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    builder.body(Body::from(body.to_string())).unwrap()
}

#[tokio::test]
async fn login_returns_token_and_write_requires_it() {
    let fx = fixture().await;

    // Login
    let (status, value) = json_body(
        post_json(
            "/api/v1/auth/login",
            json!({ "email": EMAIL, "password": PASSWORD }),
            None,
        ),
        fx.state.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "login should succeed");
    let token = value["token"].as_str().expect("token present").to_string();

    // Create without token -> 401
    let body = serde_json::to_value(NewContent {
        type_id: fx.post_type.id,
        slug: "hello-world".into(),
        locale: Locale::default(),
        data: {
            let mut m = BTreeMap::new();
            m.insert("title".into(), FieldValue::String("Hello".into()));
            m
        },
        author_id: None,
    })
    .unwrap();
    let (status, _) = json_body(
        post_json("/api/v1/content/post", body.clone(), None),
        fx.state.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Create with token -> 200
    let (status, created) = json_body(
        post_json("/api/v1/content/post", body, Some(&token)),
        fx.state.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let created: Content = serde_json::from_value(created).expect("content JSON");
    assert_eq!(created.slug, "hello-world");
    assert_eq!(created.site_id, fx.site.id);
}

#[tokio::test]
async fn patch_publish_delete_round_trip() {
    let fx = fixture().await;

    // Login
    let (_s, v) = json_body(
        post_json(
            "/api/v1/auth/login",
            json!({ "email": EMAIL, "password": PASSWORD }),
            None,
        ),
        fx.state.clone(),
    )
    .await;
    let token = v["token"].as_str().unwrap().to_string();

    // Seed
    let body = serde_json::to_value(NewContent {
        type_id: fx.post_type.id,
        slug: "alpha".into(),
        locale: Locale::default(),
        data: {
            let mut m = BTreeMap::new();
            m.insert("title".into(), FieldValue::String("Alpha".into()));
            m
        },
        author_id: None,
    })
    .unwrap();
    let (s, _c) = json_body(
        post_json("/api/v1/content/post", body, Some(&token)),
        fx.state.clone(),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // PATCH
    let patch_req = Request::builder()
        .method("PATCH")
        .uri("/api/v1/content/post/alpha")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::from(
            json!({ "data": { "title": "Alpha v2" } }).to_string(),
        ))
        .unwrap();
    let (s, v) = json_body(patch_req, fx.state.clone()).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["data"]["title"], "Alpha v2");

    // Publish
    let (s, v) = json_body(
        post_json("/api/v1/content/post/alpha/publish", json!({}), Some(&token)),
        fx.state.clone(),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["status"], "published");

    // Delete
    let del_req = Request::builder()
        .method("DELETE")
        .uri("/api/v1/content/post/alpha")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let app = ferro_api::router(fx.state.clone());
    let resp = app.oneshot(del_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn invalid_token_rejected() {
    let fx = fixture().await;
    let req = Request::get("/api/v1/auth/me")
        .header(header::AUTHORIZATION, "Bearer not-a-real-token")
        .body(Body::empty())
        .unwrap();
    let app = ferro_api::router(fx.state.clone());
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
