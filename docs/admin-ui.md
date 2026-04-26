# Admin UI guide

The Leptos SPA at `/admin`. Renders client-side; talks REST. Auth via JWT in `localStorage` with one-shot refresh-token rotation on 401.

## Sign in

`/admin/login`. Email + password. On success the `token` and `refresh_token` go to `localStorage` (keys `ferro.admin.token`, `ferro.admin.refresh`). If the user has TOTP enrolled, the response is an `mfa_token` instead — the SPA redirects to `/admin/mfa` for the 6-digit code step.

## Dashboard

Welcome banner with display-name fallback to email. Quick links to all sections plus a count of registered content types. Lightweight; no API calls beyond the boot hydration.

## Content

`/admin/content[/:type_slug]`.

- Type picker (`<select>`) — switching navigates to `/admin/content/<slug>` so the URL is shareable.
- Per-row actions: **Edit** (pencil), **Publish**, **Delete** (with native `confirm()`).
- Updated timestamps formatted via `format_dt` (drops fractional seconds + trailing `Z`).
- New entry → `/admin/content/<slug>/new`.

### Editor

`/admin/content/<slug>/new` or `/admin/content/<slug>/edit/<entry-slug>`. JSON `data` field is the canonical content payload — the structured field-by-field editor is on the roadmap. Slug is editable; backend re-keys storage and reuses content id.

For an existing entry, a Versions card lists prior snapshots most-recent-first. **Restore** writes the snapshot back through the standard update path, which itself snapshots the live state first — so restores are reversible.

## Schema

`/admin/schema`. Lists existing types with edit + delete. Editing runs the migrator across existing rows; the resulting `rows_migrated` count surfaces as a toast.

`/admin/schema/new` or `/admin/schema/edit/<slug>` opens the designer. Quick-add at the bottom mints `FieldDef` entries for the common kinds:

| Preset    | `kind` produced                                       |
|-----------|-------------------------------------------------------|
| Text      | `{ "type": "text", "multiline": false }`              |
| Rich text | `{ "type": "rich_text", "format": "markdown" }`       |
| Number    | `{ "type": "number", "int": false }`                  |
| Boolean   | `{ "type": "boolean" }`                               |
| Date      | `{ "type": "date" }`                                  |

For other kinds (`enum`, `slug`, `reference`, `media`, `json`), edit the JSON array directly. Refer to `ferro_core::FieldKind` for the full enum.

## Media

`/admin/media`. Upload form (multipart, optional alt text) plus a thumbnail/icon grid of existing assets.

- Upload size cap: 25 MiB (server-enforced).
- MIME sniffed via `mime_guess` if the upload omits Content-Type.
- Image kind ⇒ `<img src="/api/v1/media/{id}/raw">`. Other kinds get a placeholder with the kind label.
- Per-tile **View** opens the raw URL in a new tab; **Delete** drops the metadata and best-effort removes the blob.

`/api/v1/media/{id}/raw` is intentionally **public** — anchor tags can't carry a Bearer header. Treat uploads as world-readable. Sensitive assets belong in a separate signed-URL store.

## Users

`/admin/users`. Lists email, handle, role count, active flag. Read-only today; create/edit/delete must go through the CLI (`ferro admin create-user`, `ferro admin grant-role`). Returns 403 + a friendly explanation if the caller lacks `ManageUsers`.

## Plugins

`/admin/plugins`. Status + roadmap card. Outbound webhooks live in `ferro.toml`; the WASM plugin host is scaffolded but not yet loading components at runtime.

## Settings

`/admin/settings`. Three cards:

- **Change password** — verifies current via argon2id, hashes new with argon2id, sets `password_changed_at`. Tokens minted before this timestamp will be rejected on next call (logout-all-sessions semantics).
- **Two-factor authentication**:
  1. "Set up TOTP" mints a fresh secret + `otpauth://` URI (no persist).
  2. Scan with an authenticator app (1Password, Google Authenticator, Raivo).
  3. Enter a 6-digit code → `/auth/totp/enable` verifies + persists.
  4. Disable requires a current code (proves you still have the device).
- **Session** — current email + log out (revokes the refresh token server-side, clears local tokens).

## Toasts

Bottom-right ephemeral cards. Auto-dismiss at 2.5s. `setToast_ok` / `setToast_err` from `AdminState`.

## Auth state

- Boot effect (`bootstrap_after_mount`) calls `/me` + `/types` once the WASM mounts.
- `Shell` redirects to `/admin/login` when bootstrap finishes with no user (token missing or invalid).
- Every outbound request goes through `api::request` which:
  1. Attaches `Authorization: Bearer <token>` if present.
  2. On 401, calls `try_refresh` (single-flight) — if it succeeds, retries the original request once.
  3. Throws `ApiError::Http { status, message, body }` on non-2xx, surfacing the server's JSON `message`.

## Keyboard shortcuts

None today. The pages are small enough that the keyboard stays useful via Tab + Enter on forms.

## Theming

`crates/ferro-admin/style/main.scss`. Supports `prefers-color-scheme: light dark` via CSS custom properties. Edit and rebuild:

```sh
cargo leptos build --project ferro-admin
# bin restart not required — only static CSS changed
```

## Troubleshooting

- **404 on POST /api/v1/media** — the boot path didn't seed a default site, or you're hitting an old binary. Restart `ferro serve`; check the log for `seeded default site`.
- **Hydration errors after edit** — admin runs in CSR mode; pure client-side render. If you see "expected marker" panics, the cached WASM is from an older SSR-hydrate build. Hard-refresh (Cmd/Ctrl + Shift + R).
- **401 immediately after login** — JWT secret mismatch between issuer and verifier. Rebooting `ferro serve` mid-session invalidates stored tokens; log out and back in.
