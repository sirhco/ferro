//! SSE live-preview endpoint test.
//!
//! Strategy: bind a real TCP listener on a random port, fire a content
//! mutation, then read the SSE stream via reqwest and assert the event
//! payload arrives. We can't use `oneshot` here because SSE needs an
//! actual long-lived TCP connection.

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use ferro_api::AppState;
use ferro_auth::{hash_password, AuthService, JwtManager, MemorySessionStore};
use ferro_core::{
    ContentType, FieldDef, FieldId, FieldKind, FieldValue, Locale, NewContent, Permission, Role,
    RoleId, Scope, Site, SiteSettings, User, UserId,
};
use ferro_media::MediaConfig;
use ferro_storage::StorageConfig;
use serde_json::Value;
use time::OffsetDateTime;

const EMAIL: &str = "admin@example.com";
const PASSWORD: &str = "correct-horse-battery-staple";

async fn fixture() -> (tempfile::TempDir, Arc<AppState>, ContentType) {
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
        permissions: vec![Permission::Write(Scope::Type { id: ty.id })],
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
    let state = Arc::new(AppState::new(repo, media_store, auth, jwt));
    (tmp, state, ty)
}

async fn login_token(state: Arc<AppState>, base_url: &str) -> String {
    let body = serde_json::json!({ "email": EMAIL, "password": PASSWORD });
    let resp = reqwest::Client::new()
        .post(format!("{base_url}/api/v1/auth/login"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let v: Value = resp.json().await.unwrap();
    let _ = state;
    v["token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn sse_streams_content_created_event() {
    let (_tmp, state, ty) = fixture().await;

    // Start a real server on an ephemeral port so we can drive SSE over TCP.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base = format!("http://{addr}");
    let app = ferro_api::router(state.clone());

    // Spawn the server. Drop the join handle on test exit; tempdir cleanup
    // happens when `_tmp` falls out of scope.
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let token = login_token(state.clone(), &base).await;

    // Open the SSE stream first so the registry has a subscriber by the time
    // the mutation fires.
    let sse_url = format!("{base}/api/v1/events?token={token}");
    let resp = reqwest::Client::new().get(&sse_url).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    assert!(resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("text/event-stream"));

    // Wait briefly so the broadcast subscription is registered before we fire.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Fire a content create.
    let body = NewContent {
        type_id: ty.id,
        slug: "alpha".into(),
        locale: Locale::default(),
        data: {
            let mut m = BTreeMap::new();
            m.insert("title".into(), FieldValue::String("Alpha".into()));
            m
        },
        author_id: None,
    };
    let create_resp = reqwest::Client::new()
        .post(format!("{base}/api/v1/content/post"))
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(create_resp.status(), 200, "create body: {:?}", create_resp.text().await);

    // Read SSE frames until we see a `content.created` event or time out.
    let mut stream = resp.bytes_stream();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut buf = String::new();
    let mut got_event = false;
    use futures::StreamExt;
    while std::time::Instant::now() < deadline {
        let next = tokio::time::timeout(Duration::from_millis(500), stream.next()).await;
        let Ok(Some(chunk)) = next else { continue };
        let chunk = chunk.unwrap();
        buf.push_str(std::str::from_utf8(&chunk).unwrap());
        if buf.contains("event: content.created") {
            got_event = true;
            break;
        }
    }
    assert!(got_event, "did not observe content.created in SSE stream; got: {buf}");

    // Parse the data line and verify the slug round-tripped.
    let data_line = buf.lines().find(|l| l.starts_with("data:")).expect("data line present");
    let json: Value = serde_json::from_str(data_line.trim_start_matches("data: ")).unwrap();
    assert_eq!(json["kind"], "content_created");
    assert_eq!(json["content"]["slug"], "alpha");

    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn sse_rejects_unauthenticated_request() {
    let (_tmp, state, _ty) = fixture().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base = format!("http://{addr}");
    let app = ferro_api::router(state);
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let resp = reqwest::Client::new().get(format!("{base}/api/v1/events")).send().await.unwrap();
    assert_eq!(resp.status(), 401);

    server.abort();
    let _ = server.await;
}
