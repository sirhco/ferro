# Plugin walkthrough

The starter-blog ships four reference plugins demonstrating Ferro's WASM
plugin host. Each lives under `examples/`.

| Plugin                 | What it shows                                       |
|------------------------|-----------------------------------------------------|
| `plugin-hello`         | Minimal observer — logs `content.published`.        |
| `plugin-seo`           | Sandboxed file I/O — emits OG/JSON-LD sidecars.     |
| `plugin-audit`         | Multi-hook subscription — JSONL audit trail.        |
| `plugin-panic`         | Fault isolation — intentional panic, host stays up. |
| `plugin-webhook-demo`  | Built-in `WebhookHook` — config-only, no WASM.      |

## Build all WASM plugins

```sh
for p in plugin-seo plugin-audit plugin-panic; do
  cargo build --manifest-path examples/$p/Cargo.toml --release --target wasm32-wasip2
  mkdir -p examples/starter-blog/plugins/${p#plugin-}
  cp target/wasm32-wasip2/release/${p//-/_}.wasm examples/starter-blog/plugins/${p#plugin-}/plugin.wasm
  cp examples/$p/plugin.toml examples/starter-blog/plugins/${p#plugin-}/
done
```

Then start the server (`cargo run -p ferro-cli -- serve`) and open
`http://localhost:8080/admin/plugins` to enable them.

## Sandbox model

Each plugin gets a per-instance WASI sandbox with one preopened
directory: `<plugin_dir>/data` mounted as `/data`. Anything outside that
path is unreachable. Capabilities (`logs`, `content.read`, etc.) gate
host-imported functions independently — even with `/data` mounted, a
plugin without `logs` cannot call `log()`.

## Hot-swap

The plugin registry supports live reload via
`PluginRegistry::reload()` and per-plugin enable/disable. The admin UI
exposes both. In-flight invocations hold an `Arc<PluginHandle>` and
complete on the old store before being dropped — no torn writes.

## Fault isolation

`call_on_event` catches every panic and trap inside the WASM module and
returns it as a logged error rather than propagating to the user
request. `plugin-panic` exists specifically to demonstrate this.
