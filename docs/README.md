# Ferro documentation

Operator and developer guides for the Ferro CMS.

## Building this site

The same files render as a searchable [mdBook](https://rust-lang.github.io/mdBook/):

```sh
cargo install mdbook --locked
mdbook serve docs            # live-reload at http://localhost:3000
mdbook build docs            # static HTML in target/book/
```

## Pages

### Start here

- [`getting-started.md`](getting-started.md) — install, init, first admin user, login.
- [`architecture.md`](architecture.md) — crate map, request flow, process model.

### Configuration & operations

- [`configuration.md`](configuration.md) — every `ferro.toml` knob.
- [`cli.md`](cli.md) — `ferro` subcommands and flags.
- [`deployment.md`](deployment.md) — systemd, Docker, nginx, hardening.
- [`troubleshooting.md`](troubleshooting.md) — symptoms → fixes.

### Surface guides

- [`admin-ui.md`](admin-ui.md) — feature-by-feature tour of the Leptos SPA.
- [`rest-api.md`](rest-api.md) — REST endpoint reference + curl examples.
- [`graphql.md`](graphql.md) — GraphQL schema + subscriptions + SSE.
- [`api-versioning.md`](api-versioning.md) — versioning model + deprecation timeline shared by REST and GraphQL.

### Subsystems

- [`auth.md`](auth.md) — passwords, JWT, refresh, TOTP, RBAC, CSRF, rate limit.
- [`storage-backends.md`](storage-backends.md) — fs-json / fs-markdown / surreal / postgres tradeoffs.
- [`media.md`](media.md) — local / S3 / GCS, upload pipeline, public URLs.
- [`plugins-webhooks.md`](plugins-webhooks.md) — events, webhook signing, plugin host.
- [`edge.md`](edge.md) — edge-runtime constraints (v1 self-host vs. v2 CF Workers / Fastly).

### Architecture decisions

- [`adr/0001-leptos.md`](adr/0001-leptos.md) — Leptos as the UI framework.
- [`adr/0002-wasmtime-plugins.md`](adr/0002-wasmtime-plugins.md) — wasmtime + WIT for plugins.
- [`adr/0003-surrealdb-default.md`](adr/0003-surrealdb-default.md) — SurrealDB embedded as default dev backend.
- [`adr/0004-graphql-and-rest.md`](adr/0004-graphql-and-rest.md) — GraphQL + REST together.
- [`adr/0005-defer-edge.md`](adr/0005-defer-edge.md) — Edge target deferred to v2.
- [`adr/0006-argon2id.md`](adr/0006-argon2id.md) — Argon2id for password hashing.

### Examples

- [`../examples/starter-blog/`](../examples/starter-blog/) — minimal blog: site + Post/Author types + sample content.

### Project context

- [`../README.md`](../README.md) — project overview, status, highlights.
- [`../DESIGN.md`](../DESIGN.md) — architecture vision + roadmap.
- [`../CONTRIBUTING.md`](../CONTRIBUTING.md) — development setup, code style, PR flow.
