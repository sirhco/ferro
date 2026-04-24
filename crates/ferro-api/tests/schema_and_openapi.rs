//! Schema-migration round-trip + OpenAPI smoke test.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use ferro_api::AppState;
use ferro_auth::{hash_password, AuthService, JwtManager, MemorySessionStore};
use ferro_core::{
    ContentType, FieldDef, FieldId, FieldKind, FieldValue, Locale, Permission, Role, RoleId,
    Scope, Site, SiteSettings, Status, User, UserId,
};
use ferro_media::MediaConfig;
use ferro_storage::{schema as schema_migrator, StorageConfig};
use serde_json::Value;
use time::OffsetDateTime;
use tower::ServiceExt;

const EMAIL: &str = "admin@example.com";
const PASSWORD: &str = "correct-horse-battery-staple";

async fn fixture() -> (
    tempfile::TempDir,
    Arc<AppState>,
    Site,
    ContentType,
    Role,
) {
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

    let title_field_id = FieldId::new();
    let body_field_id = FieldId::new();
    let ty = ContentType {
        id: ferro_core::ContentTypeId::new(),
        site_id: site.id,
        slug: "post".into(),
        name: "Post".into(),
        description: None,
        fields: vec![
            FieldDef {
                id: title_field_id,
                slug: "title".into(),
                name: "Title".into(),
                help: None,
                kind: FieldKind::Text { multiline: false, max: Some(200) },
                required: true,
                localized: false,
                unique: false,
                hidden: false,
            },
            FieldDef {
                id: body_field_id,
                slug: "body".into(),
                name: "Body".into(),
                help: None,
                kind: FieldKind::Text { multiline: true, max: None },
                required: false,
                localized: false,
                unique: false,
                hidden: false,
            },
        ],
        singleton: false,
        title_field: Some("title".into()),
        slug_field: None,
        created_at: now,
        updated_at: now,
    };
    repo.types().upsert(ty.clone()).await.unwrap();

    let role = Role {
        id: RoleId::new(),
        name: "admin".into(),
        description: None,
        permissions: vec![Permission::ManageSchema],
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
    let user_path = tmp.path().join("data/users").join(format!("{}.json", user.id));
    let mut user_value = serde_json::to_value(&user).unwrap();
    user_value
        .as_object_mut()
        .unwrap()
        .insert("password_hash".into(), serde_json::json!(user.password_hash));
    tokio::fs::write(&user_path, serde_json::to_vec_pretty(&user_value).unwrap())
        .await
        .unwrap();

    // Seed two content rows with both fields populated.
    for slug in ["alpha", "beta"] {
        let mut data = BTreeMap::new();
        data.insert("title".into(), FieldValue::String(format!("{slug} title")));
        data.insert("body".into(), FieldValue::String(format!("{slug} body")));
        let now = OffsetDateTime::now_utc();
        let c = ferro_core::Content {
            id: ferro_core::ContentId::new(),
            site_id: site.id,
            type_id: ty.id,
            slug: slug.into(),
            locale: Locale::default(),
            status: Status::Draft,
            data,
            author_id: Some(user.id),
            created_at: now,
            updated_at: now,
            published_at: None,
        };
        repo.content().upsert(c).await.unwrap();
    }

    let sessions = Arc::new(MemorySessionStore::new());
    let auth = Arc::new(AuthService::new(repo.clone(), sessions));
    let jwt = Arc::new(JwtManager::hs256("ferro-test", b"dev-only-test-jwt-secret-32-bytes"));
    let state = Arc::new(AppState::new(repo, media_store, auth, jwt));
    (tmp, state, site, ty, role)
}

#[tokio::test]
async fn schema_migrator_renames_field_in_existing_content() {
    let (_tmp, state, site, ty, _role) = fixture().await;

    // Build new ContentType with `body` renamed to `summary` and a new `tags` field.
    let mut new_ty = ty.clone();
    let body_idx = new_ty.fields.iter().position(|f| f.slug == "body").unwrap();
    new_ty.fields[body_idx].slug = "summary".into();
    new_ty.fields.push(FieldDef {
        id: FieldId::new(),
        slug: "tags".into(),
        name: "Tags".into(),
        help: None,
        kind: FieldKind::Text { multiline: false, max: None },
        required: false,
        localized: false,
        unique: false,
        hidden: false,
    });

    let changes = ContentType::diff(&ty, &new_ty);
    assert_eq!(changes.len(), 2, "expected rename + add, got {changes:?}");

    state.repo.types().upsert(new_ty.clone()).await.unwrap();
    let touched = schema_migrator::apply_changes(&*state.repo, site.id, ty.id, &changes)
        .await
        .unwrap();
    assert_eq!(touched, 2, "two seed rows should have been migrated");

    let alpha = state
        .repo
        .content()
        .by_slug(site.id, ty.id, "alpha")
        .await
        .unwrap()
        .unwrap();
    // Old key gone
    assert!(!alpha.data.contains_key("body"));
    // New key present with original value
    match alpha.data.get("summary") {
        Some(FieldValue::String(s)) if s == "alpha body" => {}
        other => panic!("expected migrated body value, got {other:?}"),
    }
    // Added field present as Null
    match alpha.data.get("tags") {
        Some(FieldValue::Null) => {}
        other => panic!("expected Null for added field, got {other:?}"),
    }
}

#[tokio::test]
async fn openapi_endpoint_returns_valid_spec() {
    let (_tmp, state, _site, _ty, _role) = fixture().await;
    let app = ferro_api::router(state);
    let resp = app
        .oneshot(
            Request::get("/api/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["info"]["title"], "Ferro API");
    assert!(v["paths"].as_object().unwrap().contains_key("/api/v1/types"));
    assert!(v["paths"].as_object().unwrap().contains_key("/api/v1/content/{type_slug}/{slug}/publish"));
    assert!(v["components"]["securitySchemes"]["bearer"].is_object());
}

#[tokio::test]
async fn type_patch_route_runs_migrator() {
    let (_tmp, state, site, ty, _role) = fixture().await;

    // Login first.
    let login_body = serde_json::json!({ "email": EMAIL, "password": PASSWORD }).to_string();
    let app = ferro_api::router(state.clone());
    let resp = app
        .oneshot(
            Request::post("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(login_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    let token = v["token"].as_str().unwrap().to_string();

    // PATCH the type with `body` renamed to `summary`.
    let mut new_ty = ty.clone();
    let body_idx = new_ty.fields.iter().position(|f| f.slug == "body").unwrap();
    new_ty.fields[body_idx].slug = "summary".into();
    let body = serde_json::to_string(&new_ty).unwrap();

    let app2 = ferro_api::router(state.clone());
    let resp = app2
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/v1/types/{}", ty.slug))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["rows_migrated"], 2);
    assert_eq!(v["changes"][0]["op"], "renamed");

    let alpha = state
        .repo
        .content()
        .by_slug(site.id, ty.id, "alpha")
        .await
        .unwrap()
        .unwrap();
    assert!(alpha.data.contains_key("summary"));
    assert!(!alpha.data.contains_key("body"));
}
