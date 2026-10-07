#!/usr/bin/env sh
# Builds Guardian Strike for the browser into demo3d/dist.
#
#   demo3d/build.sh            # optimized build (slow: ~10 min the first time)
#   demo3d/build.sh webdev     # faster build for iterating
#
# Needs: rustup target wasm32-unknown-unknown, wasm-bindgen-cli 0.2.129
# (cargo install wasm-bindgen-cli --version 0.2.129), and optionally
# wasm-opt (npm install binaryen) for a smaller download.
set -e
PROFILE="${1:-release}"
cd "$(dirname "$0")/.."
cargo build -p guardian_strike --profile "$PROFILE" --target wasm32-unknown-unknown
OUT=demo3d/dist
rm -rf "$OUT" && mkdir -p "$OUT"
wasm-bindgen --target web --no-typescript --out-dir "$OUT" --out-name guardian_strike \
  "target/wasm32-unknown-unknown/$PROFILE/guardian_strike.wasm"
WASM="$OUT/guardian_strike_bg.wasm"
OPT="${WASM_OPT:-$(command -v wasm-opt || true)}"
if [ "$PROFILE" = "release" ] && [ -n "$OPT" ]; then
  "$OPT" -Oz --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext \
    --enable-mutable-globals --enable-reference-types --enable-multivalue "$WASM" -o "$WASM.opt"
  mv "$WASM.opt" "$WASM"
fi
# Ship the module gzipped; the page decompresses it with DecompressionStream.
gzip -9 -c "$WASM" > "$OUT/guardian_strike_bg.wasm.gz" && rm "$WASM"
cp -r demo3d/assets "$OUT/assets"
cp demo3d/web/index.html "$OUT/index.html"
ls -la "$OUT"
