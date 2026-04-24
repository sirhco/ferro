# ADR-0003: SurrealDB embedded as the default dev backend

**Status:** Accepted
**Date:** 2026-04-24

## Context

Ferro supports multiple storage backends. We need a zero-config default that makes `ferro init && ferro serve` work without external dependencies.

## Decision

Default backend: **SurrealDB embedded** (RocksDB kv). Feature flag `storage-surreal`. Flat-JSON backend is available as an even simpler alternative (`storage-fs-json`), and Postgres is the recommended production backend (`storage-postgres`).

## Rationale

- Embedded mode → single binary, no Docker, no sidecar.
- Same driver works for remote SurrealDB in prod — one codepath.
- Graph + document + relational in one engine fits our content model (refs between entries).
- Schemaful mode lets us enforce content-type shapes at the DB layer.

## Alternatives Considered

- **SQLite**: Simpler, tremendously stable. Weaker graph story; we'd bolt on ad-hoc join tables for references.
- **sled / redb**: Lower-level; we'd build a query layer on top.
- **Postgres only**: Requires server; rules out zero-config dev.

## Consequences

- SurrealDB is newer than SQLite/Postgres; we track upstream releases closely and pin minor versions.
- Storage trait is expressive enough that `fs-json` and `postgres` implementations are straightforward — users can move off Surreal with `ferro export | ferro import`.
