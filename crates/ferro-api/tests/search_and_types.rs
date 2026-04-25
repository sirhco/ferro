//! Content search query + content-type CRUD over REST.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use ferro_api::AppState;
use ferro_auth::{hash_password, AuthService, JwtManager, MemorySessionStore};
use ferro_core::{
    Content, ContentId, ContentType, ContentTypeId, FieldDef, FieldId, FieldKind, FieldValue,
    Locale, Permission, Role, RoleId, Site, SiteSettings, Status, User, UserId,
};
use ferro_media::MediaConfig;
use ferro_storage::StorageConfig;
use serde_json::{json, Value};
use time::OffsetDateTime;
use tower::ServiceExt;

const ADMIN_EMAIL: &str = "admin@example.com";
const ADMIN_PASSWORD: &str = "correct-horse-battery-staple";

async fn fixture() -> (tempfile::TempDir, Arc<AppState>, Site, ContentType) {
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

    // Seed three rows: alpha/bravo/charlie with searchable titles.
    for (slug, title) in [
        ("alpha", "Hello world"),
        ("bravo", "Goodbye galaxy"),
        ("charlie", "Hello hello"),
    ] {
        let mut data = BTreeMap::new();
        data.insert("title".into(), FieldValue::String(title.into()));
        let now = OffsetDateTime::now_utc();
        let c = Content {
            id: ContentId::new(),
            site_id: site.id,
            type_id: ty.id,
            slug: slug.into(),
            locale: Locale::default(),
            status: Status::Draft,
            data,
            author_id: None,
            created_at: now,
            updated_at: now,
            published_at: None,
        };
        repo.content().upsert(c).await.unwrap();
    }

    let admin_role = Role {
        id: RoleId::new(),
        name: "admin".into(),
        description: None,
        permissions: vec![Permission::Admin],
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
        created_at: OffsetDateTime::now_utc(),
        last_login: None,
        password_changed_at: None,
    };
    repo.users().upsert(user).await.unwrap();

    let sessions = Arc::new(MemorySessionStore::new());
    let auth = Arc::new(AuthService::new(repo.clone(), sessions));
    let jwt = Arc::new(JwtManager::hs256("ferro-test", b"dev-only-test-jwt-secret-32-bytes"));
    let state = Arc::new(AppState::new(repo, media_store, auth, jwt));
    (tmp, state, site, ty)
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

#[tokio::test]
async fn list_content_with_q_filters_substring() {
    let (_tmp, state, _site, _ty) = fixture().await;

    let app = ferro_api::router(state.clone());
    let resp = app
        .oneshot(
            Request::get("/api/v1/content/post?q=hello")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    let items = v["items"].as_array().unwrap();
    let slugs: Vec<&str> = items.iter().map(|x| x["slug"].as_str().unwrap()).collect();
    assert_eq!(slugs.len(), 2, "expected alpha + charlie, got {slugs:?}");
    assert!(slugs.contains(&"alpha") && slugs.contains(&"charlie"));

    // Empty match
    let app = ferro_api::router(state);
    let resp = app
        .oneshot(
            Request::get("/api/v1/content/post?q=zzz_no_match")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["items"].as_array().unwrap().len(), 0);
    assert_eq!(v["total"], 0);
}

#[tokio::test]
async fn type_crud_round_trip_via_rest() {
    let (_tmp, state, site, _) = fixture().await;
    let token = login(state.clone()).await;

    let now_rfc = OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    // Typed ids use `#[serde(transparent)]` over `Ulid`, so the wire shape is
    // the bare 26-char Crockford string — not the prefixed Display form.
    let new_ty = json!({
        "id": ContentTypeId::new().0.to_string(),
        "site_id": site.id.0.to_string(),
        "slug": "page",
        "name": "Page",
        "description": "static pages",
        "fields": [{
            "id": FieldId::new().0.to_string(),
            "slug": "headline",
            "name": "Headline",
            "kind": { "type": "text", "multiline": false }
        }],
        "singleton": false,
        "title_field": "headline",
        "slug_field": null,
        "created_at": now_rfc,
        "updated_at": now_rfc
    });

    let app = ferro_api::router(state.clone());
    let resp = app
        .oneshot(
            Request::post("/api/v1/types")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::from(new_ty.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let body = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
    let body_str = std::str::from_utf8(&body).unwrap_or("(non-utf8)");
    assert_eq!(status, StatusCode::OK, "create body: {body_str}");
    let saved: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(saved["slug"], "page");

    // List should now contain `post` + `page`.
    let app = ferro_api::router(state.clone());
    let resp = app
        .oneshot(
            Request::get("/api/v1/types")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
    let arr: Value = serde_json::from_slice(&bytes).unwrap();
    let slugs: Vec<&str> =
        arr.as_array().unwrap().iter().map(|t| t["slug"].as_str().unwrap()).collect();
    assert!(slugs.contains(&"page"));

    // Delete `page`.
    let app = ferro_api::router(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/types/page")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}
