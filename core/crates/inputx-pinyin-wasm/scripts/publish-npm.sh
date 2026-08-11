#!/usr/bin/env bash
# Build the WASM package and publish to npm under the @goliapkg scope.
# Run from anywhere; the script resolves paths relative to itself.
#
# Mirror of wubi-wasm/scripts/publish-npm.sh — same flow, swapped names.

set -euo pipefail

CRATE_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$CRATE_DIR"

NPM_NAME="${NPM_NAME:-@goliapkg/pinyin}"
TARGET="${WASM_PACK_TARGET:-web}"

# --features bootstrap_only keeps the bundle ~108 KB. Pass FEATURES="" to
# bake the full ~15 MB dict instead (item 33 streaming-load deferred).
FEATURES="${FEATURES:---features bootstrap_only}"
echo "[publish-npm] wasm-pack build --target $TARGET --release $FEATURES"
wasm-pack build --target "$TARGET" --release -- $FEATURES

# wasm-pack defaults the package name to the Cargo crate name
# (`inputx-pinyin-wasm`). Rewrite pkg/package.json to publish under the
# desired scoped name.
echo "[publish-npm] rewriting pkg/package.json name → $NPM_NAME"
/usr/bin/python3 - <<PY
import json, pathlib
p = pathlib.Path("pkg/package.json")
data = json.loads(p.read_text())
data["name"] = "$NPM_NAME"
p.write_text(json.dumps(data, indent=2) + "\n")
PY

echo "[publish-npm] dry-run pack:"
( cd pkg && npm pack --dry-run )

cat <<EOF

To actually publish, run:
  cd $CRATE_DIR/pkg
  npm publish --access public

(Login first via 'npm login --scope=@goliapkg' if you haven't.)
EOF
