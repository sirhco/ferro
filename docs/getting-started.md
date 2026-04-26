# Getting started

A fresh Ferro instance from zero to logged-in admin in under five minutes.

## Prerequisites

- Rust nightly (the workspace `rust-toolchain.toml` pins the channel; `rustup` will pick it up automatically).
- `cargo-leptos` for building the admin SPA's WASM bundle:
  ```sh
  cargo install cargo-leptos
  ```
- macOS / Linux. Windows works under WSL2; native Windows is untested.

## Build

From the workspace root:

```sh
# Server binary (`ferro` CLI)
cargo build -p ferro-cli

# Admin SPA (WASM + JS + CSS into target/site/pkg/)
cargo leptos build --project ferro-admin
```

Subsequent rebuilds are incremental. Add `--release` to either command for production binaries.

## Initialize a project

Pick any empty directory. `ferro init` writes a `ferro.toml` plus storage/media/plugin subdirs there.

```sh
mkdir -p ~/myferro && cd ~/myferro
~/path/to/ferro/target/debug/ferro init --storage fs-json
```

`--storage` accepts `surreal | fs-json | fs-markdown | postgres`. `fs-json` is the simplest — content lives as plain JSON files under `./data/`.

The init writes:

```
./ferro.toml          # config (storage, media, auth, plugins, webhooks)
./data/               # storage root (per backend)
./media-store/        # uploaded files (local backend)
./plugins/            # WASM plugin drop-in dir
./content/            # legacy content (unused by fs-json backend)
```

## Create the first admin user

The repo starts empty. Bootstrap an admin via the CLI:

```sh
ferro admin create-user \
  --email you@example.com \
  --handle you \
  --password 'pick-a-real-password' \
  --with-admin
```

`--with-admin` seeds the `admin` role (full permissions) and attaches it to the user. Idempotent — safe to re-run.

Verify:

```sh
ferro admin list-users
```

## Run the server

```sh
FERRO_JWT_SECRET=$(openssl rand -hex 32) ferro serve
```

You should see:

```
INFO ferro_cli::serve: seeded default site
INFO ferro_cli::serve: ferro listening on http://0.0.0.0:8080 (admin SPA hydrates from .../target/site/pkg)
```

`seeded default site` runs once on first boot — a single-tenant `default` site is auto-created so admin flows (media, content lists) can resolve "the site".

## Open the admin

Browse to <http://localhost:8080/admin>. Sign in with the credentials you just created.

You land on the dashboard. The side nav offers:

| Section   | Purpose                                                               |
|-----------|-----------------------------------------------------------------------|
| Dashboard | Quick links + count of registered content types                       |
| Content   | Browse, edit, publish, delete entries — grouped by content type       |
| Schema    | Designer for content types (slug, name, fields). Triggers migrations. |
| Media     | Upload + browse assets (local FS / S3 / GCS depending on config)      |
| Users     | List users with roles + active flag (requires `ManageUsers`)          |
| Plugins   | Capability info; webhooks live in `ferro.toml`                        |
| Settings  | Change password + enroll TOTP (2FA) + log out                         |

## Define your first content type

Schema → "New type". Slug `post`, name `Post`. Add fields with the quick-add row at the bottom (text/richtext/number/boolean/date). Press "Create". Switch to Content → "New entry" to author one.

For a code-first workflow, see [`examples/starter-blog`](../examples/starter-blog/) — types declared via `#[derive(ContentType)]` instead of the UI.

## Where to next

- [`configuration.md`](configuration.md) — every `ferro.toml` knob
- [`admin-ui.md`](admin-ui.md) — feature-by-feature tour
- [`rest-api.md`](rest-api.md) — REST endpoint reference
- [`auth.md`](auth.md) — JWT/refresh/MFA/RBAC details
- [`deployment.md`](deployment.md) — Dockerfile + reverse-proxy + production hardening
