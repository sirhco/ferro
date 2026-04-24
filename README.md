# Ferro

**The Rust-Powered Content Engine.**

Ferro is an isomorphic CMS built on [Leptos](https://leptos.dev) and [Axum](https://github.com/tokio-rs/axum). Same Rust, running server-side and in the browser as WASM — with route-level code-splitting, brotli-compressed chunks, pluggable storage, baked-in auth, and sandboxed WebAssembly plugins via [`wasmtime`](https://wasmtime.dev).

> _Because your content management should be as solid as iron._

## Status

**Pre-alpha.** The workspace is scaffolded and compiles the data model, traits, and API surface. Backends, admin UI, and plugin host are progressing per the [roadmap](DESIGN.md#13-roadmap).

## Highlights

- **Isomorphic Rust**: one codebase, SSR + hydration + islands via Leptos.
- **Route-level split**: `lazy_route!` + `cargo leptos --split` + brotli-compressed `.wasm` per route.
- **Storage-pluggable**: SurrealDB (embedded/remote), Postgres, flat JSON, flat Markdown — one trait, feature-gated impls.
- **GraphQL + REST**: both first-class (`async-graphql` + Axum + OpenAPI).
- **Auth baked in**: argon2id passwords, sessions, JWT, RBAC.
- **Media**: local FS, S3, GCS — swap via config.
- **WASM plugins**: `wasmtime` + component model + WIT. Capability-based sandboxing, no ambient authority.
- **Ironclad by default**: Rust memory safety, CSP, CSRF, Zeroize on secret drop.

Edge runtime (Cloudflare Workers / Fastly) is a **v2** target — see [ADR-0005](docs/adr/0005-defer-edge.md) for why.

## Workspace layout

```
crates/
  ferro-core       # domain model (pure data + validation)
  ferro-storage    # Repository trait + backends (surreal/postgres/fs-json/fs-markdown)
  ferro-auth       # argon2 passwords, sessions, JWT, RBAC
  ferro-media      # MediaStore trait + local/S3/GCS + image pipeline
  ferro-plugin     # wasmtime plugin host (component model, WIT, capabilities)
  ferro-api        # Axum + async-graphql + REST + OpenAPI
  ferro-editor     # Leptos field-editor components (islands)
  ferro-admin      # Leptos SSR admin app (lazy_route, split, brotli)
  ferro-macros     # proc-macros (#[derive(ContentType)])
  ferro-cli        # `ferro` binary: init | serve | migrate | export | import | build | plugin
examples/
  starter-blog     # minimal Post + Author example
docs/
  adr/             # architecture decision records
DESIGN.md          # design + roadmap
```

## Quick start

Requires Rust nightly (`rust-toolchain.toml` pins the channel) and `cargo-leptos`:

```sh
cargo install cargo-leptos
cd examples/starter-blog
cargo run -p ferro-cli -- init --storage fs-json
cargo run -p ferro-cli -- serve
```

Admin UI at `http://localhost:8080/admin`, GraphiQL at `/graphiql`, REST at `/api/v1/*`.

## CLI

```sh
ferro init [--storage surreal|fs-json|fs-markdown|postgres]
ferro serve
ferro migrate
ferro export --out site.bundle.json [--include-media]
ferro import site.bundle.json [--mode merge|replace]
ferro build                    # cargo leptos build --split + brotli
ferro plugin list|inspect|reload
```

## Design

Read [DESIGN.md](DESIGN.md) for the full architecture, then the ADRs:

- [ADR-0001](docs/adr/0001-leptos.md) — Leptos as the UI framework
- [ADR-0002](docs/adr/0002-wasmtime-plugins.md) — wasmtime + WIT for plugins
- [ADR-0003](docs/adr/0003-surrealdb-default.md) — SurrealDB embedded as default dev backend
- [ADR-0004](docs/adr/0004-graphql-and-rest.md) — GraphQL + REST together
- [ADR-0005](docs/adr/0005-defer-edge.md) — Edge target deferred to v2
- [ADR-0006](docs/adr/0006-argon2id.md) — Argon2id for password hashing

## License

Apache-2.0. See [LICENSE](LICENSE).
