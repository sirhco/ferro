//! In-process plugin hook system.
//!
//! Hooks fire synchronously after a successful mutation. The wasmtime-backed
//! plugin host (see [`crate::runtime`]) will eventually drive these handlers
//! across the WIT boundary, but the dispatcher itself is independent of the
//! transport — Rust-side code can register handlers directly.
//!
//! ## Failure semantics
//!
//! Handler errors are logged via `tracing` but do not fail the originating
//! request. Hooks describe events that already happened; turning them into
//! request-level failures would couple side-channels to user-facing latency.

use std::sync::Arc;

use async_trait::async_trait;
use ferro_core::{Content, ContentId, ContentTypeId, FieldChange, SiteId};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, RwLock};

use crate::error::PluginResult;

/// Capacity of the in-process broadcast bus. Subscribers that fall this far
/// behind get a `RecvError::Lagged` and skip ahead — preferring fresh events
/// over stale ones for live-preview clients.
const BUS_CAPACITY: usize = 256;

/// Discrete events emitted by the storage / API layer. New variants are added
/// by appending — handlers should treat unknown variants as no-ops via the
/// non-exhaustive marker.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HookEvent {
    /// A new content row was successfully created.
    ContentCreated { content: Content },
    /// An existing content row was updated. `before` / `after` capture the
    /// observable change so handlers can diff field-by-field.
    ContentUpdated { before: Box<Content>, after: Box<Content> },
    /// A content row transitioned to `Status::Published`.
    ContentPublished { content: Content },
    /// A content row was deleted. Carries identity only — body is gone.
    ContentDeleted {
        site_id: SiteId,
        type_id: ContentTypeId,
        content_id: ContentId,
        slug: String,
    },
    /// A content type's schema changed and the migrator finished applying
    /// `changes` to existing rows.
    TypeMigrated {
        site_id: SiteId,
        type_id: ContentTypeId,
        rows_migrated: u64,
        changes: Vec<FieldChange>,
    },
}

#[async_trait]
pub trait HookHandler: Send + Sync + std::fmt::Debug {
    /// Handle one event. Errors are logged by the dispatcher and discarded.
    async fn handle(&self, event: &HookEvent) -> PluginResult<()>;

    /// Human-readable name, used in trace output.
    fn name(&self) -> &str;
}

/// Thread-safe registry of hook handlers + a broadcast bus for live-preview
/// subscribers. Cheap to clone (`Arc` internals).
#[derive(Debug, Clone)]
pub struct HookRegistry {
    handlers: Arc<RwLock<Vec<Arc<dyn HookHandler>>>>,
    bus: broadcast::Sender<HookEvent>,
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl HookRegistry {
    #[must_use]
    pub fn new() -> Self {
        let (bus, _) = broadcast::channel(BUS_CAPACITY);
        Self { handlers: Arc::default(), bus }
    }

    pub async fn register(&self, handler: Arc<dyn HookHandler>) {
        self.handlers.write().await.push(handler);
    }

    /// Snapshot the current handler list. Used by the dispatcher so a handler
    /// can register/deregister mid-event without deadlocking.
    pub async fn snapshot(&self) -> Vec<Arc<dyn HookHandler>> {
        self.handlers.read().await.clone()
    }

    /// Subscribe to the live event bus. Receivers that fall behind get
    /// `RecvError::Lagged` and should skip ahead; the channel keeps the most
    /// recent `BUS_CAPACITY` events.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<HookEvent> {
        self.bus.subscribe()
    }

    /// Number of active subscribers. Useful for diagnostics.
    #[must_use]
    pub fn subscriber_count(&self) -> usize {
        self.bus.receiver_count()
    }

    /// Fire `event` to every registered handler **and** to the broadcast bus.
    /// Handler errors are logged at WARN and discarded so the calling write
    /// path is never blocked by a hook. Bus send errors (no subscribers) are
    /// silent — the bus is fire-and-forget.
    pub async fn dispatch(&self, event: HookEvent) {
        let handlers = self.snapshot().await;
        for h in handlers {
            if let Err(e) = h.handle(&event).await {
                tracing::warn!(
                    target: "ferro::hook",
                    handler = h.name(),
                    error = %e,
                    "hook handler failed"
                );
            }
        }
        // `send` returns `Err` only when there are no subscribers — that's the
        // normal case when nobody is watching, so swallow it.
        let _ = self.bus.send(event);
    }
}

/// Built-in handler that traces every event at INFO. Useful as a default and
/// as a smoke test that the dispatch chain works end-to-end.
#[derive(Debug, Default)]
pub struct LoggingHook;

#[async_trait]
impl HookHandler for LoggingHook {
    async fn handle(&self, event: &HookEvent) -> PluginResult<()> {
        match event {
            HookEvent::ContentCreated { content } => {
                tracing::info!(
                    target: "ferro::hook",
                    content_id = %content.id,
                    slug = %content.slug,
                    "content created"
                );
            }
            HookEvent::ContentUpdated { after, .. } => {
                tracing::info!(
                    target: "ferro::hook",
                    content_id = %after.id,
                    slug = %after.slug,
                    "content updated"
                );
            }
            HookEvent::ContentPublished { content } => {
                tracing::info!(
                    target: "ferro::hook",
                    content_id = %content.id,
                    slug = %content.slug,
                    "content published"
                );
            }
            HookEvent::ContentDeleted { content_id, slug, .. } => {
                tracing::info!(
                    target: "ferro::hook",
                    content_id = %content_id,
                    slug = %slug,
                    "content deleted"
                );
            }
            HookEvent::TypeMigrated { type_id, rows_migrated, changes, .. } => {
                tracing::info!(
                    target: "ferro::hook",
                    type_id = %type_id,
                    rows = rows_migrated,
                    changes = changes.len(),
                    "type migrated"
                );
            }
        }
        Ok(())
    }

    fn name(&self) -> &str {
        "logging"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default)]
    struct CountHook {
        count: tokio::sync::Mutex<u32>,
    }

    #[async_trait]
    impl HookHandler for CountHook {
        async fn handle(&self, _event: &HookEvent) -> PluginResult<()> {
            *self.count.lock().await += 1;
            Ok(())
        }
        fn name(&self) -> &str {
            "count"
        }
    }

    #[tokio::test]
    async fn dispatch_invokes_each_handler() {
        let reg = HookRegistry::new();
        let h = Arc::new(CountHook::default());
        reg.register(h.clone()).await;

        let evt = HookEvent::ContentDeleted {
            site_id: ferro_core::SiteId::new(),
            type_id: ferro_core::ContentTypeId::new(),
            content_id: ContentId::new(),
            slug: "x".into(),
        };
        reg.dispatch(evt).await;
        assert_eq!(*h.count.lock().await, 1);
    }

    #[derive(Debug, Default)]
    struct FailingHook;

    #[async_trait]
    impl HookHandler for FailingHook {
        async fn handle(&self, _event: &HookEvent) -> PluginResult<()> {
            Err(crate::error::PluginError::Other("boom".into()))
        }
        fn name(&self) -> &str {
            "fail"
        }
    }

    #[tokio::test]
    async fn handler_failure_does_not_propagate() {
        let reg = HookRegistry::new();
        reg.register(Arc::new(FailingHook)).await;
        // Should not panic / return.
        reg.dispatch(HookEvent::ContentDeleted {
            site_id: ferro_core::SiteId::new(),
            type_id: ferro_core::ContentTypeId::new(),
            content_id: ContentId::new(),
            slug: "x".into(),
        })
        .await;
    }
}
