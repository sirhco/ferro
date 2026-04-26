# Block editor

Ferro ships a pure-Rust Leptos block editor for any field declared with
`{"type": "rich_text", "format": "blocks"}`. There is no JavaScript
WYSIWYG dependency — every block-level UI control compiles down to the
same Leptos signal graph as the rest of the admin.

## Block types

| Block       | JSON shape                                                |
|-------------|-----------------------------------------------------------|
| Paragraph   | `{ "kind": "paragraph", "text": "..." }`                  |
| Heading     | `{ "kind": "heading", "level": 1\|2\|3, "text": "..." }`  |
| Quote       | `{ "kind": "quote", "text": "...", "cite": "..." }`       |
| Code        | `{ "kind": "code", "lang": "rust", "code": "..." }`       |
| Image       | `{ "kind": "image", "media_id": "...", "alt": "..." }`    |
| List        | `{ "kind": "list", "ordered": true, "items": ["..."] }`   |
| Divider     | `{ "kind": "divider" }`                                   |

A document is a JSON array of these objects, persisted in the field's
`FieldValue::Object`. Validation is enforced by `ferro_core::FieldValue::validate_against`
for the `RichFormat::Blocks` arm.

## Adding the field to a content type

```json
{
  "id": "01HQYFQ400000000000000000Z",
  "slug": "blocks",
  "name": "Body",
  "kind": { "type": "rich_text", "format": "blocks" },
  "required": false
}
```

Re-load the schema (or hot-restart) and the admin's content-edit form
auto-renders the `BlockEditor` for that field.

## Server-side rendering

`ferro_editor::render_blocks_html(&Document, media_base_url)` returns
escaped HTML suitable for SSR. The starter-blog public site uses it for
`Page`, `Product`, and `Event` body fields; the admin's `/preview` route
uses the same function so what you see in the preview iframe matches
what visitors see.

## Editor UI

* Per-block ↑ / ↓ / × controls.
* Footer **+ Add block** opens a chooser with all block kinds.
* Heading rows include a level selector (H1–H3).
* List rows include an "Ordered" toggle and one item per line.

The component is available as `ferro_editor::BlockEditor`; the
`FieldEditor` dispatcher routes `RichFormat::Blocks` to it automatically.
