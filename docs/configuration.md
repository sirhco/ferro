# Configuration reference

`ferro.toml` is the single config file. Loaded by `ferro serve` (and most `ferro admin *` subcommands) from the path passed via `--config`, defaulting to `./ferro.toml`.

## Skeleton

```toml
[server]
bind = "0.0.0.0:8080"
public_url = "http://localhost:8080"
admin_enabled = true

[storage]
kind = "fs-json"      # or "surreal-embedded" | "fs-markdown" | "postgres"
path = "./data"

[media]
kind = "local"        # or "s3" | "gcs"
path = "./media-store"
base_url = "http://localhost:8080/media"

[auth]
session_secret = "CHANGE_ME_..."
jwt_issuer = "ferro"
jwt_secret = "CHANGE_ME_..."   # overridden by FERRO_JWT_SECRET env
allow_public_signup = false

[plugins]
dir = "./plugins"
max_memory_mb = 128
fuel_per_request = 10_000_000

# Optional: zero or more outbound webhooks
[[webhooks]]
url = "https://hooks.example.com/ferro"
events = ["content.created", "content.published"]
secret = "shared-with-receiver-for-hmac"
timeout_ms = 5000
name = "production-mirror"
```

## `[server]`

| Key             | Type    | Default                   | Notes                                                         |
|-----------------|---------|---------------------------|---------------------------------------------------------------|
| `bind`          | string  | `"0.0.0.0:8080"`          | TCP listener address. Override with `ferro serve --bind`.     |
| `public_url`    | string  | none                      | Used for `Set-Cookie` `Domain` (TODO) + absolute media URLs.  |
| `admin_enabled` | bool    | `true`                    | When `false`, `/admin/*` returns 404. (Future flag, not yet enforced.) |

## `[storage]`

Tag `kind` selects the backend. Each variant has different keys:

### `fs-json`

```toml
[storage]
kind = "fs-json"
path = "./data"
```

One JSON file per record under `<path>/<table>/<id>.json`. Versions snapshots under `<path>/versions/<content-id>/<version-id>.json`. Best for demos and single-user dev.

### `fs-markdown`

```toml
[storage]
kind = "fs-markdown"
path = "./content"
```

Git-friendly. Metadata under `<path>/_meta/{sites,types,users,roles,media,versions}/`. Content under `<path>/<site-slug>/<type-slug>/<content-slug>.<locale>.md` with YAML front-matter. Body markdown is folded into `data["body"]` on read.

### `postgres`

```toml
[storage]
kind = "postgres"
url = "postgres://ferro:secret@localhost:5432/ferro"
max_connections = 10        # optional
```

Production. Runs migrations on startup via sqlx. JSONB-backed content, ILIKE-based search.

### `surreal-embedded`

```toml
[storage]
kind = "surreal-embedded"
path = "./data/ferro.db"
namespace = "ferro"
database = "main"
```

Embedded SurrealDB on RocksDB. Single-process; for multi-writer use the `surreal` (remote) variant or Postgres.

## `[media]`

### `local`

```toml
[media]
kind = "local"
path = "./media-store"
base_url = "http://localhost:8080/media"
```

Filesystem under `path`. Files exposed at `/api/v1/media/{id}/raw`.

### `s3`

```toml
[media]
kind = "s3"
bucket = "ferro-media"
region = "us-east-1"
prefix = "prod/"
endpoint = "https://s3.amazonaws.com"   # optional, for MinIO/R2
public_base_url = "https://cdn.example.com/prod/"
```

AWS credentials picked up via the standard chain (env, `~/.aws/credentials`, IAM role).

### `gcs`

```toml
[media]
kind = "gcs"
bucket = "ferro-media"
prefix = "prod/"
service_account_path = "/etc/ferro/sa.json"
public_base_url = "https://storage.googleapis.com/ferro-media/prod/"
```

## `[auth]`

| Key                  | Type   | Default     | Notes                                                                |
|----------------------|--------|-------------|----------------------------------------------------------------------|
| `session_secret`     | string | required    | Used for legacy session signing; rotate to invalidate cookies.        |
| `jwt_issuer`         | string | `"ferro"`   | `iss` claim on access JWTs.                                           |
| `jwt_secret`         | string | required    | HS256 signing key. **Override via `FERRO_JWT_SECRET` env in prod.**   |
| `allow_public_signup`| bool   | `false`     | When `true`, `POST /api/v1/auth/signup` works without admin auth.    |
| `access_ttl_secs`    | int    | `43200`     | Access JWT lifetime. (Future knob; today hard-coded to 12h.)          |
| `refresh_ttl_days`   | int    | `30`        | Refresh-token lifetime. (Future knob; today hard-coded to 30d.)       |

Best-practice: leave `jwt_secret` as a placeholder in `ferro.toml`, set `FERRO_JWT_SECRET=$(openssl rand -hex 32)` in your service manager / docker-compose.

## `[plugins]`

| Key                | Type | Default          | Notes                                                                |
|--------------------|------|------------------|----------------------------------------------------------------------|
| `dir`              | str  | `"./plugins"`    | Drop-in directory for `.wasm` plugin components.                     |
| `max_memory_mb`    | int  | `128`            | Per-plugin memory cap (wasmtime `ResourceLimiter`).                  |
| `fuel_per_request` | int  | `10_000_000`     | Per-invocation fuel budget. Prevents runaway plugins.                |

WASM plugin host is scaffolded; in-process Rust hooks (LoggingHook, WebhookHook) are active today.

## `[[webhooks]]`

Repeat the table to register multiple hooks.

| Key          | Type      | Default | Notes                                                                          |
|--------------|-----------|---------|--------------------------------------------------------------------------------|
| `url`        | string    | required| HTTPS endpoint to POST events to.                                              |
| `events`     | [string]  | `[]`    | Filter; empty = all events. Names: `content.created`, `content.updated`, `content.published`, `content.deleted`, `type.migrated`. |
| `secret`     | string    | none    | If set, body is signed with HMAC-SHA256, header `X-Ferro-Signature`.           |
| `timeout_ms` | int       | `5000`  | Per-call timeout. Failures are logged, never block the originating mutation.   |
| `name`       | string    | `url`   | Friendly name for tracing.                                                     |

## Environment overrides

| Var                  | Purpose                                                              |
|----------------------|----------------------------------------------------------------------|
| `FERRO_JWT_SECRET`   | Wins over `[auth] jwt_secret` in `ferro.toml`. Use this in prod.    |
| `FERRO_SITE_DIR`     | Override the cargo-leptos build output dir for `/pkg/*` static.     |
| `RUST_LOG`           | tracing-subscriber filter. Default: `info,ferro=debug`.              |
| `DATABASE_URL`       | (postgres backend) overrides `[storage].url`.                       |
| `AWS_*`              | Standard AWS credential chain for the S3 media backend.             |

## Validating your config

```sh
ferro serve --config ./ferro.toml
```

Startup will fail fast on:
- Unknown `kind`.
- Missing required keys.
- Storage connect / migrate errors.
- Bad JWT secret length (<32 bytes).
- Webhook URLs that don't parse.

Inspect emitted tracing for `webhook ... registered` / `webhook ... failed to register` lines.
