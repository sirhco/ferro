//! Content version snapshots + restore round-trip.

use std::{collections::BTreeMap, sync::Arc};

use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
};
use ferro_api::AppState;
use ferro_auth::{hash_password, AuthService, JwtManager, MemorySessionStore};
use ferro_core::{
    Content, ContentId, ContentType, ContentTypeId, FieldDef, FieldId, FieldKind, FieldValue,
    Locale, Permission, Role, RoleId, Scope, Site, SiteSettings, Status, User, UserId,
};
use ferro_media::MediaConfig;
use ferro_storage::StorageConfig;
use serde_json::{json, Value};
use time::OffsetDateTime;
use tower::ServiceExt;

const EMAIL: &str = "admin@example.com";
const PASSWORD: &str = "correct-horse-battery-staple";

async fn fixture() -> (tempfile::TempDir, Arc<AppState>, ContentType, Content) {
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

    let ty = ContentType {
        id: ContentTypeId::new(),
        site_id: site.id,
        slug: "post".into(),
        name: "Post".into(),
        description: None,
        fields: vec![FieldDef {
            id: FieldId::new(),
            slug: "title".into(),
            name: "Title".into(),
            help: None,
            kind: FieldKind::Text { multiline: false, max: None },
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
    repo.types().upsert(ty.clone()).await.unwrap();

    let role = Role {
        id: RoleId::new(),
        name: "editor".into(),
        description: None,
        permissions: vec![
            Permission::Write(Scope::Type { id: ty.id }),
            Permission::Publish(Scope::Type { id: ty.id }),
        ],
    };
    repo.users().upsert_role(role.clone()).await.unwrap();
    let user = User {
        id: UserId::new(),
        email: EMAIL.into(),
        handle: "admin".into(),
        display_name: None,
        password_hash: Some(hash_password(PASSWORD).unwrap()),
        roles: vec![role.id],
        active: true,
        created_at: now,
        last_login: None,
        password_changed_at: None,
        totp_secret: None,
    };
    repo.users().upsert(user).await.unwrap();

    // Seed a content row.
    let mut data = BTreeMap::new();
    data.insert("title".into(), FieldValue::String("v1".into()));
    let content = Content {
        id: ContentId::new(),
        site_id: site.id,
        type_id: ty.id,
        slug: "alpha".into(),
        locale: Locale::default(),
        status: Status::Draft,
        data,
        author_id: None,
        created_at: now,
        updated_at: now,
        published_at: None,
    };
    repo.content().upsert(content.clone()).await.unwrap();

    let sessions = Arc::new(MemorySessionStore::new());
    let auth = Arc::new(AuthService::new(repo.clone(), sessions));
    let jwt = Arc::new(JwtManager::hs256("ferro-test", b"dev-only-test-jwt-secret-32-bytes"));
    let state = Arc::new(AppState::new(repo, media_store, auth, jwt));
    (tmp, state, ty, content)
}

async fn login(state: Arc<AppState>) -> String {
    let body = json!({ "email": EMAIL, "password": PASSWORD }).to_string();
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
    let v = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, v)
}

#[tokio::test]
async fn update_creates_version_then_restore_reverts() {
    let (_tmp, state, _ty, _content) = fixture().await;
    let token = login(state.clone()).await;

    // PATCH content → version captured.
    let (s, _) = req_json(
        state.clone(),
        Request::builder()
            .method("PATCH")
            .uri("/api/v1/content/post/alpha")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::from(json!({ "data": { "title": "v2" } }).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // Second PATCH → another version.
    let (s, _) = req_json(
        state.clone(),
        Request::builder()
            .method("PATCH")
            .uri("/api/v1/content/post/alpha")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::from(json!({ "data": { "title": "v3" } }).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // List versions — should have 2 (snapshots before each PATCH).
    let (s, versions) = req_json(
        state.clone(),
        Request::get("/api/v1/content/post/alpha/versions").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let arr = versions.as_array().unwrap();
    assert_eq!(arr.len(), 2, "got {arr:#?}");
    // Most-recent-first ordering — first entry should be the v2 snapshot
    // (captured before the PATCH that wrote v3).
    assert_eq!(arr[0]["data"]["title"], "v2");
    assert_eq!(arr[1]["data"]["title"], "v1");

    // Restore the v1 snapshot.
    let v1_id = arr[1]["id"].as_str().unwrap();
    let (s, restored) = req_json(
        state.clone(),
        Request::post(format!("/api/v1/content/post/alpha/versions/{v1_id}/restore"))
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "restore: {restored}");
    assert_eq!(restored["data"]["title"], "v1");

    // Restore itself snapshots the prior live state — so the version list
    // should now have 3 entries.
    let (_s, versions) = req_json(
        state.clone(),
        Request::get("/api/v1/content/post/alpha/versions").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(versions.as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn restore_rejects_cross_content_id() {
    let (_tmp, state, ty, content) = fixture().await;
    let token = login(state.clone()).await;

    // Manually insert a version belonging to a different fake content_id.
    use ferro_core::ContentVersion;
    let stranger = ContentVersion {
        id: ferro_core::ContentVersionId::new(),
        content_id: ContentId::new(),
        site_id: content.site_id,
        type_id: ty.id,
        slug: "alpha".into(),
        locale: Locale::default(),
        status: Status::Draft,
        data: BTreeMap::new(),
        author_id: None,
        captured_at: OffsetDateTime::now_utc(),
        parent_version: None,
    };
    state.repo.versions().create(stranger.clone()).await.unwrap();

    let (s, _) = req_json(
        state,
        Request::post(format!("/api/v1/content/post/alpha/versions/{}/restore", stranger.id))
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}
