# REST API reference

Base path: `/api/v1`. JSON in / JSON out. Auth via `Authorization: Bearer <jwt>`. Read endpoints are public unless noted; mutating endpoints need either the `Write` permission (content) or `ManageUsers` / `ManageSchema`.

OpenAPI spec lives at `/api/openapi.json`; Swagger UI at `/api/docs`.

## Conventions

- Errors: `{ "error": "<code>", "message": "<human>" }`. Codes: `not_found`, `unauthorized`, `forbidden`, `bad_request`, `rate_limited`, `internal`.
- Times: RFC 3339 UTC.
- IDs: ULID-shaped 26-char Crockford strings, prefixed (e.g. `01HK...`). Typed via `SiteId`, `ContentId`, etc. on the wire as plain strings.
- Pagination: `?page=1&per_page=200`. Response: `{ items: [...], total: N, page, per_page }`.

## Auth

| Method | Path                                   | Auth   | Notes                                                                  |
|--------|----------------------------------------|--------|------------------------------------------------------------------------|
| GET    | `/api/v1/auth/csrf`                    | none   | Mints a CSRF double-submit token, sets `ferro_csrf` cookie.            |
| POST   | `/api/v1/auth/login`                   | none   | Body: `{email, password}`. Returns `{token, refresh_token, user}` or `{mfa_required: true, mfa_token}` if TOTP enrolled. Rate-limited. |
| POST   | `/api/v1/auth/refresh`                 | none   | Body: `{refresh_token}`. Rotates: old token revoked, new pair returned. Rate-limited. |
| POST   | `/api/v1/auth/logout`                  | bearer | Body: `{refresh_token?}`. Revokes the refresh token server-side. Always 204. |
| GET    | `/api/v1/auth/me`                      | bearer | Returns the current user (redacted) + `totp_enabled` boolean.          |
| POST   | `/api/v1/auth/signup`                  | none   | Public sign-up. Disabled by default; flip `[auth] allow_public_signup`. |
| POST   | `/api/v1/auth/change-password`         | bearer | Body: `{current_password, new_password}`. Sets `password_changed_at`.   |
| POST   | `/api/v1/auth/totp/setup`              | bearer | Mints secret + `otpauth://` URI. Doesn't persist; commit with `enable`. |
| POST   | `/api/v1/auth/totp/enable`             | bearer | Body: `{secret, code}`. Verifies + persists.                            |
| POST   | `/api/v1/auth/totp/disable`            | bearer | Body: `{code}`. Verifies current code, clears secret.                  |
| POST   | `/api/v1/auth/totp/login`              | none   | Body: `{mfa_token, code}`. Exchanges challenge for real session pair.   |

## Sites

| Method | Path                | Auth   | Notes                                                                  |
|--------|---------------------|--------|------------------------------------------------------------------------|
| GET    | `/api/v1/sites`     | none   | Lists sites. Single-tenant deployments return one row.                 |

## Content types

| Method | Path                        | Auth                     | Notes                                                                          |
|--------|-----------------------------|--------------------------|--------------------------------------------------------------------------------|
| GET    | `/api/v1/types`             | none                     | All types for the active site.                                                 |
| POST   | `/api/v1/types`             | bearer + `ManageSchema`  | Create. Body matches `ContentType`. `site_id` is overridden server-side.       |
| GET    | `/api/v1/types/{slug}`      | none                     | Lookup by slug.                                                                |
| PATCH  | `/api/v1/types/{slug}`      | bearer + `ManageSchema`  | Replace. Diffs old vs new and runs `apply_changes` migrator. Returns `{ type, rows_migrated, changes[] }`. |
| DELETE | `/api/v1/types/{slug}`      | bearer + `ManageSchema`  | Drop the type. Existing content stays but is orphaned.                         |

## Content

| Method | Path                                                      | Auth                       | Notes                                                                                 |
|--------|-----------------------------------------------------------|----------------------------|---------------------------------------------------------------------------------------|
| GET    | `/api/v1/content/{type_slug}`                             | none                       | Query params: `locale`, `status`, `page`, `per_page`, `q` (substring/JSONB ILIKE).    |
| POST   | `/api/v1/content/{type_slug}`                             | bearer + `Write(Type)`     | Body: `NewContent` (`type_id, slug, locale, data, author_id?`). Validated against type. |
| GET    | `/api/v1/content/{type_slug}/{slug}`                      | none                       | Single entry by slug.                                                                |
| PATCH  | `/api/v1/content/{type_slug}/{slug}`                      | bearer + `Write(Type)`     | Body: `ContentPatch` (`slug?, status?, data?`). Snapshots prior state to versions.    |
| DELETE | `/api/v1/content/{type_slug}/{slug}`                      | bearer + `Write(Type)`     | Hard delete. Versions remain, orphaned by content id.                                |
| POST   | `/api/v1/content/{type_slug}/{slug}/publish`              | bearer + `Publish(Type)`   | Sets status to Published. Snapshots first.                                           |
| GET    | `/api/v1/content/{type_slug}/{slug}/versions`             | none                       | Snapshot history, most-recent first.                                                 |
| POST   | `/api/v1/content/{type_slug}/{slug}/versions/{vid}/restore` | bearer + `Write(Type)`   | Reverts to the snapshot. Snapshots the live state first so restores are reversible.   |

## Users & roles

| Method | Path                       | Auth                      |
|--------|----------------------------|---------------------------|
| GET    | `/api/v1/users`            | bearer + `ManageUsers`    |
| POST   | `/api/v1/users`            | bearer + `ManageUsers`    |
| GET    | `/api/v1/users/{id}`       | bearer + `ManageUsers`    |
| PATCH  | `/api/v1/users/{id}`       | bearer + `ManageUsers`    |
| DELETE | `/api/v1/users/{id}`       | bearer + `ManageUsers`    |
| GET    | `/api/v1/roles`            | bearer + `ManageUsers`    |
| POST   | `/api/v1/roles`            | bearer + `ManageUsers`    |
| GET    | `/api/v1/roles/{id}`       | bearer + `ManageUsers`    |
| PATCH  | `/api/v1/roles/{id}`       | bearer + `ManageUsers`    |
| DELETE | `/api/v1/roles/{id}`       | bearer + `ManageUsers`    |

User responses redact `password_hash` and `totp_secret`.

## Media

| Method | Path                                  | Auth                     | Notes                                                                        |
|--------|---------------------------------------|--------------------------|------------------------------------------------------------------------------|
| GET    | `/api/v1/media`                       | bearer                   | List all media for the active site.                                          |
| POST   | `/api/v1/media`                       | bearer                   | `multipart/form-data`. Field `file` (required, ≤25 MiB), `alt` (optional).   |
| GET    | `/api/v1/media/{id}`                  | bearer                   | Metadata.                                                                    |
| GET    | `/api/v1/media/{id}/raw`              | **public**               | Streams the bytes. Public by design (anchor/img tags can't carry bearer).    |
| DELETE | `/api/v1/media/{id}`                  | bearer + `ManageUsers`   | Drops metadata + best-effort blob delete.                                    |

## Health

| Method | Path                | Notes                                                            |
|--------|---------------------|------------------------------------------------------------------|
| GET    | `/healthz`          | Liveness. Always `ok` if the process is up.                      |
| GET    | `/readyz`           | Readiness. Pings storage; returns 5xx if backend unreachable.   |

## CSRF

POST/PUT/PATCH/DELETE requests with a `Cookie: ferro_csrf=...` header must echo the same value in `X-CSRF-Token`. Bearer-authenticated requests bypass — the `Authorization` header isn't auto-attached cross-site, so it's CSRF-immune by construction. Browsers using cookie-based sessions should `GET /api/v1/auth/csrf` to mint a token, then mirror it in headers.

## Curl examples

Login + use the token:

```sh
TOKEN=$(curl -s -XPOST http://localhost:8080/api/v1/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"email":"you@example.com","password":"hunter2"}' | jq -r .token)

curl -H "Authorization: Bearer $TOKEN" http://localhost:8080/api/v1/auth/me
```

Create a content entry:

```sh
curl -XPOST http://localhost:8080/api/v1/content/post \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{
    "type_id": "01HK...post-type-id",
    "slug": "hello-world",
    "locale": "en",
    "data": { "title": "Hello", "body": "# Markdown body" }
  }'
```

Upload media:

```sh
curl -XPOST http://localhost:8080/api/v1/media \
  -H "Authorization: Bearer $TOKEN" \
  -F file=@./cover.jpg \
  -F alt="cover image"
```

List versions of an entry:

```sh
curl http://localhost:8080/api/v1/content/post/hello-world/versions | jq
```
