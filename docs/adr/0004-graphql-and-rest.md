# ADR-0004: Ship GraphQL and REST together

**Status:** Accepted
**Date:** 2026-04-24

## Context

Headless CMS clients have split preferences: GraphQL for flexible consumer queries, REST for simple integrations, webhooks, and CDN-cached content delivery.

## Decision

First-class support for **both GraphQL and REST**, generated from a shared in-memory schema registry seeded by registered `ContentType`s.

## Rationale

- GraphQL (`async-graphql`): single-request field selection, subscriptions for live preview, typed schema introspection.
- REST: simple cache semantics (ETag, `Cache-Control`), easy to hit from shell/webhooks, OpenAPI (`utoipa`) for client gen.
- Both read from the same `Repository` — no duplicated business logic.

## Alternatives Considered

- **GraphQL only**: Elegant but punishes cache layers and simple integrations.
- **REST only**: Forces clients into overfetching or bespoke endpoints per view.

## Consequences

- Two surfaces to version. We document a single deprecation policy covering both.
- Subscription support is GraphQL-only in v1; REST can consume SSE for events in a later release.
