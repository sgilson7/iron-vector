#!/usr/bin/env bash
# Build the release WebAssembly module and assemble the static site in dist/.
# Needs: Rust with the wasm32-unknown-unknown target, and the wasm-bindgen CLI
# at the version pinned in crates/shim/Cargo.toml.
set -euo pipefail
cd "$(dirname "$0")/.."

PIN=$(sed -n 's/^wasm-bindgen = "=\(.*\)"/\1/p' crates/shim/Cargo.toml)
HAVE=$(wasm-bindgen --version 2>/dev/null | awk '{print $2}')
if [ "$HAVE" != "$PIN" ]; then
  echo "wasm-bindgen CLI is '${HAVE:-missing}', the shim pins $PIN." >&2
  echo "Install it with: cargo install wasm-bindgen-cli --version $PIN --locked" >&2
  exit 1
fi

cargo build --release --target wasm32-unknown-unknown -p iv_shim

rm -rf dist
mkdir -p dist/pkg
wasm-bindgen --target web --no-typescript --out-dir dist/pkg \
  target/wasm32-unknown-unknown/release/iv_shim.wasm
cp web/* dist/
cp -R data dist/data

# Cache busting (from gear-master-2d by way of slushline). GitHub Pages serves
# every file with a ten-minute cache, so a returning player would keep the old
# module after a deploy. The content hash goes into every internal URL, and a
# changed build is a different URL.
sha256() { if command -v shasum >/dev/null; then shasum -a 256; else sha256sum; fi }
BUILD=$(find dist -type f | LC_ALL=C sort | xargs cat | sha256 | cut -c1-8)
for js in dist/*.js; do
  perl -0777 -pi -e "s{from \"\./([A-Za-z0-9_/-]+\.js)\"}{from \"./\$1?v=$BUILD\"}g; s/__BUILD__/$BUILD/g" "$js"
done
perl -0777 -pi -e "s{new URL\('iv_shim_bg\.wasm', import\.meta\.url\)}{new URL('iv_shim_bg.wasm?v=$BUILD', import.meta.url)}g" dist/pkg/iv_shim.js
perl -0777 -pi -e "s{src=\"app\.js\"}{src=\"app.js?v=$BUILD\"}; s{href=\"style\.css\"}{href=\"style.css?v=$BUILD\"}" dist/index.html
grep -q "app.js?v=$BUILD" dist/index.html || { echo "cache busting did not apply to index.html" >&2; exit 1; }
grep -q "iv_shim_bg.wasm?v=$BUILD" dist/pkg/iv_shim.js || { echo "wasm URL not stamped" >&2; exit 1; }
if grep -nE 'from "\./[A-Za-z0-9_/-]+\.js"' dist/*.js; then
  echo "an import above is unstamped and would be served stale after a deploy" >&2
  exit 1
fi
echo "$BUILD $(git rev-parse --short HEAD 2>/dev/null || echo none)" > dist/build.txt
touch dist/.nojekyll

echo "Built dist/ (build $BUILD):"
find dist -type f | sort
ls -la dist/pkg/iv_shim_bg.wasm | awk '{printf "wasm: %.0f KB\n", $5/1024}'
