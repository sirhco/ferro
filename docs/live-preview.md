# Live preview

Every content edit page in the admin embeds a server-rendered preview of
the current entry — including drafts. The preview reloads automatically
when the entry changes, via the existing SSE stream.

## How it works

1. `GET /preview/:type_slug/:slug` (in `crates/ferro-api/src/preview.rs`)
   resolves the entry (drafts included) and renders it as a standalone
   HTML page using:
   * `ferro_editor::render_blocks_html` for `rich_text { format: blocks }` fields.
   * `ferro_editor::markdown::render_markdown` for `rich_text { format: markdown }` fields.
   * Type-aware fallbacks for media, booleans, numbers, and JSON.
2. The admin content-edit page renders an `<iframe id="ferro-preview-iframe" />`
   alongside the form.
3. The browser opens an `EventSource` to
   `/api/v1/events?type=<type_slug>` (the SSE endpoint at
   `crates/ferro-api/src/sse.rs`) and reloads the iframe whenever an
   incoming event mentions the current slug.

The result: save the form, ~1s later the preview pane refreshes with
the new content. No manual reload, no extra plumbing.

## Auth

The preview route requires the admin session cookie / bearer token —
unauthenticated requests get a 401. This protects unpublished drafts.

## Customizing the preview chrome

The default preview ships a minimal CSS theme (`preview-header`,
`preview-main`, `preview-field` classes) inline in the response. To
customize, fork `crates/ferro-api/src/preview.rs::wrap_document` or
proxy the route through your own handler that injects a layout.
