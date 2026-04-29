# Changelog

All notable changes to Ferro. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
project versioning follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

`0.1.0` is the first **published** crate tag (alpha). The entries below tagged
`0.1` … `0.6` are **internal milestone markers** from the pre-publication dev
arc in [`DESIGN.md §13`](DESIGN.md#13-roadmap) — they were never crate-tagged,
they document the build-up to `0.1.0`. SemVer guarantees begin at `1.0`.

## [0.1.0] — 2026-04-26

First tagged alpha preview. Feature-preview milestone for the v1 roadmap —
**not** a stabilization release. Public APIs of every `ferro-*` crate may
still change without notice until 1.0; SemVer guarantees begin then.

### Added
- **WASM plugin host (real)**: `wasmtime` + component model wiring is live. The
  host implements the `host` interface (`log`, `get-content`); plugins
  implement `guest` (`init`, `on-event`). Capability grants flow from
  `ferro.toml [[plugins.grants]]`; missing grants block the load.
  ([`crates/ferro-plugin/wit/ferro.wit`](crates/ferro-plugin/wit/ferro.wit),
  [`crates/ferro-plugin/src/runtime.rs`](crates/ferro-plugin/src/runtime.rs))
- **Plugin registry**: hot-swap reload via `Arc<RwLock<HashMap>>`,
  enable/disable per plugin, `describe[_all]` over loaded set.
- **Plugin REST surface**: `GET /api/v1/plugins[/{name}]`, `POST .../grant`,
  `POST .../reload`, `POST .../enabled` — all gated by `ManagePlugins`.
  OpenAPI spec includes them.
- **Admin UI plugins page**: replaces the stub. Lists installed plugins with
  declared/granted caps as chips, enable/disable toggles, reload button.
- **`examples/plugin-hello`**: minimal WASM example. Subscribes to
  `content.published`, logs the slug via host.
- **Integration test**: `crates/ferro-plugin/tests/wasm_dispatch.rs` (gated
  on `WASM_TESTS=1`) loads the example component, fires a real event,
  asserts the log lands.
- **RBAC editor UI**: `/admin/roles` for full role CRUD with permission
  picker; `/admin/users` upgraded with role assignment, active toggle,
  password rotation.
- **mdBook docs site**: `docs/book.toml` + `docs/SUMMARY.md`. Builds to
  `target/book/`. `mdbook serve docs` for local dev.
- **Perf budget enforcement**: `ferro budgets` subcommand walks
  `target/site/pkg/*.wasm.br` and asserts per-route ≤ 250 KB, aggregate
  ≤ 1 MB (per DESIGN.md §10). New integration test
  `crates/ferro-cli/tests/perf_budgets.rs` (`PERF_BUDGETS=strict` for CI).
- **CI**: dedicated jobs for `mdbook build`, `wasm32-wasip2` plugin build +
  `wasm_dispatch` test, full bundle build + perf-budget enforcement.

### Changed
- `HookHandler::plugin_name()` default-impl method added so the registry can
  selectively swap plugin-owned handlers without disturbing built-ins.
- `AppState` gains `Option<Arc<PluginRegistry>>`; tests stay backwards
  compatible.
- `ApiError::Unavailable(String)` → 503 Service Unavailable.

## [0.6] — 2026-04-26

### Added
- GraphQL subscriptions over WebSocket (`/graphql/ws`) bridged to the
  `HookRegistry` broadcast bus.
- Per-event RBAC filtering for GraphQL and SSE subscribers.
- TOTP enrollment + MFA-aware login (RFC 6238). Admin UI for setup/disable.
- Strongly-typed OpenAPI spec at `/api/openapi.json` (Swagger UI at
  `/api/docs`).
- fs-markdown content versioning + CSRF double-submit on browser POST.
- Admin UI migrated to Leptos SSR + CSR (lazy-route split + brotli).
- Technical handbook + starter-blog example.

## [0.5] — RBAC + admin polish + plugin UI + OpenAPI

### Added
- Refresh-token rotation with theft detection (one-shot, 30-day TTL).
- Content versioning (`ContentVersion` + REST `/versions`, `/restore`).
  Implemented for fs-json, then expanded to Postgres + Surreal.
- Admin REST API for users + roles. CLI for out-of-band admin bootstrap.
- Public signup + password rotation (default-deny).
- Per-event RBAC filtering for GraphQL + SSE subscribers.

## [0.4] — S3/GCS media + schema migrations + export/import

### Added
- S3 + GCS media backends.
- Automated schema-evolution migrations (`PATCH /api/v1/types/{slug}` triggers
  `schema_migrator`; `rows_migrated` reported via UI toasts).
- `ferro export` / `ferro import` site-bundle round trip.
- Cross-backend search (`ContentQuery::search`) for fs-json, Postgres,
  Surreal.

## [0.3] — GraphQL + Postgres + plugin host MVP

### Added
- GraphQL queries + mutations (`async-graphql` + Axum).
- Postgres backend with full CRUD + JSONB.
- Plugin host MVP: in-process Rust hooks via `HookRegistry` +
  `HookEvent` (created/updated/published/deleted/migrated). Webhook engine
  with HMAC-SHA256 signed deliveries.
- Stateless JWT invalidation via `iat` + `password_changed_at`.
- Per-IP token-bucket rate limiting on `/login` + `/signup`.
- fs-markdown backend completion (24 method stubs filled, YAML front-matter,
  git-friendly layout).

## [0.2] — Content editor + media local + REST write + JWT

### Added
- Leptos field-editor components (islands).
- Local-fs media backend + multipart upload pipeline.
- REST write-path for sites / types / content / users / roles / media.
- JWT auth with HS256 signing, refresh from session store.
- Schema designer in admin UI (slug/name/description + quick-add presets
  for Text, RichText, Number, Boolean, Date).

## [0.1] — Workspace scaffold

### Added
- Workspace with 10 crates: `ferro-{core, macros, storage, auth, media,
  plugin, api, editor, admin, cli}`.
- Domain types in `ferro-core` (Site, ContentType, FieldDef, Content, User,
  Role, Media, Permission/Scope).
- `Repository` trait + fs-json + SurrealDB embedded backends.
- Admin login + dashboard (initial SPA shell).
- REST read-path (GET sites / types / content).
- ADRs 0001–0006 (Leptos, wasmtime+WIT, SurrealDB-embedded default,
  GraphQL + REST co-equal, defer edge to v2, Argon2id).
