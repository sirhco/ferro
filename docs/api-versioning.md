# API versioning & deprecation policy

Ferro ships two co-equal API surfaces — REST and GraphQL — backed by one schema registry and one `Repository`. They version together, deprecate together, and follow one timeline.

This page is the contract for downstream clients.

## Surfaces under this policy

- **REST**: every path under `/api/v1/*` plus the OpenAPI document at `/api/openapi.json`.
- **GraphQL**: the schema served at `/graphql`, including subscriptions over `/graphql/ws`.
- **SSE**: the event stream at `/api/v1/events`.

The admin UI's internal endpoints and any path prefixed `/_internal/*` are **not** under this policy and may change at any time.

## Versioning model

- **Stable major version is in the path** (`/api/v1`) and in the GraphQL schema (`type Query` shape). Major versions are introduced at most once per year; v1 stays on `/api/v1` indefinitely once another major is added (we don't auto-retire).
- **Additive changes ship continuously.** New fields on records, new endpoints, new GraphQL types, new event variants — all land in the current major without warning. Clients must tolerate unknown fields.
- **Breaking changes only on major bumps.** Removing a field, renaming a path, changing a response shape, narrowing a value range, tightening required-fields — none of these happen inside a major.

## Deprecation timeline

Once we decide to remove or change something incompatibly, the timeline is:

1. **Announce** — release `N`. The thing is marked deprecated:
   - REST: `Deprecation` HTTP header on every response, with a `Sunset` header carrying the cut-off date (RFC 8594). OpenAPI marks the operation/schema with `deprecated: true`.
   - GraphQL: the field/argument/enum value is annotated `@deprecated(reason: "...")`. Schema introspection surfaces it.
   - `CHANGELOG.md` "Deprecated" section lists it.
2. **Soft-removed** — release `N+1` (≥ 90 days later). The thing still works but logs a warning; tracing emits a `deprecated_api_call` span attribute.
3. **Removed** — next major (release `N+M`). The endpoint returns `410 Gone` (REST) or the field is gone from the schema (GraphQL).

Total minimum window from announce → removal: **180 days**, and removal only at a major boundary. Security-driven removals (CVE-class) skip step 2 and may compress the timeline; we'll publish the rationale in the security advisory.

## Authentication and headers

- Auth headers (`Authorization: Bearer ...`, `X-CSRF-Token`, refresh-token cookie shape) are part of the API contract. Same deprecation rules apply.
- The CSRF cookie name and value format are stable inside a major.

## Webhook payloads

The `[[webhooks]]` outbound JSON body is part of the contract:

- HMAC signing (`X-Ferro-Signature` header, SHA-256 hex over the raw body) does not change inside a major.
- New event types are additive; receivers should ignore unknown `kind` values.

## Plugin host (separate policy)

The WASM plugin ABI follows its own versioning policy — see `plugins-webhooks.md` ("WIT package versioning policy"). The two policies are independent: a major REST/GraphQL bump does not necessarily require a plugin ABI bump, and vice versa.

## Pre-1.0 caveat

Before 1.0 (`v0.x`), this policy is best-effort. Some ADRs were rewritten and some endpoint shapes shifted between 0.4 and 1.0. From 1.0 onward, the timeline above is binding.
