# Troubleshooting

Symptoms → causes → fixes.

## Login

### "401 Unauthorized" on `/api/v1/auth/login`

- **Bad credentials.** Check `ferro admin list-users` to confirm the user exists with `active: true`.
- **JWT secret mismatch after restart.** A `serve` restart with a different `FERRO_JWT_SECRET` invalidates all in-flight access tokens. Log out (clear `localStorage`) and back in.

### MFA challenge keeps failing

- **Clock skew.** TOTP accepts ±1 30-second window. Sync the device's clock and the server's clock (NTP).
- **Wrong secret.** Re-enroll: Settings → "Disable" (with a current code) → "Set up TOTP".

### "429 Too Many Requests" on login

Per-IP rate limit (default 10/min). Wait the `retry_after_ms` from the response, or check that your reverse proxy is forwarding `X-Real-IP` so the bucket isn't shared across clients (`0.0.0.0` fallback).

## Admin SPA

### Hydration error: "expected marker, found <input>"

Cached old WASM. Hard-refresh (Cmd/Ctrl + Shift + R). The current admin runs in CSR mode and shouldn't surface this — if you see it after a clean refresh, the build didn't pick up the latest sources.

### "Loading…" forever after login

Browser DevTools → Network. Look for failed `/api/v1/auth/me` or `/api/v1/types`:

- 401: token rejected. Clear `localStorage`, sign in again.
- 404: route mismatch (server old). `cargo build -p ferro-cli && systemctl restart ferro`.
- net::ERR_CONNECTION_REFUSED: server isn't on the expected port. Check the boot log.

### Media upload returns 404

The default site wasn't seeded. Check the boot log for `seeded default site`. If absent (e.g. you upgraded from a pre-auto-seed version), seed manually via the storage repo or restart `ferro serve`.

### `/admin/...` returns plain "404"

Either:
- `--site-dir` is wrong ⇒ `pkg/` doesn't exist ⇒ Leptos shell can't load. Boot log will show "admin SPA assets not found at `<path>`". Run `ferro build` or pass the right `--site-dir`.
- The `[server] admin_enabled = false` flag is set (future flag).

## Storage

### `surreal-embedded`: "could not acquire lock"

Two `ferro serve` processes pointed at the same RocksDB path. Stop the other one or move to remote SurrealDB / Postgres.

### `postgres`: "schema does not exist"

`ferro serve` runs migrations on boot. If you see this, migrations failed. Check stderr for the sqlx error (often a permissions issue on the `ferro` Postgres role).

### `fs-markdown`: missing front-matter error

A hand-edited `.md` file lost its YAML header. Restore from git or rewrite the front-matter (`---\n...full Content...\n---\n<body>`).

## Webhooks

### Receiver gets nothing

- Check the boot log for `webhook ... registered` per `[[webhooks]]` entry.
- Trigger a test event: `POST /api/v1/content/<type>` succeeds.
- `RUST_LOG=debug,ferro::webhook=trace` to see per-delivery attempts.

### Signature mismatch

- HMAC-SHA256 over the **raw body bytes**, not a re-serialized version.
- Hex-encoded (lowercase). `Authorization: hex(hmac_sha256(secret, body))`.
- Header is `X-Ferro-Signature`. Use `hmac.compare_digest` on the receiver — never plain `==`.

## Build

### `cargo leptos build` fails with "No bin targets found for member ferro-admin"

Workspace `[[workspace.metadata.leptos]]` config is missing or stale. Confirm the root `Cargo.toml` has a single `[[workspace.metadata.leptos]]` block with `bin-package = "ferro-cli"`, `bin-target = "ferro"`, `bin-exe-name = "ferro"`, `lib-package = "ferro-admin"`.

### WASM build error: "wasm32-unknown-unknown targets are not supported by default"

`getrandom 0.3` needs the `wasm_js` backend on `wasm32`. The repo's `.cargo/config.toml` already adds `--cfg=getrandom_backend="wasm_js"` for that target — confirm it didn't get reverted, and that `ferro-admin/Cargo.toml`'s `[target.'cfg(target_arch = "wasm32")'.dependencies]` block carries `getrandom = { ..., features = ["wasm_js"] }` and `uuid = { ..., features = ["js"] }`.

### Server complains about `/pkg/ferro_admin_bg.wasm` 404

cargo-leptos 0.3 renames `<name>_bg.wasm` → `<name>.wasm`, but leptos 0.8's `HydrationScripts` still requests `_bg`. The server mirrors the file under both names on boot. If you see this 404 anyway, confirm `target/site/pkg/ferro_admin.wasm` exists and that the boot user can write the alias next to it.

## Runtime

### `EADDRINUSE` on serve

Port already bound. `lsof -nP -iTCP:8080 -sTCP:LISTEN` to find the offender.

### High memory after long uptime

Likely the `MemorySessionStore` accumulating expired tokens. Switch to the Postgres session store (matches your storage backend if you're already on Postgres). Auto-purge runs periodically but can lag under load.

### Slow listings

`fs-json` / `fs-markdown` walk the directory per request. For lists > a few hundred entries, switch to `surreal-embedded` or `postgres`.

## Importing data

### `ferro import` reports "site already exists"

Use `--mode replace` to wipe and re-create, or `--mode merge` (default) to upsert. Note: `replace` is destructive and deletes any content not present in the bundle.

### Media imports skip files

The bundle was created without `--include-media`. Re-export with the flag, or upload media manually after import.

## Performance

### High CPU during admin browsing

The admin SPA fetches `/types` on every navigation today. Cache headers on these endpoints + ETags are on the roadmap.

### Long-tail webhook latency

Webhook calls happen synchronously after the storage write. If a receiver is slow, the originating request still returns fast, but subsequent hooks queue behind it. Move slow integrations to a downstream queue (your webhook receiver pushes onto SQS/RabbitMQ and returns 202).

## When you need help

1. `RUST_LOG=debug,ferro=trace ferro serve` — enable trace logs.
2. `git rev-parse HEAD` and the output of `ferro --version`.
3. Reproduce against `examples/starter-blog` if possible — eliminates config drift as a cause.
4. Open an issue with config (secrets redacted), the failing request, and the relevant trace lines.
