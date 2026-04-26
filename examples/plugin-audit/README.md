# plugin-audit

Appends a JSONL audit line for every content lifecycle event:
`content.created`, `content.updated`, `content.published`, `content.deleted`.

## Build

```sh
cargo build --manifest-path examples/plugin-audit/Cargo.toml --release --target wasm32-wasip2
```

## Install

```sh
mkdir -p examples/starter-blog/plugins/audit
cp target/wasm32-wasip2/release/plugin_audit.wasm examples/starter-blog/plugins/audit/plugin.wasm
cp examples/plugin-audit/plugin.toml examples/starter-blog/plugins/audit/
```

## Output

After a few content edits and publishes, the audit log lives at:

```
examples/starter-blog/plugins/audit/data/audit.log
```

```jsonl
{"event":"content.created","type":"page","slug":"changelog","status":"draft"}
{"event":"content.updated","type":"page","slug":"changelog","status":"draft"}
{"event":"content.published","type":"page","slug":"changelog"}
```

Tail with:

```sh
tail -f examples/starter-blog/plugins/audit/data/audit.log
```
