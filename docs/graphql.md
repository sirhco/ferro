# GraphQL & SSE

Endpoint: `POST /graphql` (queries + mutations) and `GET /graphql/ws` (subscriptions over WebSocket). GraphiQL playground at `/graphiql` for exploration.

The schema is built with [`async-graphql`](https://docs.rs/async-graphql) and wired by `ferro_api::graphql::router`.

## Authentication

Same as REST: `Authorization: Bearer <jwt>`. The schema reads the bearer JWT and resolves an `AuthContext` once per request, then makes it available to resolvers via `Context::data::<AuthContext>()`.

For subscriptions over WS, the bearer is sent in the `connection_init` payload:

```json
{
  "type": "connection_init",
  "payload": { "Authorization": "Bearer <jwt>" }
}
```

## Schema overview

```graphql
type Query {
  sites: [Site!]!
  contentTypes: [ContentType!]!
  contentType(slug: String!): ContentType
  content(typeSlug: String!, query: ContentQueryInput): ContentPage!
  contentBySlug(typeSlug: String!, slug: String!): Content
  versions(contentId: ID!): [ContentVersion!]!
}

type Mutation {
  createContent(typeSlug: String!, input: NewContentInput!): Content!
  updateContent(typeSlug: String!, slug: String!, patch: ContentPatchInput!): Content!
  publishContent(typeSlug: String!, slug: String!): Content!
  deleteContent(typeSlug: String!, slug: String!): Boolean!
  upsertType(input: ContentTypeInput!): TypeUpdateResult!
}

type Subscription {
  contentEvents(typeSlug: String): ContentEvent!
}
```

`ContentEvent` is a union of `ContentCreated | ContentUpdated | ContentPublished | ContentDeleted`, with a `typeSlug` filter narrowing the stream.

## Subscriptions

Bridged from the same `HookRegistry` that drives webhooks. Per-event RBAC: a subscriber sees an event only if the resolved `AuthContext` has `Read(Site)` (or `Admin`) for the affected scope. No silent leakage of restricted content types.

Transport: `graphql-transport-ws` (the modern protocol, used by `graphql-ws@5+`). The legacy `subscriptions-transport-ws` is not supported.

## Server-sent events (SSE)

Alternative to WebSocket for environments that can't keep open WS (browser inactivity, some proxies):

```
GET /api/v1/events?type=post
Accept: text/event-stream
Authorization: Bearer <jwt>
```

Stream of `event: <name>\ndata: <json>\n\n` lines. Same RBAC as the GraphQL subscription. Useful for dashboards and webhook receivers that prefer pull-style HTTP.

## GraphiQL

`/graphiql` serves a hosted GraphiQL playground. Set the `Authorization` header in the upper-right "Headers" pane to authenticate. Subscriptions work over WS to `/graphql/ws`.

In production, lock down GraphiQL behind a flag (TODO: `[server] graphiql_enabled = false`) — for now, gate it via reverse-proxy auth or remove the route at compile time if you don't want it exposed.

## Why both GraphQL and REST?

REST is the lingua franca for plugins, webhooks, and one-off curl. GraphQL is the productive choice when a UI needs a custom shape per-screen, or when a downstream consumer wants to walk relations. Both speak to the same `Repository` traits — no second source of truth.

See [ADR-0004](adr/0004-graphql-and-rest.md) for the rationale.

## Curl examples

Query:

```sh
curl -X POST http://localhost:8080/graphql \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "query": "{ contentBySlug(typeSlug:\"post\", slug:\"hello\") { id slug status data } }"
  }' | jq
```

Mutation:

```sh
curl -X POST http://localhost:8080/graphql \
  -H 'Content-Type: application/json' \
  -H "Authorization: Bearer $TOKEN" \
  -d '{
    "query": "mutation($s: String!, $p: ContentPatchInput!) { updateContent(typeSlug:\"post\", slug:$s, patch:$p) { id slug status } }",
    "variables": { "s": "hello", "p": { "data": { "title": "Updated" } } }
  }' | jq
```

Subscribe (using `wscat` + the `graphql-transport-ws` protocol):

```sh
wscat -c ws://localhost:8080/graphql/ws -s graphql-transport-ws
> {"type":"connection_init","payload":{"Authorization":"Bearer ..."}}
> {"id":"1","type":"subscribe","payload":{"query":"subscription { contentEvents(typeSlug:\"post\") { ... on ContentPublished { content { slug } } } }"}}
```
