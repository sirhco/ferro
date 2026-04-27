# Media

Three backends behind one `MediaStore` trait: `local` (filesystem), `s3` (AWS / R2 / MinIO / any S3-API), `gcs` (Google Cloud Storage). Selected via `[media] kind` in `ferro.toml`.

## API surface

REST endpoints (see [`rest-api.md`](rest-api.md) for full table):

- `POST /api/v1/media` — multipart upload, ≤25 MiB, optional `alt` text.
- `GET /api/v1/media` — list (auth-gated).
- `GET /api/v1/media/{id}` — metadata.
- `GET /api/v1/media/{id}/raw` — **public**. Streams bytes. Required public so `<img src="...">` and anchor `href` work without bearer headers.
- `DELETE /api/v1/media/{id}` — drops metadata + best-effort blob delete (`ManageUsers`).

## Upload pipeline

1. Multipart parsed; `file` field required, `alt` optional.
2. Size check (25 MiB). Larger ⇒ `400 bad_request`.
3. MIME from the multipart Content-Type, fallback `mime_guess` from filename.
4. `key = "<media-id>/<sanitized-filename>"` — collision-proof + human-readable.
5. Backend `put(key, body, mime, size)` streams into storage.
6. `MediaMetaRepo::create` inserts the metadata row.
7. Response is the full `Media` struct.

`sanitize_filename` strips anything outside `[A-Za-z0-9.-_]` to `_`, then collapses dot-only / underscore-only names to `upload`. Defends against directory traversal in the storage key.

## Backends

### `local`

```toml
[media]
kind = "local"
path = "./media-store"
base_url = "http://localhost:8080/media"   # optional; pre-computed Media.url
```

Files live under `path/<key>`. No public-facing URL by default — clients hit `/api/v1/media/{id}/raw` and Ferro streams the file. If you want CDN-friendly direct URLs, set `base_url` and front the files with a static server (nginx, Caddy `file_server`).

### `s3`

```toml
[media]
kind = "s3"
bucket = "ferro-media"
region = "us-east-1"
prefix = "prod/"                         # optional, prepended to every key
endpoint = "https://s3.amazonaws.com"    # optional override (R2: account-id.r2.cloudflarestorage.com; MinIO: your endpoint)
public_base_url = "https://cdn.example.com/prod/"  # optional; if set, Media.url is precomputed
```

Credentials follow the standard AWS chain (`AWS_ACCESS_KEY_ID`/`AWS_SECRET_ACCESS_KEY` env, `~/.aws/credentials`, IAM role on EC2/ECS/Lambda). `public_base_url` is what gets baked into `Media.url` when present — so you can serve directly from CloudFront/R2 without round-tripping the Ferro server.

### `gcs`

```toml
[media]
kind = "gcs"
bucket = "ferro-media"
prefix = "prod/"
service_account_path = "/etc/ferro/sa.json"
public_base_url = "https://storage.googleapis.com/ferro-media/prod/"
```

Service account key file is required today; ADC (`GOOGLE_APPLICATION_CREDENTIALS` env) support is planned.

## URL strategy

`Media.url` is precomputed at upload time when the backend has a `public_base_url`. The admin UI prefers `Media.url` if present, else falls back to `/api/v1/media/{id}/raw`.

For production with S3/GCS:
- Set `public_base_url` so admin tiles and front-end consumers hit the CDN, not your origin.
- Set bucket ACL to public-read for the `public_base_url` to work, or stick a CDN with origin auth in front.

## Image pipeline

`crates/ferro-media` gates `images` feature for image-specific helpers (probe dimensions, generate thumbnails). Disabled by default; enable in production if you want `Media.width`/`Media.height` populated automatically.

## Public exposure

`/api/v1/media/{id}/raw` is intentionally public. Reasoning: anchor and image tags don't carry custom headers, and storing a one-off signed URL per request adds latency. Treat the media bucket as world-readable.

If you have sensitive uploads (PDFs with PII, draft-only assets), put them in a separate bucket behind signed URLs and skip the Ferro media surface for those.

## Storage size + cleanup

Hard delete via `DELETE /api/v1/media/{id}` removes the metadata row and best-effort the underlying blob. Storage costs scale with retained metadata + blobs; the only cleanup today is operator-driven.

Future: garbage-collect media not referenced by any content row. Easy to add on top of `MediaRepo` + a content scan.

## Migration

Switching backends? Use the export/import bundle:

```sh
# old:
ferro export --out bundle.json --include-media
# edit ferro.toml — switch [media] kind, set new bucket/path
ferro import bundle.json --mode replace
```

`--include-media` base64-embeds every blob into the bundle, so the import re-uploads to the new backend. Skip the flag if you've manually mirrored the bucket — the metadata still imports and points at the new backend's URLs.
