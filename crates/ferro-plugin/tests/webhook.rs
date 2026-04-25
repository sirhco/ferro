//! End-to-end webhook delivery + signature verification.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::post;
use axum::Router;
use ferro_plugin::{webhook::sign, HookEvent, HookRegistry, WebhookConfig, WebhookHook};
use serde_json::Value;

#[derive(Default, Clone)]
struct Captured {
    inner: Arc<Mutex<Vec<(HeaderMap, Vec<u8>)>>>,
}

#[tokio::test]
async fn webhook_delivers_signed_event() {
    let captured = Captured::default();
    let cap_for_handler = captured.clone();

    let app = Router::new()
        .route(
            "/hook",
            post(
                |State(c): State<Captured>, headers: HeaderMap, body: Bytes| async move {
                    c.inner.lock().unwrap().push((headers, body.to_vec()));
                    "ok"
                },
            ),
        )
        .with_state(cap_for_handler);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let secret = "shhh-this-is-a-test-secret";
    let cfg = WebhookConfig {
        name: Some("test".into()),
        url: format!("http://{addr}/hook"),
        secret: Some(secret.into()),
        events: vec!["content.created".into()],
        timeout_secs: 5,
    };
    let hook = Arc::new(WebhookHook::new(cfg).unwrap());

    let registry = HookRegistry::new();
    registry.register(hook.clone()).await;

    // Dispatch one matching event + one filtered-out event.
    let now = time::OffsetDateTime::now_utc();
    let permitted = ferro_core::Content {
        id: ferro_core::ContentId::new(),
        site_id: ferro_core::SiteId::new(),
        type_id: ferro_core::ContentTypeId::new(),
        slug: "alpha".into(),
        locale: ferro_core::Locale::default(),
        status: ferro_core::Status::Draft,
        data: std::collections::BTreeMap::new(),
        author_id: None,
        created_at: now,
        updated_at: now,
        published_at: None,
    };
    registry
        .dispatch(HookEvent::ContentCreated {
            content: permitted,
            type_slug: Some("post".into()),
        })
        .await;
    registry
        .dispatch(HookEvent::ContentDeleted {
            site_id: ferro_core::SiteId::new(),
            type_id: ferro_core::ContentTypeId::new(),
            content_id: ferro_core::ContentId::new(),
            slug: "removed".into(),
            type_slug: Some("post".into()),
        })
        .await;

    // Allow one tick for the async dispatch to flush.
    tokio::time::sleep(Duration::from_millis(100)).await;

    let captured = captured.inner.lock().unwrap();
    assert_eq!(captured.len(), 1, "filter should drop content.deleted");
    let (headers, raw) = &captured[0];
    assert_eq!(
        headers.get("x-ferro-event").unwrap().to_str().unwrap(),
        "content.created"
    );
    let parsed: Value = serde_json::from_slice(raw).unwrap();
    assert_eq!(parsed["kind"], "content_created");

    // Verify signature against the bytes we received on the wire.
    let got = headers.get("x-ferro-signature").unwrap().to_str().unwrap();
    let expected = sign(secret.as_bytes(), raw);
    assert_eq!(got, expected, "signature mismatch");

    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn webhook_without_filter_delivers_everything() {
    let captured = Captured::default();
    let cap_for_handler = captured.clone();

    let app = Router::new()
        .route(
            "/hook",
            post(
                |State(c): State<Captured>, _h: HeaderMap, body: Bytes| async move {
                    c.inner.lock().unwrap().push((HeaderMap::new(), body.to_vec()));
                    "ok"
                },
            ),
        )
        .with_state(cap_for_handler);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let cfg = WebhookConfig {
        name: None,
        url: format!("http://{addr}/hook"),
        secret: None,
        events: Vec::new(),
        timeout_secs: 5,
    };
    let hook = Arc::new(WebhookHook::new(cfg).unwrap());
    let registry = HookRegistry::new();
    registry.register(hook).await;

    registry
        .dispatch(HookEvent::ContentDeleted {
            site_id: ferro_core::SiteId::new(),
            type_id: ferro_core::ContentTypeId::new(),
            content_id: ferro_core::ContentId::new(),
            slug: "x".into(),
            type_slug: Some("post".into()),
        })
        .await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(captured.inner.lock().unwrap().len(), 1);

    server.abort();
    let _ = server.await;
}
