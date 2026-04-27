# plugin-webhook-demo

Configuration-only demo: no WASM, just a built-in `WebhookHook` entry in
`ferro.toml`. Sends an HMAC-signed POST to a target URL on every
content lifecycle event.

## Setup

1. Pick a webhook receiver URL — e.g. https://webhook.site/ gives you a
   throwaway endpoint with a live request log.
2. Append this section to `examples/starter-blog/ferro.toml`:

   ```toml
   [[webhooks]]
   url = "https://webhook.site/your-uuid-here"
   secret = "change-me-to-a-strong-secret"
   events = [
     "content.created",
     "content.updated",
     "content.published",
     "content.deleted",
   ]
   ```
3. Restart the server.

## What you'll see

Every matching event triggers a POST with these headers:

```
content-type: application/json
x-ferro-signature: sha256=<hex hmac of body>
x-ferro-event: content.published
```

The body is the same `HookEvent` JSON the SSE stream emits. The signature
is `HMAC-SHA256(secret, body)` — verify on the receiver before trusting.

## Why this matters

WordPress plugins send webhooks via PHP code that runs in the request
path; failures degrade the user-facing response. Ferro's webhook hook
fires asynchronously after the commit returns, so a slow or failing
receiver never delays the editor.
