# ADR-0002: wasmtime + WIT for the plugin system

**Status:** Accepted
**Date:** 2026-04-24

## Context

Ferro advertises a "WASM-first plugin system". We need a host that runs untrusted, multi-language plugins with strong isolation, capability gating, and preemptive timeouts.

## Decision

Use **`wasmtime`** with the **WebAssembly Component Model** and **WIT** for typed interfaces.

## Rationale

- Wasmtime is the reference Bytecode Alliance runtime — stable, well-audited, shipped in production at multiple orgs.
- Component Model + WIT give us typed, language-independent interfaces. Rust plugins via `wit-bindgen`, JS via `componentize-js`, Go via TinyGo.
- Fine-grained resource control: `fuel`, `epoch_interruption`, `StoreLimits` for memory caps.
- Capability model is explicit — nothing ambient. Fits our "no ambient authority" security stance.

## Alternatives Considered

- **extism**: Faster to integrate, polyglot PDK, but thinner ABI (JSON in/out), weaker typing, fewer runtime controls. Good for an MVP; ceilings show up around multi-import / resource handles.
- **wasmer**: Comparable to wasmtime. Wasmtime's Component Model support is more mature today.
- **JS-based plugin host**: Wrong ecosystem for us; duplicates an engine.

## Consequences

- Higher upfront implementation cost (WIT authoring, `wasmtime::component` plumbing).
- Plugin authors need the component toolchain. We ship templates + `ferro plugin new`.
- We own the host ABI stability story (WIT versioning with semver).
