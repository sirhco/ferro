# Contributing to Ferro

## Development setup

- Rust nightly (pinned by `rust-toolchain.toml`).
- `rustup target add wasm32-unknown-unknown`.
- `cargo install cargo-leptos`.

## Layout

See [README.md](README.md) and [DESIGN.md](DESIGN.md).

## Workflow

1. Branch from `main`.
2. Keep changes scoped to a single crate where possible.
3. `cargo fmt --all` and `cargo clippy --workspace --all-targets` before pushing.
4. New backends, auth flows, or plugin capabilities ship with an ADR under `docs/adr/`.

## Commit messages

Conventional Commits. `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`.

## Review bar

- **Security-relevant code** (auth, plugin host, storage drivers, media sniffing): two approvals.
- **API surface changes**: update OpenAPI + GraphQL schema, bump minor version.
- **Plugin ABI (WIT) changes**: semver — additive only within a minor.
