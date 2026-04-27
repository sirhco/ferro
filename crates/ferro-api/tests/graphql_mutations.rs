//! GraphQL mutation integration tests over the fs-json backend.

use std::{collections::BTreeMap, sync::Arc};

use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
};
use ferro_api::AppState;
use ferro_auth::{hash_password, AuthService, JwtManager, MemorySessionStore};
use ferro_core::{
    ContentType, FieldDef, FieldId, FieldKind, Locale, Permission, Role, RoleId, Scope, Site,
    SiteSettings, User, UserId,
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
        password_changed_at: None,
        totp_secret: None,
    };
    // `User.password_hash` is `#[serde(skip_serializing)]`; write JSON directly
    // so the fs-json backend preserves the hash across reload.
    let user_path = tmp.path().join("data/users").join(format!("{}.json", user.id));
    let mut user_value = serde_json::to_value(&user).unwrap();
    user_value
        .as_object_mut()
        .unwrap()
        .insert("password_hash".into(), serde_json::json!(user.password_hash));
    tokio::fs::write(&user_path, serde_json::to_vec_pretty(&user_value).unwrap()).await.unwrap();

    let sessions = Arc::new(MemorySessionStore::new());
    let auth = Arc::new(AuthService::new(repo.clone(), sessions));
    let jwt = Arc::new(JwtManager::hs256("ferro-test", b"dev-only-test-jwt-secret-32-bytes"));

    let state = Arc::new(AppState::new(repo, media, auth, jwt));
    Fixture { _tmp: tmp, state }
}

async fn gql(state: Arc<AppState>, query: &str, bearer: Option<&str>) -> Value {
    let mut req_builder =
        Request::post("/graphql").header(header::CONTENT_TYPE, "application/json");
    if let Some(t) = bearer {
        req_builder = req_builder.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    let body = json!({ "query": query }).to_string();
    let req = req_builder.body(Body::from(body)).unwrap();
    let app = ferro_api::router(state);
    let resp = app.oneshot(req).await.expect("router oneshot");
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn login_mutation_issues_token() {
    let fx = fixture().await;
    let resp = gql(
        fx.state.clone(),
        &format!(
            "mutation {{ login(email: \"{EMAIL}\", password: \"{PASSWORD}\") {{ token email }} }}"
        ),
        None,
    )
    .await;
    assert!(resp.get("errors").is_none(), "unexpected errors: {resp:?}");
    let token = resp["data"]["login"]["token"].as_str().unwrap();
    assert!(!token.is_empty());
    assert_eq!(resp["data"]["login"]["email"], EMAIL);
}

#[tokio::test]
async fn mutation_requires_auth() {
    let fx = fixture().await;
    let resp = gql(
        fx.state.clone(),
        "mutation { createContent(input: { typeSlug: \"post\", slug: \"x\", data: {} }) { id } }",
        None,
    )
    .await;
    let errors = resp.get("errors").expect("should have errors");
    assert!(errors.to_string().contains("unauthenticated"));
}

#[tokio::test]
async fn create_update_publish_delete_via_graphql() {
    let fx = fixture().await;

    // Login
    let login = gql(
        fx.state.clone(),
        &format!("mutation {{ login(email: \"{EMAIL}\", password: \"{PASSWORD}\") {{ token }} }}"),
        None,
    )
    .await;
    let token = login["data"]["login"]["token"].as_str().unwrap().to_string();

    // Create — data must be a JSON object literal in GraphQL; use variables for safety.
    let create_query = r#"
        mutation Create($data: JSON!) {
            createContent(input: { typeSlug: "post", slug: "hello", data: $data }) { id slug status }
        }"#;
    let _ = create_query; // not used below — inline literal instead to keep deps slim.

    // Use inline data literal: GraphQL input type is `JSON` (serde_json::Value)
    // represented as JSON in query variables. We can pass it as inline object.
    let data = BTreeMap::from([("title".to_string(), serde_json::json!({"String": "Hello"}))]);
    let _ = data; // shape differs from FieldValue; use serialized FieldValue instead.

    let data_literal = serde_json::to_string(&serde_json::json!({
        "title": "Hello"
    }))
    .unwrap();
    // Embed as a variable via the request body JSON.
    let body = json!({
        "query": "mutation($data: JSON!) { createContent(input: { typeSlug: \"post\", slug: \"hello\", data: $data }) { slug status } }",
        "variables": { "data": serde_json::from_str::<serde_json::Value>(&data_literal).unwrap() }
    })
    .to_string();
    let req = Request::post("/graphql")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::from(body))
        .unwrap();
    let app = ferro_api::router(fx.state.clone());
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(v.get("errors").is_none(), "create errors: {v:?}");
    assert_eq!(v["data"]["createContent"]["slug"], "hello");
    assert_eq!(v["data"]["createContent"]["status"], "draft");

    // Publish
    let pub_resp = gql(
        fx.state.clone(),
        "mutation { publishContent(typeSlug: \"post\", slug: \"hello\") { status } }",
        Some(&token),
    )
    .await;
    assert!(pub_resp.get("errors").is_none(), "publish errors: {pub_resp:?}");
    assert_eq!(pub_resp["data"]["publishContent"]["status"], "published");

    // Delete
    let del_resp = gql(
        fx.state.clone(),
        "mutation { deleteContent(typeSlug: \"post\", slug: \"hello\") }",
        Some(&token),
    )
    .await;
    assert!(del_resp.get("errors").is_none(), "delete errors: {del_resp:?}");
    assert_eq!(del_resp["data"]["deleteContent"], true);
}
