# CLAUDE.md — Ferro project notes for Claude

Pre-alpha **Rust CMS**. Isomorphic Leptos SSR + Axum API + wasmtime plugin host + zero-JS public Leptos islands site. Roadmap in `DESIGN.md §13`. Architecture in `DESIGN.md` and `docs/`. ADRs in `docs/adr/`.

## Workspace (10 crates under `crates/` + 2 example crates)

| Crate | Role |
|---|---|
| `ferro-core` | Domain models: `Site`, `ContentType`, `FieldDef`, `Content`, `User`, `Role`, `Media`, `Permission`. `RichFormat` enum: Markdown, ProseMirror, Html, **Blocks** (native Ferro block-doc format). |
| `ferro-macros` | Proc-macros for derives (minimal use today) |
| `ferro-storage` | `Repository` trait + 4 backends (`fs-json`, `fs-markdown`, `surreal`, `postgres`) — all full CRUD, feature-gated. Default feature: `surreal`. **fs-json writes are atomic** (write-then-rename). |
| `ferro-auth` | Argon2id, JWT (`iat`-based invalidation), refresh-token rotation, RFC 6238 TOTP, RBAC, per-IP token-bucket rate limit, CSRF double-submit |
| `ferro-media` | `MediaStore` trait + local/S3/GCS backends + image pipeline |
| `ferro-plugin` | wasmtime + component-model plugin host. Engine + capability gate scaffolded; `Services` real. In-process Rust hooks via `HookRegistry`. Outbound HMAC webhooks via `WebhookHook`. **Per-plugin WASI sandbox dir** (`<plugin>/data` preopened as `/data`) for sidecar writes. |
| `ferro-api` | Axum router: REST (`/api/v1/*`), GraphQL (`/graphql` + WS subs at `/graphql/ws`), SSE (`/api/v1/events`), OpenAPI (`/api/openapi.json`), Swagger UI (`/api/docs`), **`/preview/{type}/{slug}`** (auth-gated draft renderer). |
| `ferro-editor` | Leptos field-editor components (islands). `FieldEditor` dispatcher + `MarkdownEditor` (pulldown-cmark) + **`BlockEditor`** (pure-Rust block editor: paragraph, heading, quote, code, image, list, divider; slash-menu insertion; ↑↓× row controls). `render_blocks_html` shared by preview route + public site SSR. |
| `ferro-admin` | Leptos **SSR + CSR** admin app. Routes: `login`, `dashboard`, `content` (list/edit), `schema`, `media`, `users`, `roles`, `settings`, `plugins`. Content edit is **schema-driven** (per-field `FieldEditor`) with **side-by-side live preview iframe** auto-reloaded via SSE. Lazy-route split + brotli post-build via CLI. |
| `ferro-cli` | `ferro` binary. Subcommands: `init`, `serve`, `migrate`, `export`, `import`, `build` (now takes `--project`), `plugin` (`list`/`inspect`/`reload`/**`install <crate-path>`**), `admin`, `config`, `budgets`. |

### Examples

| Path | Role |
|---|---|
| `examples/starter-blog/` | Demo CMS instance. **5 content types** (Post, Author, Page, Product, Event) + 11 seed entries. fs-json + local media. `ferro.toml` declares plugin grants. |
| `examples/starter-blog/site-app/` | **Leptos islands lib** (cdylib + rlib). `App`, `shell`, `hydrate()` entry. Routes: `/`, `/blog`, `/blog/:slug`, `/products`, `/products/:slug`, `/events`, `/events/:slug`, `/:slug`. Detail routes wrapped in `Lazy<...>` via `#[lazy_route]`. Two `#[island]` components: `ThemeToggle` (localStorage), `SearchFilter` (DOM filter). |
| `examples/starter-blog/site-server/` | Public SSR Axum bin. Serves `/pkg/*` with brotli content-negotiation + immutable cache. Loads SEO sidecars from `plugins/seo/data/` and injects OG meta + JSON-LD. |
| `examples/plugin-hello/` | Minimal observer: logs `content.published`. |
| `examples/plugin-seo/` | Writes `/data/{type}/{slug}.json` (OG + JSON-LD by type). Capabilities: `logs`, `content.read`. |
| `examples/plugin-audit/` | JSONL audit log of all 4 content events to `/data/audit.log`. |
| `examples/plugin-panic/` | Intentional `panic!()` on `content.created` — demos host fault isolation + hot-swap. |
| `examples/plugin-webhook-demo/` | Config-only. Uses built-in `WebhookHook` via `ferro.toml`. |

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
- **Public site = zero-JS by default**, islands hydrate only marked components (`#[island]`). Shipping a non-island component as interactive is a code-review block.
- **Block editor** is pure-Rust Leptos. No TipTap/ProseMirror/JS WYSIWYG.
- **Markdown**: pulldown-cmark with tables, footnotes, strikethrough, task lists. Public-facing markdown should be sanitized before injection (raw HTML passes through by default).

## Tests

~140 passing. New since last update: `crates/ferro-editor/` (14: blocks model + render + markdown), `crates/ferro-api/tests/preview.rs` (3), `examples/starter-blog/site-app/src/seo.rs` (3 unit), atomic fs-json write covered by existing storage tests.

## Dev

### Admin + API (port 8080)

```sh
cargo leptos build --project ferro-admin                # one-time SPA bundle
cd examples/starter-blog
FERRO_JWT_SECRET=$(openssl rand -hex 32) \
  ../../target/debug/ferro --config ./ferro.toml serve --site-dir ../../target/site
# Admin: /admin   GraphiQL: /graphiql   REST: /api/v1/*   Preview: /preview/:type/:slug
```

Demo login: `me@example.com` / `correct-horse-battery-staple`.

### Public islands site (port 3001)

```sh
# Build islands bundle + bin (per-route --split, brotli post-build)
cargo leptos build --project starter-site --release --split
./target/debug/ferro build --skip-leptos --site-dir target/starter-site --quality 11

# Run (env vars only needed when not via cargo-leptos serve)
LEPTOS_SITE_ADDR=127.0.0.1:3001 \
LEPTOS_SITE_ROOT=$PWD/target/starter-site \
LEPTOS_OUTPUT_NAME=starter_site \
LEPTOS_SITE_PKG_DIR=pkg \
  ./target/release/starter-site

# Or live-reload
cargo leptos serve --project starter-site
```

Bundle sizes (release + brotli q=11): wasm 595K → **203K**, js 17K → 4.5K, css 3.2K → 1K.

### Plugins

```sh
# CLI install (per plugin)
./target/debug/ferro --config ./ferro.toml plugin install ../plugin-seo

# Or one-shot bash helper
./examples/starter-blog/install-plugins.sh

# Then list + toggle in admin
./target/debug/ferro --config ./ferro.toml plugin list
# → /admin/plugins to enable/disable hot-swap
```

`ferro.toml` declares grants per plugin via `[[plugins.grants]]` (name + capabilities).

## cargo-leptos projects

Two `[[workspace.metadata.leptos]]` tables in root `Cargo.toml`:

| Project | bin / lib | Port |
|---|---|---|
| `ferro-admin` | `ferro-cli` (bin) + `ferro-admin` (lib, `hydrate`) | 3000 (dev) / 8080 (prod) |
| `starter-site` | `starter-site-server` (bin, `ssr`) + `starter-site-app` (lib, `hydrate` + islands) | 3001 |

`ferro build --project <name> --site-dir <path>` runs cargo-leptos with `--split` then brotli-compresses the output.

## Docs

14 markdown files in `docs/` + ADRs in `docs/adr/` + new authoring guides: `docs/block-editor.md`, `docs/live-preview.md`, `docs/plugin-walkthrough.md`. mdbook config at `docs/book.toml` (still 1.0 roadmap item to publish).

## Roadmap status

See `DESIGN.md §13`. Versions 0.1, 0.2, 0.4, 0.6 = DONE. 0.3, 0.5 = DONE after plugin host completion. 1.0 (stabilization, perf budgets, docs site) — perf budgets DONE, docs site DONE (mdbook). Remaining: stabilization pass.
