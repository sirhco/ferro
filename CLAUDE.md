# CLAUDE.md — Ferro project notes for Claude

Pre-alpha **Rust CMS**. Isomorphic Leptos SSR + Axum API + wasmtime plugin host. Roadmap in `DESIGN.md §13`. Architecture in `DESIGN.md` and `docs/`. ADRs in `docs/adr/`.

## Workspace (10 crates under `crates/`)

| Crate | Role |
|---|---|
| `ferro-core` | Domain models: `Site`, `ContentType`, `FieldDef`, `Content`, `User`, `Role`, `Media`, `Permission` |
| `ferro-macros` | Proc-macros for derives (minimal use today) |
| `ferro-storage` | `Repository` trait + 4 backends (`fs-json`, `fs-markdown`, `surreal`, `postgres`) — all full CRUD, feature-gated. Default feature: `surreal`. |
| `ferro-auth` | Argon2id, JWT (`iat`-based invalidation), refresh-token rotation, RFC 6238 TOTP, RBAC, per-IP token-bucket rate limit, CSRF double-submit |
| `ferro-media` | `MediaStore` trait + local/S3/GCS backends + image pipeline |
| `ferro-plugin` | wasmtime + component-model plugin host. Engine + capability gate scaffolded; `Services` real (post-roadmap-update). In-process Rust hooks via `HookRegistry`. Outbound HMAC webhooks via `WebhookHook`. |
| `ferro-api` | Axum router: REST (`/api/v1/*`), GraphQL (`/graphql` + WS subs at `/graphql/ws`), SSE (`/api/v1/events`), OpenAPI (`/api/openapi.json`), Swagger UI (`/api/docs`) |
| `ferro-editor` | Leptos field-editor components (islands) |
| `ferro-admin` | Leptos **SSR + CSR** admin app. Routes: `login`, `dashboard`, `content` (list/edit), `schema`, `media`, `users`, `settings`, `plugins`. Lazy-route split + brotli post-build via CLI. |
| `ferro-cli` | `ferro` binary. Subcommands: `init`, `serve`, `migrate`, `export`, `import`, `build`, `plugin`, `admin`, `config` |

Examples: `examples/starter-blog/` (minimal Post + Author project, multi-backend), `examples/plugin-hello/` (WASM example plugin — added with plugin host completion).

## Auth surface (full lifecycle)

Argon2id passwords → JWT access + 30-day refresh (one-shot rotation, theft detection) → optional TOTP (mfa_token → POST `/auth/totp/login`) → JWT `iat` < `user.password_changed_at` invalidates → per-IP rate limit on `/login` + `/signup` → CSRF double-submit on browser POST.

## Conventions

- **Rust nightly** pinned via `rust-toolchain.toml`
- **SurrealDB embedded** is the default dev backend (ADR-0003)
- **GraphQL + REST** are co-equal first-class (ADR-0004)
- **Edge runtime is v2** — see ADR-0005, do not propose CF Workers / Fastly work
- **Argon2id** for passwords (ADR-0006). Never bcrypt/scrypt.
- **wasmtime** over extism for plugins (ADR-0002). Component model + WIT.
- **Leptos** over Yew/Dioxus (ADR-0001)

## Tests

~88 passing across 20 test files: `crates/ferro-api/tests/` (14), `crates/ferro-cli/tests/` (2), `crates/ferro-storage/tests/` (3), `crates/ferro-plugin/tests/` (1+).

## Dev

```sh
cd examples/starter-blog
cargo run -p ferro-cli -- init --storage fs-json
cargo run -p ferro-cli -- serve
# Admin: http://localhost:8080/admin   GraphiQL: /graphiql   REST: /api/v1/*
```

Or `cargo leptos watch` from `crates/ferro-admin/` for live-reload.

## Docs

14 markdown files in `docs/` + ADRs in `docs/adr/`. No built docs site yet (mdbook is a 1.0 roadmap item).

## Roadmap status

See `DESIGN.md §13`. Versions 0.1, 0.2, 0.4, 0.6 = DONE. 0.3, 0.5 = PARTIAL→DONE after plugin host completion. 1.0 (stabilization, perf budgets, docs site) remains.
