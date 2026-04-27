//! End-to-end WASM plugin dispatch.
//!
//! Loads the `examples/plugin-hello` component, fires a `ContentPublished`
//! event through the [`HookRegistry`], and asserts the plugin's `host::log`
//! call lands in a `tracing` capture layer.
//!
//! Gated on `WASM_TESTS=1` so default CI (without the `wasm32-wasip2` target
//! installed) skips silently. Run locally with:
//!
//! ```sh
//! rustup target add wasm32-wasip2
//! cargo build --manifest-path examples/plugin-hello/Cargo.toml \
//!     --release --target wasm32-wasip2
//! WASM_TESTS=1 cargo test -p ferro-plugin --test wasm_dispatch
//! ```

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use ferro_plugin::{
    HookEvent, HookRegistry, PluginGrant, PluginRegistry, PluginRuntime, RuntimeConfig, Services,
};
use ferro_storage::StorageConfig;
use tracing::Subscriber;
use tracing_subscriber::layer::{Context as LayerContext, Layer, SubscriberExt};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wasm_plugin_observes_published_event() {
    if std::env::var("WASM_TESTS").as_deref() != Ok("1") {
        eprintln!("WASM_TESTS != 1, skipping wasm_dispatch test");
        return;
    }

    let plugin_wasm = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples/plugin-hello/target/wasm32-wasip2/release/plugin_hello.wasm");
    assert!(
        plugin_wasm.exists(),
        "plugin not built — run `cargo build --manifest-path examples/plugin-hello/Cargo.toml --release --target wasm32-wasip2` (looked at {})",
        plugin_wasm.display()
    );

    // Capture every tracing event into a Mutex so we can inspect after dispatch.
    let sink: Arc<Mutex<Vec<String>>> = Arc::default();
    let layer = CaptureLayer { sink: sink.clone() };
    let subscriber = tracing_subscriber::registry().with(layer);
    let _guard = tracing::subscriber::set_default(subscriber);

    // Stage a plugin dir + storage root under a tempdir.
    let tmp = tempfile::tempdir().unwrap();
    let plugins_dir = tmp.path().join("plugins");
    let plugin_dir = plugins_dir.join("hello");
    tokio::fs::create_dir_all(&plugin_dir).await.unwrap();
    tokio::fs::copy(&plugin_wasm, plugin_dir.join("plugin.wasm")).await.unwrap();
    tokio::fs::write(
        plugin_dir.join("plugin.toml"),
        b"name = \"hello\"\nversion = \"0.1.0\"\nentry = \"plugin.wasm\"\ncapabilities = [\"logs\"]\nhooks = [\"content.published\"]\n",
    )
    .await
    .unwrap();

    let storage_dir = tmp.path().join("data");
    tokio::fs::create_dir_all(&storage_dir).await.unwrap();
    let repo: Arc<dyn ferro_storage::Repository> = Arc::from(
        ferro_storage::connect(&StorageConfig::FsJson { path: storage_dir }).await.unwrap(),
    );
    repo.migrate().await.unwrap();

    let hooks = HookRegistry::new();
    let services = Arc::new(Services::new(repo.clone(), hooks.clone()));
    let runtime = PluginRuntime::new(RuntimeConfig::default(), services).unwrap();
    let grants = vec![PluginGrant { name: "hello".into(), capabilities: vec!["logs".into()] }];
    let registry = PluginRegistry::new(runtime, plugins_dir, hooks.clone(), &grants);
    registry.scan().await.unwrap();

    let info = registry.describe("hello").await.expect("hello loaded");
    assert_eq!(info.granted, vec!["logs".to_string()]);
    assert_eq!(info.hooks, vec!["content.published".to_string()]);

    // Fire a ContentPublished event — the plugin should log "published marker-7".
    let now = time::OffsetDateTime::now_utc();
    let content = ferro_core::Content {
        id: ferro_core::ContentId::new(),
        site_id: ferro_core::SiteId::new(),
        type_id: ferro_core::ContentTypeId::new(),
        slug: "marker-7".into(),
        locale: ferro_core::Locale::default(),
        status: ferro_core::Status::Published,
        data: Default::default(),
        author_id: None,
        created_at: now,
        updated_at: now,
        published_at: Some(now),
    };
    let evt = HookEvent::ContentPublished { content, type_slug: Some("post".into()) };
    hooks.dispatch(evt).await;

    let captured = sink.lock().unwrap().clone();
    assert!(
        captured.iter().any(|s| s.contains("published marker-7")),
        "didn't see plugin log; captured = {captured:#?}"
    );
}

#[derive(Clone)]
struct CaptureLayer {
    sink: Arc<Mutex<Vec<String>>>,
}

impl<S: Subscriber> Layer<S> for CaptureLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: LayerContext<'_, S>) {
        let mut v = MessageVisitor::default();
        event.record(&mut v);
        if !v.0.is_empty() {
            self.sink.lock().unwrap().push(v.0);
        }
    }
}

#[derive(Default)]
struct MessageVisitor(String);

impl tracing::field::Visit for MessageVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.0.push_str(value);
        } else {
            if !self.0.is_empty() {
                self.0.push(' ');
            }
            self.0.push_str(field.name());
            self.0.push('=');
            self.0.push_str(value);
        }
    }
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            use std::fmt::Write;
            let _ = write!(self.0, "{value:?}");
        }
    }
}
