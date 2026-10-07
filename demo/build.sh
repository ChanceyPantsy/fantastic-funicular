#!/usr/bin/env sh
# Builds the arena to WebAssembly and copies it next to index.html.
# Then serve demo/web with any static file server, e.g.:
#   python3 -m http.server -d demo/web 8080
set -e
cd "$(dirname "$0")/.."
rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true
cargo build -p guardian_arena --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/guardian_arena.wasm demo/web/
echo "Built demo/web/guardian_arena.wasm"
