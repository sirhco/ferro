#!/usr/bin/env bash
# Build and install the four reference plugins into starter-blog/plugins/.
# Run from anywhere; resolves paths relative to its own location.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
PLUGINS_DIR="$SCRIPT_DIR/plugins"
TARGET=wasm32-wasip2

if ! rustup target list --installed | grep -q "^${TARGET}$"; then
    echo "Installing rust target ${TARGET}..."
    rustup target add "${TARGET}"
fi

build_and_install() {
    local crate="$1"        # e.g. plugin-seo
    local install_name="$2" # e.g. seo
    local wasm_name="$3"    # e.g. plugin_seo

    echo
    echo "==> $crate"
    cargo build \
        --manifest-path "$REPO_ROOT/examples/$crate/Cargo.toml" \
        --release \
        --target "$TARGET"

    # Each plugin crate declares its own [workspace], so its build artifacts
    # land in <crate>/target/, not the repo root target/.
    local out_dir="$PLUGINS_DIR/$install_name"
    mkdir -p "$out_dir"
    cp "$REPO_ROOT/examples/$crate/target/$TARGET/release/${wasm_name}.wasm" "$out_dir/plugin.wasm"
    cp "$REPO_ROOT/examples/$crate/plugin.toml" "$out_dir/plugin.toml"
    echo "    installed → $out_dir/"
}

build_and_install plugin-hello hello plugin_hello
build_and_install plugin-seo   seo   plugin_seo
build_and_install plugin-audit audit plugin_audit
build_and_install plugin-panic panic plugin_panic

echo
echo "All plugins installed. Restart 'ferro serve' to pick them up."
echo "Toggle in admin → /admin/plugins."
