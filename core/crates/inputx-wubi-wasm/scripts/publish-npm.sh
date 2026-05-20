#!/usr/bin/env bash
# Build the WASM package and publish to npm under the @goliapkg scope.
# Run from the wubi-wasm crate directory.

set -euo pipefail

CRATE_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$CRATE_DIR"

NPM_NAME="${NPM_NAME:-@goliapkg/wubi}"
TARGET="${WASM_PACK_TARGET:-web}"

echo "[publish-npm] wasm-pack build --target $TARGET --release"
wasm-pack build --target "$TARGET" --release

# wasm-pack defaults the package name to the Cargo crate name (`wubi-wasm`).
# Rewrite pkg/package.json to publish under the desired scoped name.
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
  cd pkg
  npm publish --access public

(Login first via 'npm login --scope=@goliapkg' if you haven't.)
EOF
