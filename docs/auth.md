# Authentication & authorization

## Passwords

Argon2id, default cost (`m=19MiB, t=2, p=1`). Salted per-record. Verification via `argon2`'s constant-time helper. Hashes never leave the server — `User::redacted()` zeros them on the wire.

## Access tokens

HS256 JWT. 12-hour TTL. Claims:

```json
{
  "sub": "<user-id-ulid>",
  "iss": "ferro",
  "iat": 1714080000,
  "exp": 1714123200,
  "roles": ["editor"]
}
```

Stateless. Verified per request in `AuthUser::try_from_headers`:

1. Strip `Bearer ` prefix.
2. `JwtManager::verify` — checks signature, issuer, expiry.
3. Hydrate user via `repo.users().get(claims.user_id())`.
4. Reject if `!user.active`.
5. Reject if `claims.iat < user.password_changed_at` — gives logout-all-sessions for free.
6. Resolve role records into an `AuthContext` for policy checks.

## Refresh tokens

Opaque 64-character hex (32 bytes of `rand::thread_rng()`). 30-day TTL. Stored in the `SessionStore` (memory by default; backend-specific impls swap in for prod). Tied to a user id and issued/expires timestamps.

Rotation is one-shot: `POST /auth/refresh` revokes the presented token before minting the new pair. A leaked refresh token can be redeemed exactly once before the legitimate user notices the next refresh failing — that's the theft-detection signal.

The admin SPA wraps this in a single-flight latch: concurrent 401s from parallel API calls coalesce into one rotation, and each original request retries once with the new token.

## Logout

`POST /auth/logout` with `{refresh_token}` revokes the token server-side. The access JWT can't be revoked statelessly — it expires on its own (≤12h). For immediate cutover, change the password (sets `password_changed_at`, invalidates all earlier-issued JWTs).

## TOTP (RFC 6238)

Two-factor based on HMAC-SHA1 with 30-second time windows.

### Enrollment

1. `POST /auth/totp/setup` — server mints a fresh 160-bit Base32 secret + builds an `otpauth://totp/Ferro:user@example.com?secret=...&issuer=Ferro` URI. Response is **not persisted** — caller must commit.
2. User scans the URI as a QR code (or pastes the secret) into an authenticator app.
3. `POST /auth/totp/enable` with `{secret, code}`. Server verifies the 6-digit code (default ±1 window for clock skew), persists the secret on the user.

### Login flow

1. `POST /auth/login` — when `user.totp_secret.is_some()`, response is `{ mfa_required: true, mfa_token: "mfa:..." }` instead of session tokens. The challenge is stored under the `mfa:` prefix in the session store with a 5-minute TTL.
2. `POST /auth/totp/login` with `{mfa_token, code}` — challenge is one-shot (revoked regardless of code outcome), code is verified, real session pair is minted.

### Disable

`POST /auth/totp/disable` with `{code}` — proves the device is still in possession, then clears `user.totp_secret`.

### Implementation notes

- Verification accepts `±1` 30-second window. Clients with significant clock drift will fail.
- Secrets are Base32 (RFC 4648), no padding. `ferro_auth::totp::generate_secret` mints them.
- Reference vector test in `crates/ferro-auth/src/totp.rs` validates RFC 6238 counter 59 → `287082`.

## Rate limiting

Per-IP token bucket. Configured at `RateLimitConfig { burst, refill_per_sec }`. Default burst 10 over 60 seconds. Identified via `X-Real-IP` → first `X-Forwarded-For` entry → fallback to `0.0.0.0` (test bucket).

Active on:
- `POST /auth/login`
- `POST /auth/signup`
- `POST /auth/refresh`
- `POST /auth/totp/login`

Excess requests get `429 Too Many Requests` with a JSON `{retry_after_ms}` hint.

## CSRF

Double-submit token. The middleware (`ferro_api::csrf::enforce`) bypasses on:
- Safe methods (GET/HEAD/OPTIONS).
- Bearer-authenticated requests (the `Authorization` header isn't auto-attached cross-site).
- Cookie-less requests (no session ⇒ no target).

Otherwise enforces `X-CSRF-Token` header == `ferro_csrf` cookie (constant-time compare).

Mint a token via `GET /api/v1/auth/csrf` — sets the cookie (`SameSite=Strict; Path=/`), returns `{token: "<hex>"}` for the SPA to mirror in the header.

The Leptos admin uses Bearer in `localStorage` and never relies on cookies, so CSRF is a defense-in-depth measure for any cookie-session client added later (form-post integrations, third-party tools).

## RBAC

Three primitives in `ferro_core::permission`:

- **Action** — `Read`, `Write`, `Publish`, `ManageUsers`, `ManageSchema`, `Admin` (wildcard).
- **Scope** — `Site { id }`, `Type { id }`, `Global`.
- **Permission** — `(Action, Scope)`.

Roles are named bundles of permissions. Users have many roles. `authorize(ctx, Permission::Write(Scope::Type { id }))` walks the user's roles and returns success if any covers the requested permission. The `Admin` action short-circuits all checks, which is how the seeded `admin` role grants everything.

Common patterns:

| Role          | Permissions                                                              |
|---------------|--------------------------------------------------------------------------|
| `viewer`      | `Read(Site)`                                                             |
| `editor`      | `Read(Site)` + `Write(Type::posts)`                                      |
| `publisher`   | `editor` + `Publish(Type::posts)`                                        |
| `schema-admin`| `ManageSchema`                                                           |
| `user-admin`  | `ManageUsers`                                                            |
| `admin`       | `Admin` (everything)                                                     |

Create custom roles via `ferro admin create-role` or `POST /api/v1/roles` (with `ManageUsers`).

## Session storage backends

- `MemorySessionStore` (default). Lost on restart. Fine for dev.
- `PostgresSessionStore` (when the postgres storage backend is used; same connection pool).
- `RedisSessionStore` is a future option — the trait is small (4 methods).

## Security checklist for production

- Set `FERRO_JWT_SECRET` to ≥32 bytes of entropy. Never commit.
- Run behind TLS-terminating reverse proxy. Pass `X-Real-IP` / `X-Forwarded-For` so rate limiting works per real client.
- Lock `[auth] allow_public_signup = false` unless you specifically want open enrollment.
- Encourage TOTP for admin users; the UI flow is two clicks.
- Rotate the JWT secret on suspected compromise (invalidates all access tokens immediately; refresh tokens still work since they're stateful).
- Turn on Postgres for `SessionStore` if you need cross-restart session survival.

## Secrets and the SSR/CSR boundary

Leptos compiles the same Rust to both server (SSR) and browser (WASM hydrate). Anything reachable from a `#[component]` reachable on the client gets shipped to every visitor. Secrets must never cross that line.

Hard rules — enforced in code review:

- **No `FERRO_JWT_SECRET` access outside `ferro-cli`/`ferro-api`/`ferro-auth`.** These crates compile only for the host target. Don't add `std::env::var("FERRO_JWT_SECRET")` to `ferro-admin`, `ferro-editor`, `ferro-core`, or any crate that compiles to wasm.
- **No DB credentials, S3 keys, webhook signing secrets, or GCS service-account paths in `ferro-admin` or `ferro-editor`.** The admin UI talks to its own API (`/api/v1/*`) — credentials stay server-side.
- **Server functions (`#[server]`) are server-only by construction**; their bodies are stripped from the WASM bundle. Use them when admin-only mutations need to read secrets — but verify the function isn't accidentally re-exported through a `pub use` chain to a client-reachable module.
- **`#[island]` components hydrate in the browser.** Treat their entire transitive module tree as public.

When in doubt: if a module is referenced (directly or via `pub use`) from `ferro-admin/src/lib.rs` or `ferro-editor/src/lib.rs`, assume the browser gets the source.
