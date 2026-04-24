# Ferro — Design Document

> The Rust-Powered Content Engine for the WebAssembly Era.

Ferro is an isomorphic CMS built on [Leptos](https://leptos.dev): the same Rust code compiles to a native server (Axum) and a browser WASM bundle. Content editors use the admin UI; developers consume REST + GraphQL APIs; plugins extend the system through sandboxed WebAssembly.

---

## 1. Goals & Non-Goals

### Goals (v1)

- **Isomorphic rendering**: SSR + hydration + islands via Leptos (0.8+).
- **Pluggable storage**: SurrealDB (embedded), Postgres, flat JSON, flat Markdown — swap with a feature flag.
- **GraphQL + REST** first-class, co-generated from a shared schema.
- **Baked-in auth**: argon2 password hashing, session cookies, JWT (optional), RBAC.
- **WASM plugins** via `wasmtime` with a capability-based host ABI.
- **Media**: local filesystem, S3, GCS — behind one trait.
- **Route-level code-splitting** with Leptos `lazy_route` + `cargo leptos --split` + brotli-compressed `.wasm` chunks.
- **CLI**: `ferro init|serve|migrate|export|import|plugin`.

### Non-Goals (v1)

- **Edge runtime** (Cloudflare Workers, Fastly Compute@Edge) — deferred to v2. The storage trait is designed to accept KV/D1/DO backends later without API changes.
- Multi-tenancy at the database level (single-site per deploy; multi-site within one deploy is planned v1.1).
- A visual page builder. v1 ships a block/field editor, not drag-and-drop layout.

---

## 2. Architecture Overview

```
                     ┌────────────────────────────────────┐
                     │            ferro-cli               │
                     │  init • serve • migrate • export   │
                     └──────────────┬─────────────────────┘
                                    │
            ┌───────────────────────┼───────────────────────┐
            │                       │                       │
  ┌─────────▼─────────┐   ┌─────────▼─────────┐   ┌─────────▼─────────┐
  │   ferro-admin     │   │    ferro-api      │   │   ferro-plugin    │
  │ (Leptos SSR app)  │   │ Axum + GraphQL    │   │ wasmtime host     │
  │ lazy_route+split  │   │ + REST            │   │ capability gated  │
  └─────────┬─────────┘   └─────────┬─────────┘   └─────────┬─────────┘
            │                       │                       │
            └──────────┬────────────┴───────────┬───────────┘
                       │                        │
              ┌────────▼────────┐      ┌────────▼────────┐
              │  ferro-core     │      │  ferro-auth     │
              │  domain model   │      │  argon2+session │
              │  schema+fields  │      │  RBAC+JWT       │
              └────────┬────────┘      └─────────────────┘
                       │
              ┌────────▼────────────────────────────────┐
              │            ferro-storage                │
              │  Repository trait + backends (features) │
              │  • surreal  • postgres  • fs-json  • md │
              └────────┬────────────────────────────────┘
                       │
              ┌────────▼────────┐
              │  ferro-media    │
              │  FS • S3 • GCS  │
              └─────────────────┘
```

### Crate map

| Crate | Role |
|---|---|
| `ferro-core` | Domain types: `Site`, `ContentType`, `Field`, `Content`, `User`, `Role`, `Media`. Validation, schema evolution. |
| `ferro-macros` | `#[derive(ContentType)]`, `field!` proc-macros for typed content. |
| `ferro-storage` | `Repository` trait hierarchy + feature-gated impls. |
| `ferro-auth` | Argon2 password hash, session store, JWT, RBAC policy. |
| `ferro-media` | `MediaStore` trait + local, S3, GCS backends. |
| `ferro-plugin` | `wasmtime`-based host, capability grants, plugin manifest. |
| `ferro-api` | Axum router, `async-graphql` schema, REST endpoints, OpenAPI. |
| `ferro-editor` | Markdown + block editor Leptos component (island). |
| `ferro-admin` | Leptos SSR app — login, dashboard, schema, content editor, media library, users, plugins. |
| `ferro-cli` | Binary entrypoint; orchestrates the above. |
| `examples/starter-blog` | Minimal site scaffold. |

---

## 3. Data Model (core)

```rust
pub struct Site { id, name, slug, locales, default_locale, settings }
pub struct ContentType { id, slug, name, fields: Vec<FieldDef>, singleton: bool }
pub struct FieldDef { id, slug, kind: FieldKind, required, localized, validators }
pub enum FieldKind {
    Text { multiline: bool, max: Option<usize> },
    RichText { format: RichFormat },
    Number { int: bool, min, max },
    Boolean,
    Date,
    DateTime,
    Enum(Vec<String>),
    Reference { to: ContentTypeId, multiple: bool },
    Media { multiple: bool, accept: Vec<MimePattern> },
    Json,
    Slug { source: FieldId },
}
pub struct Content { id, type_id, slug, status: Status, locale, data: Value, created_at, updated_at, author }
pub enum Status { Draft, Published, Archived }
pub struct User { id, email, handle, password_hash, roles: Vec<RoleId>, created_at }
pub struct Role { id, name, permissions: Vec<Permission> }
pub enum Permission { Read(Scope), Write(Scope), Publish(Scope), Admin }
pub struct Media { id, key, mime, size, width, height, alt, created_at }
```

### Schema evolution

- Field adds are non-breaking.
- Field removes require a two-phase migration: soft-deprecate → drop after N releases.
- Migrations are stored per-backend (`migrations/<backend>/*`). The CLI applies them.

---

## 4. Storage

### Trait hierarchy

```rust
#[async_trait]
pub trait Repository: Send + Sync {
    fn content(&self) -> &dyn ContentRepo;
    fn users(&self) -> &dyn UserRepo;
    fn types(&self) -> &dyn ContentTypeRepo;
    fn media(&self) -> &dyn MediaMetaRepo;
    async fn migrate(&self) -> Result<()>;
    async fn health(&self) -> Result<()>;
}

#[async_trait] pub trait ContentRepo {
    async fn get(&self, id: ContentId) -> Result<Option<Content>>;
    async fn list(&self, q: ContentQuery) -> Result<Page<Content>>;
    async fn create(&self, c: NewContent) -> Result<Content>;
    async fn update(&self, id: ContentId, patch: ContentPatch) -> Result<Content>;
    async fn delete(&self, id: ContentId) -> Result<()>;
    async fn publish(&self, id: ContentId) -> Result<Content>;
}
```

### Backend matrix

| Backend | Feature flag | Notes |
|---|---|---|
| SurrealDB embedded | `storage-surreal` | Default dev. RocksDB under the hood. |
| SurrealDB remote | `storage-surreal` + URL | Same impl, different connect string. |
| Postgres | `storage-postgres` | `sqlx` + compile-time queries. |
| Flat JSON | `storage-fs-json` | Single file + index. Good for demos. |
| Flat Markdown | `storage-fs-markdown` | YAML front matter + body. Git-friendly content. |

### Export / Import

Canonical interchange format: **JSON bundle** (`ferro.bundle.json`) — content types + content + users (without password hashes) + media manifest. CLI commands:

```sh
ferro export --out site.bundle.json [--include-media ./media]
ferro import site.bundle.json [--merge|--replace]
```

Lets users migrate between backends: export from SurrealDB → switch feature → import into Postgres.

---

## 5. Auth

- Password hashing: `argon2id` (`argon2` crate).
- Sessions: server-side store keyed by opaque cookie (SameSite=Lax, HttpOnly, Secure).
- JWT: optional, for headless/API clients (`jsonwebtoken`).
- RBAC: role → permission set → scope (global, per type, per entry).
- CSRF: double-submit cookie for browser POST; API tokens exempt.
- Rate-limit login (per IP + per account) to blunt credential stuffing.

---

## 6. Media

`MediaStore` trait:

```rust
#[async_trait]
pub trait MediaStore: Send + Sync {
    async fn put(&self, key: &str, body: ByteStream, mime: &str) -> Result<MediaRef>;
    async fn get(&self, key: &str) -> Result<ByteStream>;
    async fn delete(&self, key: &str) -> Result<()>;
    async fn presign_get(&self, key: &str, ttl: Duration) -> Result<Url>;
}
```

Backends: `local-fs` (default, dev), `s3` (via `aws-sdk-s3`), `gcs` (via `google-cloud-storage`).

Image pipeline: `image` crate for resize/format/quality; derivatives cached keyed by `(asset, transform_hash)`.

---

## 7. Plugin System (`ferro-plugin`)

**Runtime:** `wasmtime` with the component model.

**ABI:** WIT (WebAssembly Interface Types). A plugin exports hooks and imports host capabilities.

```wit
// wit/ferro.wit
package ferro:cms@0.1.0;

interface host {
    get-content: func(id: string) -> option<content>;
    emit-log: func(level: log-level, msg: string);
    http-fetch: func(req: http-req) -> result<http-resp, string>;
}

interface hooks {
    on-content-create: func(c: content) -> option<content>;
    on-content-publish: func(c: content) -> result<_, string>;
    on-request: func(req: http-req) -> option<http-resp>;
}
```

**Capabilities**: plugins declare required capabilities in `plugin.toml`. Host grants explicitly. No ambient authority.

```toml
# plugin.toml
name = "seo-sitemap"
version = "0.1.0"
entry = "sitemap.wasm"
capabilities = ["content.read", "http.serve:/sitemap.xml"]
```

**Why wasmtime + component model**: stable ABI, cross-language (Rust/Go/C++/JS via componentize-js), preemptive fuel-based timeouts, memory limits, epoch interruption.

---

## 8. API Layer (`ferro-api`)

- **Axum** router, layered middleware (auth, CORS, compression, rate-limit, tracing).
- **GraphQL**: `async-graphql` with schema built from registered `ContentType`s at boot. Subscriptions via WebSocket for live preview.
- **REST**: conventional `/api/v1/content/:type`, `/api/v1/media`, `/api/v1/auth/*`. OpenAPI via `utoipa`.
- Content delivery endpoints are cache-friendly (ETag + `Cache-Control`); admin endpoints are no-store.

---

## 9. Admin UI (`ferro-admin`)

- Leptos **SSR** mode (server renders HTML, client hydrates).
- **Islands** for interactive widgets (editor, media picker).
- **`lazy_route!`** for per-route WASM bundles.
- **`cargo leptos --split`** for route-level code-splitting.
- **Brotli** pre-compressed `.wasm.br`, `.js.br`, `.css.br` served by the Axum layer with content-negotiation fallback to gzip.

Routes:

```
/admin/login
/admin                         (dashboard)
/admin/content/:type           (list)
/admin/content/:type/:id       (edit)
/admin/schema                  (content types)
/admin/media                   (library)
/admin/users                   (users+roles)
/admin/plugins                 (plugin manager)
/admin/settings
```

---

## 10. Build & Bundle

- `cargo leptos build --release --split` → per-route chunks.
- Post-build step (invoked by `ferro-cli build`): Brotli-compress all `.wasm`, `.js`, `.css`, `.svg` with quality 11; keep originals for gzip/identity.
- Axum serves via a `PrecompressedService` that honors `Accept-Encoding`.
- Target bundle: < 120 KB brotli for the login route, < 250 KB for content-edit route (initial budget — measured, not aspirational).

---

## 11. Security

- Argon2id for passwords. Zeroize on drop for secrets.
- CSRF: double-submit token on browser POST.
- `Content-Security-Policy` default: `script-src 'self' 'wasm-unsafe-eval'; object-src 'none'; base-uri 'self'`.
- Plugins: no network/FS unless capability granted; `epoch_interruption` for runaway plugins.
- SQL: parameterized via `sqlx`/SurrealDB driver; no string concat.
- File upload: MIME sniff + extension check + size limit + AV hook (optional plugin).

---

## 12. Observability

- `tracing` + `tracing-subscriber` (JSON in prod, pretty in dev).
- OTLP exporter optional (`otel` feature).
- Metrics: `/metrics` Prometheus endpoint (auth-gated).
- Health: `/healthz` (liveness), `/readyz` (includes storage ping).

---

## 13. Roadmap

| Version | Highlights |
|---|---|
| 0.1 | Workspace scaffold, core types, `fs-json` + `surreal-embedded`, admin login + dashboard, REST read-path. |
| 0.2 | Content editor, media local-fs, REST write-path, JWT. |
| 0.3 | GraphQL, Postgres backend, plugin host MVP (hooks only). |
| 0.4 | S3/GCS media, schema evolution migrations, export/import CLI. |
| 0.5 | Admin polish, RBAC UI, plugin capability UI, OpenAPI. |
| 0.6 | Live preview (GraphQL subscriptions). |
| 1.0 | Stabilization, performance budgets, docs site. |
| 2.0 | Edge target (CF Workers/Fastly) via KV/D1 storage impl. |

---

## 14. Architectural Decision Records

See `docs/adr/`. Numbered, immutable, superseded-by links when revised.

- ADR-0001: Leptos SSR over Yew/Dioxus/Sycamore
- ADR-0002: wasmtime over extism for plugins
- ADR-0003: SurrealDB embedded as default dev backend
- ADR-0004: GraphQL + REST (not either-or)
- ADR-0005: Defer edge to v2
- ADR-0006: Argon2id over bcrypt/scrypt

---

## 15. Open Questions

- Multi-tenant within one binary: share schema or namespace per site?
- Live preview: GraphQL subscription or SSE?
- Plugin marketplace/discovery — out of scope v1, design hook for v2.
- Localization: per-field `localized: bool` vs per-entry locale rows. (Currently: per-entry rows, keyed by `(content_id, locale)`.)
