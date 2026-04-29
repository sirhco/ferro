# Architecture

Ferro is a Rust workspace of focused crates that compose into one binary (`ferro`) plus one WASM bundle (`ferro_admin`). This page is the map.

## Crate layout

```
ferro-core      Pure data model: Site, ContentType, Field, Content, Media, User, Role, Permission, ContentVersion.
                Validation, ID newtypes, locale, error types. Zero I/O. Compiles to wasm32 and host.

ferro-storage   Repository trait + per-backend impls: fs-json, fs-markdown, postgres, surreal-embedded.
                Schema migrator (additive + soft-delete). Feature-gated so prod builds skip unused drivers.

ferro-auth      Argon2id password hashing, RFC 6238 TOTP, JWT (HS256) issuance/verification, session store
                trait (memory + Postgres impls), RBAC authorization (Permission × Scope).

ferro-media     MediaStore trait + local FS / S3 / GCS impls. Streams in & out via tokio. Image
                pipeline behind `images` feature.

ferro-plugin    HookRegistry (in-process Rust hooks today; wasmtime component-model hosting scaffolded).
                LoggingHook + WebhookHook (HMAC-SHA256 signed outbound).

ferro-api       Axum router: REST (CRUD + auth + media), GraphQL (queries/mutations/subscriptions over WS),
                SSE event stream, OpenAPI 3 spec, CSRF middleware, per-IP rate limiter.

ferro-editor    Leptos field-editor components shared between admin and downstream consumers (markdown,
                slug, ref, media, json, enum, date, number, bool, text).

ferro-admin     Leptos CSR admin SPA. Routes for login/MFA/dashboard/content/schema/media/users/plugins/
                settings. Talks REST via gloo-net + web_sys.

ferro-macros    proc-macros: `#[derive(ContentType)]` for code-first schema authoring.

ferro-cli       The `ferro` binary. Subcommands: init, serve, migrate, export, import, build, plugin, admin.
                Wires storage + media + auth + plugins + Leptos SSR mount + /pkg static service.
```

The dependency graph is one-directional: `core ← storage/auth/media/plugin ← api ← cli`, with `editor → core` and `admin → editor + core`. No cycles.

## Process model

Single binary, single process. Tokio multi-threaded runtime. The `serve` subcommand:

1. Loads `ferro.toml` (storage, media, auth, plugins, webhooks, server.bind).
2. Connects the storage backend (`ferro_storage::connect`), runs `migrate()`, auto-seeds the default site if absent.
3. Connects the media backend (`ferro_media::connect`).
4. Builds in-memory `HookRegistry`, registers `LoggingHook`, registers any configured webhooks.
5. Constructs `AppState { repo, media, auth, jwt, hooks, options, auth_rate_limit }` once, wraps in `Arc`.
6. Composes the router:
   - `/pkg/*` → tower-http `ServeDir` (precompressed brotli).
   - `/favicon.svg` → file.
   - REST + GraphQL + SSE + OpenAPI (`ferro_api::router`).
   - `/admin/*` → Leptos SSR shell + WASM hydration.
7. Binds `tokio::net::TcpListener` and serves via `axum::serve`.

## Request flow — REST

```
client → axum::Router
       → CompressionLayer (br/gzip)
       → CorsLayer
       → TraceLayer (tracing spans)
       → CSRF middleware (csrf::enforce: bypasses Bearer + safe methods)
       → route handler (extracts AuthUser if needed)
       → AuthUser::try_from_headers (verify JWT, hydrate user, check password_changed_at)
       → policy check (authorize(ctx, Permission))
       → repo trait call (Repository::content().update(...))
       → HookRegistry::dispatch (fan-out to LoggingHook + Webhooks; isolated per hook)
       → JSON response
```

Errors funnel through `ApiError → IntoResponse`, mapping each variant to the right HTTP status + JSON `{ error, message }`.

## Request flow — admin UI

```
browser GET /admin/<route>
  → Leptos SSR renders shell HTML (head + bootstrap script + empty body container)
  → browser fetches /pkg/ferro_admin.js + /pkg/ferro_admin_bg.wasm
  → WASM bootstraps: leptos::mount::mount_to_body(App)
  → App provides AdminState context, kicks Effect: bootstrap_after_mount
  → bootstrap fetches /api/v1/auth/me + /api/v1/types via gloo-net + bearer token from localStorage
  → routes hydrate; user-driven actions (POST/PATCH/DELETE) call api.rs helpers
  → 401 → automatic refresh-token rotation via single-flight latch
```

The admin runs in CSR mode (not SSR-hydrate). The Leptos SSR shell exists only to bootstrap the WASM and serve a noscript fallback.

## Storage backends

All four implement the same `Repository` trait (sites, types, content, users, roles, media, versions). Selected via `[storage] kind = ...` in `ferro.toml`. Tradeoffs:

| Backend       | Best for                            | Layout                                                                          |
|---------------|-------------------------------------|---------------------------------------------------------------------------------|
| `fs-json`     | Demos, single-tenant, dev           | One JSON file per record under `<root>/<table>/<id>.json`                       |
| `fs-markdown` | Git-friendly editorial workflows    | Markdown + YAML front-matter under `<root>/<site>/<type>/<slug>.<locale>.md`    |
| `postgres`    | Production, multi-writer            | Normalized schema, JSONB for content data, full-text-style search via ILIKE     |
| `surreal`     | Embedded NoSQL with rich queries    | SurrealDB embedded RocksDB or remote; CONTAINS-based search                     |

Versioning is implemented for all four: snapshots written before mutating writes, list/get/restore via `ContentVersionRepo`.

## Auth model

- **Passwords**: Argon2id, default cost (m=19MiB, t=2, p=1). Verify uses constant-time comparison.
- **Access tokens**: HS256 JWT, 12h TTL, stateless. Embeds user id, role names, iat. Verified per-request in `AuthUser::try_from_headers`.
- **Refresh tokens**: opaque 64-char hex, 30d TTL, stored in `SessionStore`. Rotated on every `/auth/refresh` (one-shot use) for theft detection.
- **Stateless invalidation**: User has `password_changed_at`; tokens with `iat <` that timestamp are rejected. Equivalent to logout-all-sessions.
- **MFA**: RFC 6238 TOTP. `/login` returns `mfa_token` instead of session tokens when user has `totp_secret`. Caller redeems at `/auth/totp/login` with a 6-digit code.
- **CSRF**: double-submit token (cookie + `X-CSRF-Token` header) on cookie-bearing browser POSTs. Bearer requests bypass (CSRF-immune by construction).
- **Rate limit**: per-IP token bucket on `/auth/login`, `/auth/signup`, `/auth/refresh`, `/auth/totp/login`. Default 10/min sustained.

## RBAC

Permissions are `(Action, Scope)` tuples:

- Actions: `Read`, `Write`, `Publish`, `ManageUsers`, `ManageSchema`, `Admin` (wildcard).
- Scopes: `Site { id }`, `Type { id }`, `Global`.

Roles bundle permissions; users have many roles. Authorization checks: `authorize(ctx, Permission::Write(Scope::Type { id }))`. The `admin` role pre-seeded by `ferro admin --with-admin` includes the `Admin` action which short-circuits all checks.

## Hooks & webhooks

`HookRegistry` is an async fan-out. Each `HookHandler` runs in isolation — one panicking or failing hook doesn't sink the others. Built-ins:

- `LoggingHook` — emits a tracing event per dispatched event.
- `WebhookHook` — outbound HTTPS POST to a configured URL. Body is the event JSON. Header `X-Ferro-Signature` carries an HMAC-SHA256 over the body using the per-webhook secret. Per-webhook event filter (e.g. `["content.created"]`).

Configuration lives in `ferro.toml`'s `[[webhooks]]` array.

## Observability

- `tracing` + `tracing-subscriber` (pretty in dev, JSON-ready for prod).
- `/healthz` (liveness), `/readyz` (storage ping).
- Prometheus `/metrics` and OTLP exporter are on the roadmap.

## What lives outside this binary

- TLS termination — assumed at the reverse proxy (nginx, Caddy, Traefik, ALB).
- Object storage — S3 / GCS via the media backend; local FS for single-host setups.
- Secrets — JWT secret via `FERRO_JWT_SECRET` env or `[auth] jwt_secret` (the env wins).
- Dashboards / metrics scraping — Prometheus + Grafana once `/metrics` lands.

## Deployment targets

v1 targets self-hosted binary or container. Edge runtimes (Cloudflare Workers, Fastly Compute@Edge) are deferred to v2 — see [Edge runtime](edge.md) for what's already pre-shaped and what blocks the move.
