//! GraphQL subscription end-to-end test over WebSocket.
//!
//! Drives the full transport: graphql-transport-ws subprotocol, connection_init
//! → subscribe → fire mutation → assert next event surfaces.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use ferro_api::AppState;
use ferro_auth::{hash_password, AuthService, JwtManager, MemorySessionStore};
use ferro_core::{
    ContentType, FieldDef, FieldId, FieldKind, FieldValue, Locale, NewContent, Permission, Role,
    RoleId, Scope, Site, SiteSettings, User, UserId,
};
use ferro_media::MediaConfig;
use ferro_storage::StorageConfig;
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use time::OffsetDateTime;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::protocol::Message;

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
    let state = Arc::new(AppState::new(repo, media_store, auth, jwt));
    (tmp, state, ty)
}

async fn login_token(base_url: &str) -> String {
    let body = json!({ "email": EMAIL, "password": PASSWORD });
    let resp = reqwest::Client::new()
        .post(format!("{base_url}/api/v1/auth/login"))
        .json(&body)
        .send()
        .await
        .unwrap();
    let v: Value = resp.json().await.unwrap();
    v["token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn graphql_subscription_emits_content_created() {
    let (_tmp, state, ty) = fixture().await;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base_http = format!("http://{addr}");
    let app = ferro_api::router(state.clone());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let token = login_token(&base_http).await;

    // Connect WS with the graphql-transport-ws subprotocol.
    let mut req = format!("ws://{addr}/graphql/ws").into_client_request().unwrap();
    req.headers_mut().insert(
        "Sec-WebSocket-Protocol",
        "graphql-transport-ws".parse().unwrap(),
    );
    let (mut ws, _) = tokio_tungstenite::connect_async(req).await.expect("ws connect");

    // Handshake — token goes in connection_init payload (graphql-transport-ws auth convention).
    ws.send(Message::Text(
        json!({ "type": "connection_init", "payload": { "token": token } })
            .to_string()
            .into(),
    ))
    .await
    .unwrap();
    let ack = next_text(&mut ws).await;
    assert_eq!(ack["type"], "connection_ack", "got: {ack}");

    // Subscribe
    let subscribe = json!({
        "id": "1",
        "type": "subscribe",
        "payload": {
            "query": "subscription { contentChanges { kind slug status } }"
        }
    });
    ws.send(Message::Text(subscribe.to_string().into())).await.unwrap();

    // Give the broadcast subscriber a tick to register before we fire.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Fire content create over REST. Reuse the JWT minted during the WS handshake.
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
    let resp = reqwest::Client::new()
        .post(format!("{base_http}/api/v1/content/post"))
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Read up to 5 frames, looking for a `next` event with kind=content.created.
    let mut got = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        let frame = tokio::time::timeout(Duration::from_millis(500), next_text(&mut ws)).await;
        let Ok(msg) = frame else { continue };
        if msg["type"] == "next" {
            let payload = &msg["payload"]["data"]["contentChanges"];
            if payload["kind"] == "content.created" && payload["slug"] == "alpha" {
                assert_eq!(payload["status"], "draft");
                got = true;
                break;
            }
        }
    }
    assert!(got, "did not observe content.created via subscription");

    let _ = ws.close(None).await;
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn graphql_subscription_drops_events_on_unauthorized_types() {
    let (_tmp, state, ty) = fixture().await;

    // Seed a second content type the test user has NO permission on.
    let now = OffsetDateTime::now_utc();
    let other = ContentType {
        id: ferro_core::ContentTypeId::new(),
        site_id: ty.site_id,
        slug: "secret".into(),
        name: "Secret".into(),
        description: None,
        fields: vec![],
        singleton: false,
        title_field: None,
        slug_field: None,
        created_at: now,
        updated_at: now,
    };
    state.repo.types().upsert(other.clone()).await.unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base_http = format!("http://{addr}");
    let app = ferro_api::router(state.clone());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let token = login_token(&base_http).await;

    let mut req = format!("ws://{addr}/graphql/ws").into_client_request().unwrap();
    req.headers_mut().insert(
        "Sec-WebSocket-Protocol",
        "graphql-transport-ws".parse().unwrap(),
    );
    let (mut ws, _) = tokio_tungstenite::connect_async(req).await.expect("ws connect");

    ws.send(Message::Text(
        json!({ "type": "connection_init", "payload": { "token": token } })
            .to_string()
            .into(),
    ))
    .await
    .unwrap();
    let ack = next_text(&mut ws).await;
    assert_eq!(ack["type"], "connection_ack");

    ws.send(Message::Text(
        json!({
            "id": "1",
            "type": "subscribe",
            "payload": { "query": "subscription { contentChanges { kind typeSlug } }" }
        })
        .to_string()
        .into(),
    ))
    .await
    .unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Dispatch one event the user CAN read (post) and one they cannot (secret).
    use ferro_core::{Content as Ce, ContentId as Cid, FieldValue as Fv, Locale, Status};
    use std::collections::BTreeMap as Map;
    let now = OffsetDateTime::now_utc();
    let permitted = Ce {
        id: Cid::new(),
        site_id: ty.site_id,
        type_id: ty.id,
        slug: "ok".into(),
        locale: Locale::default(),
        status: Status::Draft,
        data: {
            let mut m = Map::new();
            m.insert("title".into(), Fv::String("ok".into()));
            m
        },
        author_id: None,
        created_at: now,
        updated_at: now,
        published_at: None,
    };
    let blocked = Ce {
        id: Cid::new(),
        site_id: other.site_id,
        type_id: other.id,
        slug: "blocked".into(),
        locale: Locale::default(),
        status: Status::Draft,
        data: Map::new(),
        author_id: None,
        created_at: now,
        updated_at: now,
        published_at: None,
    };
    state
        .hooks
        .dispatch(ferro_plugin::HookEvent::ContentCreated {
            content: blocked,
            type_slug: Some(other.slug.clone()),
        })
        .await;
    state
        .hooks
        .dispatch(ferro_plugin::HookEvent::ContentCreated {
            content: permitted,
            type_slug: Some(ty.slug.clone()),
        })
        .await;

    // The very first content.created we observe must be the permitted one;
    // the blocked event should never surface, even though it was dispatched
    // first.
    let mut saw_permitted = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while std::time::Instant::now() < deadline {
        let frame = tokio::time::timeout(Duration::from_millis(500), next_text(&mut ws)).await;
        let Ok(msg) = frame else { continue };
        if msg["type"] == "next" {
            let payload = &msg["payload"]["data"]["contentChanges"];
            assert_ne!(
                payload["typeSlug"], "secret",
                "leaked event for unauthorized type: {payload}"
            );
            if payload["kind"] == "content.created" && payload["typeSlug"] == "post" {
                saw_permitted = true;
                break;
            }
        }
    }
    assert!(saw_permitted, "did not receive the permitted event");

    let _ = ws.close(None).await;
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn graphql_subscription_rejects_missing_token() {
    let (_tmp, state, _ty) = fixture().await;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = ferro_api::router(state);
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let mut req = format!("ws://{addr}/graphql/ws").into_client_request().unwrap();
    req.headers_mut().insert(
        "Sec-WebSocket-Protocol",
        "graphql-transport-ws".parse().unwrap(),
    );
    let (mut ws, _) = tokio_tungstenite::connect_async(req).await.expect("ws connect");

    ws.send(Message::Text(
        json!({ "type": "connection_init", "payload": {} }).to_string().into(),
    ))
    .await
    .unwrap();

    // The server should refuse the handshake — either via a `connection_error`
    // frame or by closing the socket with a 4xxx code.
    let mut rejected = false;
    for _ in 0..5 {
        match tokio::time::timeout(Duration::from_millis(500), ws.next()).await {
            Ok(Some(Ok(Message::Close(_)))) => {
                rejected = true;
                break;
            }
            Ok(Some(Ok(Message::Text(t)))) => {
                let v: Value = serde_json::from_str(&t).unwrap();
                if v["type"] == "connection_error" {
                    rejected = true;
                    break;
                }
                if v["type"] == "connection_ack" {
                    panic!("server accepted unauthenticated subscription: {v}");
                }
            }
            Ok(Some(Ok(_))) => continue,
            Ok(Some(Err(_))) | Ok(None) => {
                rejected = true;
                break;
            }
            Err(_) => continue,
        }
    }
    assert!(rejected, "expected handshake to fail without token");

    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn graphql_subscription_rejects_invalid_token() {
    let (_tmp, state, _ty) = fixture().await;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = ferro_api::router(state);
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let mut req = format!("ws://{addr}/graphql/ws").into_client_request().unwrap();
    req.headers_mut().insert(
        "Sec-WebSocket-Protocol",
        "graphql-transport-ws".parse().unwrap(),
    );
    let (mut ws, _) = tokio_tungstenite::connect_async(req).await.expect("ws connect");

    ws.send(Message::Text(
        json!({ "type": "connection_init", "payload": { "token": "garbage" } })
            .to_string()
            .into(),
    ))
    .await
    .unwrap();

    let mut rejected = false;
    for _ in 0..5 {
        match tokio::time::timeout(Duration::from_millis(500), ws.next()).await {
            Ok(Some(Ok(Message::Close(_)))) => {
                rejected = true;
                break;
            }
            Ok(Some(Ok(Message::Text(t)))) => {
                let v: Value = serde_json::from_str(&t).unwrap();
                if v["type"] == "connection_error" {
                    rejected = true;
                    break;
                }
                if v["type"] == "connection_ack" {
                    panic!("server accepted bogus token: {v}");
                }
            }
            Ok(Some(Ok(_))) => continue,
            Ok(Some(Err(_))) | Ok(None) => {
                rejected = true;
                break;
            }
            Err(_) => continue,
        }
    }
    assert!(rejected, "expected handshake to fail with invalid token");

    server.abort();
    let _ = server.await;
}

async fn next_text(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> Value {
    loop {
        let msg = ws.next().await.expect("ws eof").expect("ws err");
        match msg {
            Message::Text(t) => return serde_json::from_str(&t).unwrap(),
            Message::Ping(_) | Message::Pong(_) => continue,
            Message::Binary(_) => continue,
            Message::Close(_) => panic!("server closed connection"),
            Message::Frame(_) => continue,
        }
    }
}
