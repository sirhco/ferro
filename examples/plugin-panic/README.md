# plugin-panic

Demonstrates Ferro's plugin fault isolation. On every `content.created`
event this plugin panics; the host catches the trap, logs it, and keeps
the user request flowing without ever crashing the main process.

## Build

```sh
cargo build --manifest-path examples/plugin-panic/Cargo.toml --release --target wasm32-wasip2
```

## Install

```sh
mkdir -p examples/starter-blog/plugins/panic
cp target/wasm32-wasip2/release/plugin_panic.wasm examples/starter-blog/plugins/panic/plugin.wasm
cp examples/plugin-panic/plugin.toml examples/starter-blog/plugins/panic/
```

## How to demo

1. Start the server: `cargo run -p ferro-cli -- serve` (from `examples/starter-blog/`).
2. Open `http://localhost:8080/admin/plugins` and confirm `panic` is loaded
   and enabled.
3. Create any new content entry. The save succeeds; the server logs:
   ```
   plugin panic="intentional fault for hot-swap demo"
   ```
4. Click **Disable** on the panic plugin in the admin. The host hot-swaps
   without a restart. Subsequent content creates run cleanly.

This shows what's broken about the WordPress plugin model: a single
errant plugin in PHP would white-screen the site. In Ferro, the blast
radius stops at the WASM module boundary.
