#!/usr/bin/env bash
# The browser build: dist/ holds the page, myapp.js / .wasm, the assets and drawnui_host.js.
# Needs emsdk's tools on PATH first (source emsdk_env.sh) and `rustup target add wasm32-unknown-emscripten`.
set -euo pipefail
cd "$(dirname "$0")"
cargo build --release --target wasm32-unknown-emscripten
built=target/wasm32-unknown-emscripten/release
mkdir -p dist
cp "$built/myapp.js" "$built/myapp.wasm" dist/
cp -r web/* assets dist/
# The page script of the drawnui version this build linked: it and the wasm speak one protocol.
manifest=$(cargo metadata --format-version 1 | grep -o '"manifest_path":"[^"]*drawnui[/\\]Cargo.toml"' | head -1 | cut -d'"' -f4)
cp "$(dirname "$manifest")/web/drawnui_host.js" dist/
echo "web build: $(pwd)/dist (serve it over http)"
