# CLI reference

The `ferro` binary is the operator entry point: project init, server, migrations, import/export, plugin tooling, and offline admin operations.

```sh
ferro --help
```

All commands honor `--config <path>` (default `./ferro.toml`).

## `ferro init`

Scaffold a project in the current directory.

```sh
ferro init [--storage surreal|fs-json|fs-markdown|postgres] [PATH]
```

Writes `ferro.toml` plus `data/`, `media-store/`, `plugins/`, `content/` subdirs. `--storage` skips the interactive prompt.

## `ferro serve`

Run the HTTP server.

```sh
ferro serve [--bind <addr>] [--site-dir <path>]
```

| Flag           | Default            | Notes                                                               |
|----------------|--------------------|---------------------------------------------------------------------|
| `--bind`       | `[server].bind`    | Override the listener address.                                      |
| `--site-dir`   | resolved at boot   | Cargo-leptos build output. Falls back to `./target/site`, then a baked workspace path. |

On boot:
- Connects storage, runs `migrate()`.
- Auto-seeds the default site if the repo has none.
- Mirrors `pkg/ferro_admin.wasm` ↔ `pkg/ferro_admin_bg.wasm` so leptos's hydration scripts resolve either name.
- Logs `ferro listening on http://<bind> (admin SPA hydrates from <pkg-dir>)`.

## `ferro build`

Build the admin SPA + brotli-compress.

```sh
ferro build [--skip-leptos] [--quality 0..=11] [--site-dir target/site]
```

Equivalent to `cargo leptos build --project ferro-admin --release --split` followed by walking `target/site/` and writing `.br` siblings for `.wasm` / `.js` / `.css` / `.svg`.

## `ferro migrate`

Run pending storage migrations explicitly. `serve` runs them automatically on boot, but separating gives you a dry-run / staged-deploy workflow.

```sh
ferro migrate
```

## `ferro export` / `ferro import`

Bundle the repo into a single JSON file (sites + types + content + roles + users + optionally media bytes), or restore one.

```sh
ferro export --out site.bundle.json [--include-media]
ferro import site.bundle.json [--mode merge|replace]
```

`--mode merge` upserts; `replace` wipes existing content first. Media bytes are base64-embedded when `--include-media` is set, otherwise the bundle keeps only metadata.

## `ferro plugin`

Manage WASM plugin components dropped into `[plugins].dir`.

```sh
ferro plugin list
ferro plugin inspect <name>
ferro plugin reload
```

(WASM host scaffolded; today this enumerates files. Live capability granting + reload arrives with the wasmtime component-model loader.)

## `ferro admin *`

Operator tooling that bypasses the HTTP API. Reads `ferro.toml`, opens the same storage backend the server uses, and writes directly through the repo trait. Useful for bootstrapping the first user, recovering a lockout, or scripted role grants.

```sh
ferro admin <subcommand>
```

| Subcommand           | Purpose                                                                                          |
|----------------------|--------------------------------------------------------------------------------------------------|
| `create-user`        | Create a user. `--email`, `--handle`, `--password` required. `--role <id|name>` repeatable. `--with-admin` seeds + attaches the `admin` role. `--inactive` skips activation. |
| `list-users`         | Print all users (password hashes redacted).                                                      |
| `create-role`        | Create a role. `--name`, optional `--description`, `--preset full|editor|viewer|publisher`.      |
| `list-roles`         | Print all roles + their permission counts.                                                       |
| `grant-role`         | Attach an existing role to an existing user. `--user <id|email>`, `--role <id|name>`.           |
| `seed-admin-role`    | Idempotently create the `admin` role (`Permission::Admin` ⇒ wildcard).                           |

### Common flows

```sh
# First-run admin
ferro admin create-user --with-admin \
  --email me@example.com --handle me --password 'correct-horse-battery-staple'

# Promote an existing user
ferro admin grant-role --user me@example.com --role admin

# Recover a lockout — issue a temporary password, log in, change it via UI
ferro admin create-user --with-admin --email recovery@example.com \
  --handle recovery --password 'temp-rotate-immediately'
```

## Exit codes

- `0` — success.
- `1` — generic error (bad config, storage error, validation failure). Stderr carries the chain.
- `2` — `clap` argument parsing error.

## Logging

Emit JSON logs in production:

```sh
RUST_LOG=info,ferro=debug ferro serve 2>&1 | jq -c .
```

Targets worth filtering on:
- `ferro_api` — request handlers.
- `ferro_cli::serve` — boot sequence.
- `ferro::webhook` — outbound webhook deliveries.
- `ferro_storage::*` — backend specifics.
