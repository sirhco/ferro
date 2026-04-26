//! Integration test for the /preview/:type/:slug route.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use ferro_api::AppState;
use ferro_auth::{hash_password, AuthService, JwtManager, MemorySessionStore};
use ferro_core::{
    Content, ContentType, ContentTypeId, FieldDef, FieldId, FieldKind, FieldValue, Locale,
    Permission, RichFormat, Role, RoleId, Site, SiteSettings, Status, User, UserId,
};
use ferro_media::MediaConfig;
use ferro_storage::StorageConfig;
use serde_json::{json, Value};
use time::OffsetDateTime;
use tower::ServiceExt;

const ADMIN_EMAIL: &str = "preview-admin@example.com";
const ADMIN_PASSWORD: &str = "correct-horse-battery-staple";

#[tokio::test]
async fn preview_renders_blocks_and_title() {
    let (state, _tmp) = fixture().await;
    let token = login(state.clone(), ADMIN_EMAIL, ADMIN_PASSWORD).await;

    let app = ferro_api::router(state);
    let resp = app
        .oneshot(
            Request::get("/preview/page/about")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), 256 * 1024).await.unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert!(html.contains("<title>Preview"), "missing <title>: {html}");
    assert!(html.contains("About Ferro"), "title field missing: {html}");
    assert!(html.contains("<h1>About Ferro</h1>"), "h1 block missing: {html}");
    assert!(html.contains("<p>Ferro CMS"), "paragraph block missing: {html}");
    assert!(html.contains("<ul><li>"), "list block missing: {html}");
    assert!(
        html.contains("preview-pill-draft"),
        "draft pill missing: {html}"
    );
}

#[tokio::test]
async fn preview_requires_auth() {
    let (state, _tmp) = fixture().await;
    let app = ferro_api::router(state);
    let resp = app
        .oneshot(
            Request::get("/preview/page/about")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn preview_returns_404_for_unknown_slug() {
    let (state, _tmp) = fixture().await;
    let token = login(state.clone(), ADMIN_EMAIL, ADMIN_PASSWORD).await;

    let app = ferro_api::router(state);
    let resp = app
        .oneshot(
            Request::get("/preview/page/does-not-exist")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

async fn fixture() -> (Arc<AppState>, tempfile::TempDir) {
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

    let page_type = ContentType {
        id: ContentTypeId::new(),
        site_id: site.id,
        slug: "page".into(),
        name: "Page".into(),
        description: None,
        fields: vec![
            FieldDef {
                id: FieldId::new(),
                slug: "title".into(),
                name: "Title".into(),
                help: None,
                kind: FieldKind::Text { multiline: false, max: None },
                required: true,
                localized: false,
                unique: false,
                hidden: false,
            },
            FieldDef {
                id: FieldId::new(),
                slug: "blocks".into(),
                name: "Body".into(),
                help: None,
                kind: FieldKind::RichText { format: RichFormat::Blocks },
                required: false,
                localized: false,
                unique: false,
                hidden: false,
            },
        ],
        singleton: false,
        title_field: Some("title".into()),
        slug_field: Some("slug".into()),
        created_at: now,
        updated_at: now,
    };
    repo.types().upsert(page_type.clone()).await.unwrap();

    let blocks = json!([
        { "kind": "heading", "level": 1, "text": "About Ferro" },
        { "kind": "paragraph", "text": "Ferro CMS in one binary." },
        { "kind": "list", "ordered": false, "items": ["one", "two"] }
    ]);

    let mut data = BTreeMap::new();
    data.insert("title".into(), FieldValue::String("About Ferro".into()));
    data.insert("blocks".into(), FieldValue::Object(blocks));

    let content = Content {
        id: ferro_core::ContentId::new(),
        site_id: site.id,
        type_id: page_type.id,
        slug: "about".into(),
        locale: Locale::default(),
        status: Status::Draft,
        data,
        author_id: None,
        created_at: now,
        updated_at: now,
        published_at: None,
    };
    repo.content().upsert(content).await.unwrap();

    let admin_role = Role {
        id: RoleId::new(),
        name: "admin".into(),
        description: None,
        permissions: vec![Permission::ManageUsers, Permission::ManageSchema],
    };
    repo.users().upsert_role(admin_role.clone()).await.unwrap();
    seed_user(&repo, &tmp, ADMIN_EMAIL, "previewer", ADMIN_PASSWORD, vec![admin_role.id]).await;

    let sessions = Arc::new(MemorySessionStore::new());
    let auth = Arc::new(AuthService::new(repo.clone(), sessions));
    let jwt = Arc::new(JwtManager::hs256("ferro-test", b"dev-only-test-jwt-secret-32-bytes"));
    let state = Arc::new(AppState::new(repo, media_store, auth, jwt));
    (state, tmp)
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
