# starter-blog

A minimal but complete Ferro example. Drop into the workspace and you have:

- A pre-seeded **default site** ("Starter Blog").
- Five **content types**: `Post`, `Author`, `Page`, `Product`, `Event`. Cover the full range of field kinds — text, slug, rich-text markdown, native block editor, json tags, reference, media (single + multiple), enum, number, boolean, date-time.
- Eleven **seed entries**: 1 author, 3 posts, 2 pages (`about`, `contact`), 3 products (cloud pricing tiers), 2 events.
- A **demo admin user** (`me@example.com` / `correct-horse-battery-staple`) wired to the `admin` role.
- Code-first **Rust definitions** of all five content types in `src/lib.rs` via `#[derive(ContentType)]`, for teams that prefer schema-as-code.
- A **zero-JS public site** (`site-server/`) that server-renders Posts/Pages/Products/Events from the same fs-json store the admin writes to.
- Four **reference plugins** under `examples/`: `plugin-seo`, `plugin-audit`, `plugin-webhook-demo`, `plugin-panic` — see [`docs/plugin-walkthrough.md`](../../docs/plugin-walkthrough.md).

## Run

From the workspace root:

```sh
# Build the server + admin SPA once
cargo build -p ferro-cli
cargo leptos build --project ferro-admin

# Run from the example dir (cwd matters — `[storage].path = "./data"` is relative)
cd examples/starter-blog
FERRO_JWT_SECRET=$(openssl rand -hex 32) \
  ../../target/debug/ferro --config ./ferro.toml serve --site-dir ../../target/site
```

Boot log:

```
WARN  ferro_cli::config: auth.jwt_secret not set and FERRO_JWT_SECRET not in env; ...   (only if env unset)
INFO  ferro_cli::serve: ferro listening on http://127.0.0.1:8080 (admin SPA hydrates from /…/target/site/pkg)
```

`seeded default site` won't show because this example ships with a site already in `data/sites/`. If you see it, you're running from the wrong cwd — fix and the site, types, and posts below will appear.

## Sign in

Open <http://127.0.0.1:8080/admin>:

- **Email**: `me@example.com`
- **Password**: `correct-horse-battery-staple`

⚠️ Rotate this password the moment you start using the example as anything more than a demo. The hash in `data/users/*.json` is committed — anyone with the repo can sign in.

## Take the tour

### Dashboard
Welcome card + nav. Two content types should appear in the count.

### Content
Switch the type picker between **Blog post** and **Author**.

- 3 posts: `hello-ferro`, `isomorphic-rust` (both published), `shipping-checklist` (draft).
- 1 author: `jane-doe`.

Click **Edit** on a post — JSON `data` payload is editable. The Versions card on existing entries lists prior snapshots; restore reverts.

### Schema
**Blog post** → **Edit**. Inspect the `fields` JSON: text/slug/rich-text/json/reference/media field kinds, all in one type. Add a new field via the quick-add row at the bottom, save — the migrator runs across the 3 existing posts and surfaces a `rows_migrated` count in the toast.

### Media
Empty by default. Upload an image; the tile renders inline. Switch the cover_image_id of a post to point at the new media id, save, observe the post now references it.

### Settings
Enroll TOTP for an end-to-end test of the MFA flow. Disabling requires a current code (proves you still have the device).

## What's in the data dir

```
data/
  sites/01HQYHA000000000000000000Z.json     # Starter Blog
  types/01HQYTP000000000000000000Z.json     # Post
  types/01HQYTA000000000000000000Z.json     # Author
  content/01HQYAJ000000000000000000Z.json   # author: jane-doe
  content/01HQYP1000000000000000000Z.json   # post: hello-ferro
  content/01HQYP2000000000000000000Z.json   # post: isomorphic-rust
  content/01HQYP3000000000000000000Z.json   # post: shipping-checklist (draft)
  users/user_01KQ1K9KR850DY0BBNQVQ2R3CD.json
  roles/role_01KQ1K9KR8JBGSQDV3RJA08C21.json
  media/                                    # empty (uploads land here)
  versions/                                 # empty (snapshots accrue on edit)
```

The `fs-json` storage backend reads these on boot. Edit by hand to bootstrap fixtures without writing a migration.

## Code-first schemas

`src/lib.rs` declares the same Post + Author types via `#[derive(ContentType)]`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, ContentType)]
#[ferro(slug = "post", name = "Blog post")]
pub struct Post {
    pub title: String,
    pub slug: String,
    pub excerpt: Option<String>,
    pub body: String,
    pub cover_image_id: Option<String>,
    pub tags: Vec<String>,
    pub published_at: Option<time::OffsetDateTime>,
    pub author_id: String,
}
```

The macro generates a `ContentType` constructor + `FieldDef` list. Wire into a build step that synchronizes the generated schema into the repo (TODO; today the JSON files are the source of truth).

## Public site (zero-JS Leptos SSR)

A separate binary serves the public-facing site. It calls the admin API
for content and renders pure server-side HTML — no client-side
JavaScript framework, no hydration. This demonstrates the "sub-500ms
TTI" pitch end-to-end.

```sh
# In a second terminal, with ferro-cli already serving on :8080
cd examples/starter-blog
cargo run -p starter-site-server
# → starter-site listening on http://127.0.0.1:3001
```

Routes:

- `/` — home (latest posts + product grid)
- `/blog`, `/blog/:slug` — Post index + detail
- `/products`, `/products/:slug` — Product catalog
- `/events`, `/events/:slug` — Event listing
- `/:slug` — catch-all Page (e.g. `/about`, `/contact`)

`view-source:` any page and confirm there is no framework runtime in the
body — just rendered HTML and one inline stylesheet.

## Live preview in admin

The admin's content-edit page renders a live `<iframe>` of the entry
(including drafts) via `/preview/:type/:slug`. Saves trigger an SSE
event that auto-reloads the iframe — no manual refresh needed. See
[`docs/live-preview.md`](../../docs/live-preview.md).

## What to try next

- **Wire a webhook**: add `[[webhooks]] url = "https://hooks.example.com/ferro" events = ["content.published"]` to `ferro.toml`, restart, publish the draft post, watch the receiver.
- **Switch backend**: `ferro export --out bundle.json --include-media`, change `[storage] kind = "surreal-embedded"`, `ferro init --storage surreal`, `ferro import bundle.json`.
- **Add a real role**: `ferro admin create-role --name "writer" --preset editor` then attach to a new user with `ferro admin grant-role`.
- **Try the GraphQL playground** at <http://127.0.0.1:8080/graphiql>. Query `{ contentBySlug(typeSlug:"post", slug:"hello-ferro") { id slug data } }` with your bearer token in the Headers panel.

## Files you can safely delete

- `data/versions/` — accrues automatically; clear to reset history.
- `data/media/` — uploads land here under fs-json; clear to drop all assets.
- `media-store/` — local media backend root.
- `plugins/` — unused until the WASM plugin host loader lands.

Don't delete `data/sites/`, `data/types/`, or `data/users/` unless you're starting over — without them the admin UI 404s on its boot fetches.
