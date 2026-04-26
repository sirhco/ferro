# Plugins & webhooks

Ferro fans out content lifecycle events to a `HookRegistry`. Two kinds of consumers ship today:

1. **In-process Rust hooks** — `LoggingHook` (always on), `WebhookHook` (one per `[[webhooks]]` config entry).
2. **WASM plugins** — wasmtime + component model. Host scaffolded, runtime loader on the roadmap.

Hooks run in **isolation**: a panicking or failing hook doesn't sink the request that triggered it, and doesn't block other hooks for the same event.

## Events

| Event              | Fires on                                      | Payload                                                                  |
|--------------------|-----------------------------------------------|--------------------------------------------------------------------------|
| `content.created`  | `POST /api/v1/content/{type}` succeeds        | `{ content, type_slug }`                                                 |
| `content.updated`  | `PATCH /api/v1/content/{type}/{slug}` succeeds| `{ before, after, type_slug }`                                           |
| `content.published`| `POST /api/v1/content/{type}/{slug}/publish`  | `{ content, type_slug }`                                                 |
| `content.deleted`  | `DELETE /api/v1/content/{type}/{slug}`        | `{ site_id, type_id, content_id, slug, type_slug }`                      |
| `type.migrated`    | `PATCH /api/v1/types/{slug}` triggers migrator| `{ site_id, type_id, type_slug, rows_migrated, changes[] }`              |

## Webhooks

Configure in `ferro.toml`. Repeat the table to register multiple.

```toml
[[webhooks]]
url = "https://hooks.example.com/ferro"
events = ["content.created", "content.published"]   # empty = all events
secret = "shared-with-receiver"                      # optional
timeout_ms = 5000
name = "production-mirror"
```

### Outbound shape

`POST` with JSON body:

```json
{
  "event": "content.published",
  "occurred_at": "2026-04-26T02:50:08.123Z",
  "payload": { /* see Events table */ }
}
```

Headers:

- `Content-Type: application/json`
- `User-Agent: ferro-webhook/<version>`
- `X-Ferro-Event: content.published`
- `X-Ferro-Signature: hex(hmac_sha256(secret, body))` — present iff `secret` is set.

### Verifying the signature

```python
import hashlib, hmac

def verify(secret: bytes, body: bytes, header: str) -> bool:
    expected = hmac.new(secret, body, hashlib.sha256).hexdigest()
    return hmac.compare_digest(expected, header)
```

```javascript
import { createHmac, timingSafeEqual } from "node:crypto";

export function verify(secret, body, header) {
  const expected = createHmac("sha256", secret).update(body).digest("hex");
  return timingSafeEqual(Buffer.from(expected), Buffer.from(header));
}
```

### Failure semantics

- HTTP non-2xx is logged as a warning (`tracing` target `ferro::webhook`) and not retried.
- Network timeout (`timeout_ms`) is logged similarly.
- Webhook delivery never blocks the originating mutation. The handler returns to the caller as soon as the storage write commits; hook fan-out happens after.
- For at-least-once semantics, run an HTTP receiver that's idempotent per `(event, content_id, occurred_at)`.

### Filters

`events = ["content.published"]` ⇒ deliver only that event. `events = []` (or omit) ⇒ deliver all. Filter is per-webhook; multiple webhooks can subscribe to the same event independently.

### Operational tips

- Use one webhook per downstream system, not one shared with logic branching.
- Treat `secret` as a credential — rotate via config + rolling restart.
- Log on the receiver side; Ferro's tracing only sees deliver/fail signals, not body content.

## In-process hooks

You can register additional hooks in `ferro_cli::serve::run` if you're embedding Ferro:

```rust
use std::sync::Arc;
use ferro_plugin::{HookEvent, HookHandler, HookRegistry};

struct MyHook;

#[async_trait::async_trait]
impl HookHandler for MyHook {
    fn name(&self) -> &str { "my-hook" }
    async fn handle(&self, event: HookEvent) {
        match event {
            HookEvent::ContentPublished { content, .. } => {
                tracing::info!(slug = %content.slug, "my-hook: published");
            }
            _ => {}
        }
    }
}

// in serve.rs setup, after `HookRegistry::new()`:
hooks.register(Arc::new(MyHook)).await;
```

## WASM plugin host (scaffolded)

The plugin runtime in `crates/ferro-plugin/src/runtime.rs` initializes a `wasmtime::Engine` with `epoch_interruption` (for runaway protection), a `ResourceLimiter` (memory cap), and `wasmtime_wasi` for stdio (no FS / network unless explicitly granted via the WIT capability interface).

Today this loads but doesn't yet bind component-model functions to events. Roadmap:

1. Define `wit/ferro-plugin.wit` exposing `init`, `on-content-event(event: ContentEvent)`, etc.
2. Generate Rust bindings via `wasmtime::component::bindgen!`.
3. `PluginHost::load(path)` instantiates the component, stores the `HookHandler` impl that proxies into the plugin.
4. Capability grants (`network`, `fs`, custom resources) declared in a sidecar `.toml` or in the WIT instance imports.

When ready, drop a `.wasm` component into `[plugins].dir`, restart, and `ferro plugin list` will surface it.

## Why hooks instead of inline `if`s?

- Decoupling: the API handler stays focused on storage + validation.
- Fan-out: register N hooks for one event without changing the handler.
- Failure isolation: a webhook to a flaky third-party doesn't 500 the editor's PATCH call.
- Plugin parity: in-process Rust hooks and out-of-process WASM plugins consume the same events.
