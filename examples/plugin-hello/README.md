# plugin-hello

Minimal Ferro WASM plugin. Subscribes to `content.published` and logs the slug.

## Build

Requires the `wasm32-wasip2` Rust target (component model):

```sh
rustup target add wasm32-wasip2
cargo build \
  --manifest-path examples/plugin-hello/Cargo.toml \
  --release \
  --target wasm32-wasip2
```

The built component lands at:

```
examples/plugin-hello/target/wasm32-wasip2/release/plugin_hello.wasm
```

## Install

Drop the manifest + component into your Ferro plugins directory (default
`./plugins`), then grant the `logs` capability in `ferro.toml`:

```sh
mkdir -p plugins/hello
cp examples/plugin-hello/plugin.toml plugins/hello/
cp target/wasm32-wasip2/release/plugin_hello.wasm plugins/hello/plugin.wasm
```

```toml
# ferro.toml
[[plugins.grants]]
name = "hello"
capabilities = ["logs"]
```

Restart `ferro serve` (or `POST /api/v1/plugins/hello/reload`). Publish any
content row and the server log will show:

```
INFO ferro::plugin: plugin-hello: published <slug>
```
