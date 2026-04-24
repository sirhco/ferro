# ADR-0001: Leptos as the UI framework

**Status:** Accepted
**Date:** 2026-04-24

## Context

Ferro needs an isomorphic Rust UI framework for the admin app: the same code must SSR on the server and hydrate in the browser as WASM. Candidates: Leptos, Yew, Dioxus, Sycamore.

## Decision

Use **Leptos 0.8+** in `ssr` + `hydrate` mode, with islands for interactivity.

## Rationale

- Fine-grained reactive signals — smaller diff, faster updates than VDOM.
- First-class server functions (`#[server]`) eliminate a separate RPC layer for admin-only mutations.
- `lazy_route!` + `cargo leptos --split` deliver per-route code-splitting, directly serving the "chunked WASM for small bundles" goal.
- `cargo leptos` is a production-grade build orchestrator (SSR bundle, hydrate bundle, Tailwind, asset pipeline).
- Axum integration (`leptos_axum`) slots into our API crate cleanly.

## Alternatives Considered

- **Yew**: Mature, but VDOM-heavy and lacks a batteries-included SSR story.
- **Dioxus**: Fullstack story improving, but islands/code-split not as polished; LiveView mode is a different architecture.
- **Sycamore**: Fine-grained reactive like Leptos, smaller ecosystem; SSR less ergonomic.

## Consequences

- We couple to Leptos's reactive model (signals, effects, resources).
- Upgrade friction when Leptos ships breaking releases; pin in `Cargo.toml`, track upstream.
- Server functions blur the client/server boundary — we document the rule: no secrets in client-reachable modules.
