# plugin-seo

Generates Open Graph meta + JSON-LD structured data for every published
content entry, written as a sidecar JSON file at
`<plugin_dir>/data/<type_slug>/<slug>.json`.

## Build

```sh
cargo build --manifest-path examples/plugin-seo/Cargo.toml --release --target wasm32-wasip2
```

## Install

```sh
mkdir -p examples/starter-blog/plugins/seo
cp target/wasm32-wasip2/release/plugin_seo.wasm examples/starter-blog/plugins/seo/plugin.wasm
cp examples/plugin-seo/plugin.toml examples/starter-blog/plugins/seo/
```

## Capabilities

| Capability     | Why                                            |
|----------------|------------------------------------------------|
| `logs`         | structured logging                             |
| `content.read` | (declared; future use for cross-content lookup)|

## Output

After publishing the seeded About page, you should see:

```
examples/starter-blog/plugins/seo/data/page/about.json
```

The file contains an `open_graph` map and a `json_ld` object scoped by
`type_slug` (`Article` for posts, `Product` for products, `Event` for events,
`WebPage` for pages).
