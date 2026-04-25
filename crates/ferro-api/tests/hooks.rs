//! End-to-end test: REST mutations fire HookEvents through the registry.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use ferro_api::AppState;
use ferro_auth::{hash_password, AuthService, JwtManager, MemorySessionStore};
use ferro_core::{
    ContentType, FieldDef, FieldId, FieldKind, Locale, NewContent, Permission, Role, RoleId,
    Scope, Site, SiteSettings, User, UserId,
};
use ferro_media::MediaConfig;
use ferro_plugin::{HookEvent, HookHandler, HookRegistry};
use ferro_storage::StorageConfig;
use serde_json::Value;
use time::OffsetDateTime;
use tokio::sync::Mutex;
use tower::ServiceExt;

const EMAIL: &str = "admin@example.com";
const PASSWORD: &str = "correct-horse-battery-staple";

#[derive(Debug, Default)]
struct RecordingHook {
    events: Mutex<Vec<HookEvent>>,
}

#[async_trait]
impl HookHandler for RecordingHook {
    async fn handle(&self, event: &HookEvent) -> ferro_plugin::PluginResult<()> {
        self.events.lock().await.push(event.clone());
        Ok(())
    }
    fn name(&self) -> &str {
        "recording"
    }
}

async fn fixture() -> (tempfile::TempDir, Arc<AppState>, Arc<RecordingHook>, ContentType) {
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

    let sessions = Arc::new(MemorySessionStore::new());
    let auth = Arc::new(AuthService::new(repo.clone(), sessions));
    let jwt = Arc::new(JwtManager::hs256("ferro-test", b"dev-only-test-jwt-secret-32-bytes"));

    let recorder = Arc::new(RecordingHook::default());
    let hooks = HookRegistry::new();
    hooks.register(recorder.clone()).await;

    let state = Arc::new(AppState::with_hooks(repo, media_store, auth, jwt, hooks));
    (tmp, state, recorder, ty)
}

async fn login(state: Arc<AppState>) -> String {
    let body = serde_json::json!({ "email": EMAIL, "password": PASSWORD }).to_string();
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
async fn rest_mutations_emit_hook_events_in_order() {
    let (_tmp, state, recorder, ty) = fixture().await;
    let token = login(state.clone()).await;

    // Create
    let new = NewContent {
        type_id: ty.id,
        slug: "alpha".into(),
        locale: Locale::default(),
        data: {
            let mut m = BTreeMap::new();
            m.insert("title".into(), ferro_core::FieldValue::String("Alpha".into()));
            m
        },
        author_id: None,
    };
    let app = ferro_api::router(state.clone());
    let resp = app
        .oneshot(
            Request::post("/api/v1/content/post")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::from(serde_json::to_vec(&new).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Patch
    let app = ferro_api::router(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/content/post/alpha")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::from(
                    serde_json::json!({ "data": { "title": "Alpha v2" } }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Publish
    let app = ferro_api::router(state.clone());
    let resp = app
        .oneshot(
            Request::post("/api/v1/content/post/alpha/publish")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Delete
    let app = ferro_api::router(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/content/post/alpha")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Assert recorded events
    let events = recorder.events.lock().await;
    assert_eq!(events.len(), 4, "got {events:?}");
    assert!(matches!(events[0], HookEvent::ContentCreated { .. }));
    assert!(matches!(events[1], HookEvent::ContentUpdated { .. }));
    assert!(matches!(events[2], HookEvent::ContentPublished { .. }));
    assert!(matches!(events[3], HookEvent::ContentDeleted { .. }));
}

#[tokio::test]
async fn type_patch_emits_type_migrated_event() {
    let (_tmp, state, recorder, ty) = fixture().await;

    // Grant ManageSchema to the seeded user (replace role permissions).
    let users = state.repo.users().list().await.unwrap();
    let role_id = users[0].roles[0];
    let mut role = state.repo.users().get_role(role_id).await.unwrap().unwrap();
    role.permissions.push(Permission::ManageSchema);
    state.repo.users().upsert_role(role).await.unwrap();

    let token = login(state.clone()).await;

    // Patch the type to rename the title field.
    let mut new_ty = ty.clone();
    new_ty.fields[0].slug = "headline".into();
    let body = serde_json::to_string(&new_ty).unwrap();

    let app = ferro_api::router(state.clone());
    let resp = app
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

    let events = recorder.events.lock().await;
    assert_eq!(events.len(), 1);
    match &events[0] {
        HookEvent::TypeMigrated { changes, .. } => {
            assert_eq!(changes.len(), 1);
        }
        other => panic!("expected TypeMigrated, got {other:?}"),
    }
}
