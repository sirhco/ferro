# Edge runtime

> **Status:** v1 ships **self-hosted only** (binary or container). Edge runtimes — Cloudflare Workers, Fastly Compute@Edge — are a **v2 goal**. See [ADR-0005](adr/0005-defer-edge.md) for the rationale.

This page is the operator-and-contributor map for the edge story: what Ferro already does that's edge-friendly, what's blocking edge today, and which interfaces are pre-shaped so a v2 PR can land KV/D1/R2 backends without breaking self-host.

## Edge-friendly today

These properties already hold in v1 — no change needed for v2.

- **Stateless request handlers.** `AppState` (built once in `ferro-cli`) is a bag of `Arc`s — `Repository`, `MediaStore`, `JwtConfig`, `HookRegistry`, options. No per-request mutable state, no thread-locals, no leader election.
- **JWT, not session.** Access tokens are HS256 self-contained (`crates/ferro-auth/src/jwt.rs`). Verification is pure CPU — no session-store round-trip on the hot path. Fits Workers' "no long-lived state" model.
- **Cache-friendly REST.** Read endpoints set `Cache-Control` and `ETag` headers; write endpoints invalidate via predictable URLs.
- **Pure-function image pipeline.** `crates/ferro-media/src/image_pipeline.rs:1-68` takes `&[u8]` and returns transformed bytes. No backend coupling — fits any compute target that can decode + re-encode.
- **Async traits everywhere.** `Repository` and `MediaStore` are fully `async_trait`-based; backends slot in via `connect()` dispatchers in `crates/ferro-storage/src/backends/mod.rs` and `crates/ferro-media/src/backends/mod.rs`.

## Blocking edge today (v1 → v2 work)

| Blocker | Where | v2 fix |
|---|---|---|
| `wasmtime` plugin host | `crates/ferro-plugin/` | No nested-WASM in Workers. Plugin host disabled on edge build; v2 explores a worker-side plugin runtime. |
| `fs-json` + `fs-markdown` storage | `crates/ferro-storage/src/backends/{fs_json,fs_markdown}.rs` | No filesystem on edge. Replaced by `cf-kv` / `cf-d1` backends. |
| `surreal-embedded` | `crates/ferro-storage/src/backends/surreal.rs` (default feature) | RocksDB needs a filesystem. Edge build switches default to `cf-d1` or `postgres` (over a serverless driver). |
| In-memory rate-limiter | `crates/ferro-api/src/rate_limit.rs:31` (`Mutex<HashMap<IpAddr, Bucket>>`) | Per-process, not shared across replicas. v2: Workers KV or Durable Object–backed bucket. |
| In-process broadcast bus | `crates/ferro-plugin/src/hook.rs:107` (`tokio::sync::broadcast`) | SSE subscribers see only same-replica events. v2: Durable Object pub/sub or external bus. |
| In-memory `PluginRegistry` | `crates/ferro-plugin/src/registry.rs` | Already disabled-on-edge per ADR-0005; no action. |
| Local filesystem media | `crates/ferro-media/src/backends/local.rs` | Replaced by R2 (via the existing S3 backend after the endpoint-override patch — see below). |

## Pre-shaped for v2

These shapes are deliberate so a v2 PR can land without touching the v1 trait surface.

- **`Repository` is id-keyed `get` + `by_*` lookups + filterable `list`.** A KV-backed implementation can satisfy this via a small set of secondary index keys:
  - `idx:site:<id>:content:<status>` → list of content ids
  - `idx:slug:<site>:<type>:<slug>` → content id
  - `content:<id>` → serialized `Content`
  
  Full-text `search` is documented as scan-acceptable on the trait (see `crates/ferro-storage/src/repo.rs`). A KV impl can satisfy it by scanning the per-(site,type) index and filtering in-memory.
- **`MediaStore` is byte-stream-shaped.** `put`/`get` use a `Pin<Box<dyn Stream<Item=io::Result<Bytes>> + Send>>`. An R2 backend (which speaks the S3 API) reuses the `S3Store` impl after the endpoint-override patch in `crates/ferro-media/src/backends/s3.rs` — no new backend module needed.
- **Backend feature gating is per-driver.** Adding `cf-kv`, `cf-d1` features to `crates/ferro-storage/Cargo.toml` won't disturb existing builds; the `connect()` dispatcher returns `BackendNotEnabled` if the feature is off.

## Out of scope for v1

The following will not land before v2 ships:

- `cf-kv` `Repository` backend (needs `wasm32-unknown-unknown` build target and a worker-side runtime crate).
- `cf-d1` `Repository` backend.
- A distinct `r2` `MediaStore` backend — R2 reuses `s3` with the endpoint override.
- Plugin host on edge.
- Distributed rate-limit and cross-replica SSE bus.

## Reference

- [ADR-0005: Defer edge to v2](adr/0005-defer-edge.md) — decision record + rationale.
- `DESIGN.md §13` — roadmap; edge is v2.0.
