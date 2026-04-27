# Storage backends

All four implement the same `Repository` trait (`crates/ferro-storage/src/repo.rs`). Choose by use case; switch via `[storage] kind` in `ferro.toml`.

| Backend       | When to pick                                  | Multi-writer | Search           | Versioning |
|---------------|-----------------------------------------------|--------------|------------------|------------|
| `fs-json`     | Demos, single-tenant, dev                     | no           | substring scan   | yes        |
| `fs-markdown` | Git-friendly editorial workflows              | no           | substring scan   | yes        |
| `surreal`     | Embedded NoSQL, rich queries, single host     | no (embedded)| `CONTAINS`       | yes        |
| `postgres`    | Production, multi-writer, indexed search      | yes          | `ILIKE` (JSONB)  | yes        |

## fs-json

```toml
[storage]
kind = "fs-json"
path = "./data"
```

### Layout

```
data/
  sites/<site-id>.json
  types/<type-id>.json
  content/<content-id>.json
  users/<user-id>.json
  roles/<role-id>.json
  media/<media-id>.json
  versions/<content-id>/<version-id>.json
```

Plain JSON. Trivially diffable, scriptable, and survives `git`. No locking — concurrent writes are last-write-wins.

### Pros

- Zero deps. No daemons, no driver init.
- Inspect/edit content directly from a text editor.
- `ferro export` is essentially `tar c data/`.

### Cons

- O(n) listing; doesn't scale past a few thousand entries.
- Search is naive substring scan over serialized JSON.
- No transactions — partial writes possible on crash.

## fs-markdown

```toml
[storage]
kind = "fs-markdown"
path = "./content"
```

### Layout

```
content/
  _meta/
    sites/<site-id>.json
    types/<type-id>.json
    users/<user-id>.json
    roles/<role-id>.json
    media/<media-id>.json
    versions/<content-id>/<version-id>.json
  <site-slug>/<type-slug>/<content-slug>.<locale>.md
```

Each `.md` carries YAML front-matter (full `Content` minus `body`) plus a markdown body. On read, the body is folded into `data["body"]` for API parity with the other backends.

### Pros

- Native Git workflow: PRs for content edits, blame for "who wrote this paragraph", branches for staging changes.
- Authors can edit in any text editor and push.
- File system structure mirrors site information architecture.

### Cons

- Same multi-writer / scaling limits as fs-json.
- Schema migrations rewrite many files; large sites pay seek time.
- Content data must shape into one markdown body + structured front-matter.

## surreal-embedded

```toml
[storage]
kind = "surreal-embedded"
path = "./data/ferro.db"
namespace = "ferro"
database = "main"
```

Embedded SurrealDB on RocksDB. Same process, no network. Runs `surreal.surql` migrations on connect.

### Pros

- Real query engine: `CONTAINS`, indexes, composite queries, transactions.
- Single binary — no separate database to provision.
- Schemaless flexibility on the storage side; Ferro's `Repository` enforces shape at the API boundary.

### Cons

- Embedded ⇒ single-process. Spinning a second `ferro serve` against the same path will fail to acquire the RocksDB lock.
- RocksDB tuning matters at scale; defaults are reasonable for ≤100k content rows.
- For multi-writer, switch to remote SurrealDB (separate config) or Postgres.

## postgres

```toml
[storage]
kind = "postgres"
url = "postgres://ferro:secret@localhost:5432/ferro"
max_connections = 10
```

Production target. Schema:

```
sites          (id, slug, name, ..., created_at, updated_at)
content_types  (id, site_id, slug, name, fields jsonb, ...)
content        (id, site_id, type_id, slug, locale, status, data jsonb, author_id, ..., updated_at)
users          (id, email, handle, ..., password_hash, totp_secret, password_changed_at, active)
roles          (id, name, permissions jsonb)
user_roles     (user_id, role_id)
media          (id, site_id, key, filename, mime, size, alt, kind, ...)
content_versions (id, content_id, site_id, type_id, slug, locale, status, data jsonb, captured_at, parent_version)
sessions       (token, user_id, expires_at, ...)
```

### Pros

- Multi-writer, ACID, indexes, foreign keys.
- JSONB content with `?` / `@>` / `->>` operators for query.
- Hot-standby replication, point-in-time recovery, pg_dump backups — boring infra wins.
- Search via `ILIKE '%q%'` on `data::text` today; upgrade path to `tsvector`/`pg_trgm` when needed.

### Cons

- Operational overhead: provision, secure, back up.
- JSONB indexes need attention as content grows.

### Migrations

Run on `serve` boot via `repo.migrate()`, which calls `sqlx::migrate!`. Migrations live under `crates/ferro-storage/migrations/postgres/`. To preview without applying, `sqlx migrate info`.

### Pooling

`max_connections` defaults to 10. Tune to roughly `2 * max_concurrent_writes`. Small sites can leave it.

## Choosing for production

- **Single host, < 10k content, single editor**: `surreal-embedded`. One binary, real query engine.
- **Single host, want Git history of content**: `fs-markdown`. Push edits as PRs.
- **Production, multi-writer, indexed search**: `postgres`. Boring, scalable.
- **CI fixtures, prototypes**: `fs-json`. Trivially scriptable.

## Switching backends

There is no online migration today. The path is:

```sh
ferro export --out site.bundle.json --include-media
# edit ferro.toml to switch [storage] kind
ferro init --storage <new>     # if the dirs don't exist
ferro import site.bundle.json --mode replace
```

Test the import on a copy first. The bundle format is stable; backend-specific quirks (RocksDB tuning, JSONB indexes) are reset.
