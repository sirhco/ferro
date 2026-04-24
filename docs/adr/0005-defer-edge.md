# ADR-0005: Defer edge-runtime target to v2

**Status:** Accepted
**Date:** 2026-04-24

## Context

The project's pitch names edge deploys (Cloudflare Workers, Fastly Compute). Edge runtimes ban filesystem, threads, long-lived sockets, and most embedded databases — directly incompatible with SurrealDB embedded, wasmtime, and local media.

## Decision

Target **self-hosted binary / container** for v1. Edge is a **v2 goal**. The `Repository` and `MediaStore` traits are shaped so KV/D1/R2 backends can slot in without API changes.

## Rationale

- Edge-native forces the smallest common denominator (KV only, ~1 MB WASM limit on Workers, no wasmtime). Designing to that from day 1 kneecaps self-hosters who want Postgres, SurrealDB, and filesystem media.
- Plugin host (wasmtime) does not run inside CF Workers — plugins would need a different runtime. Cleaner to defer than to split the plugin story in half.
- We still get "edge-friendly" properties (stateless request handling, cache-friendly APIs) without committing to the runtime.

## Alternatives Considered

- **Edge-first**: Would mandate KV-shaped storage, no wasmtime plugins, no Postgres. Kills self-host story.
- **Dual build targets**: Maintainable later; premature now.

## Consequences

- README + marketing must not overclaim edge support in v1.
- When we tackle v2, expect: KV `Repository` impl, WASM-in-WASM plugin runtime (nested wasm not yet viable — plugins likely disabled on edge).
