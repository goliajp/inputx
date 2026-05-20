#!/usr/bin/env bash
set -euo pipefail

WEB_DIR="$(cd "$(dirname "$0")" && pwd)"
CRATE_DIR="$(cd "$WEB_DIR/.." && pwd)"

echo "[web] wasm-pack build --target web --release"
(cd "$CRATE_DIR" && wasm-pack build --target web --release)

PORT="${PORT:-8000}"
echo
echo "[web] open http://localhost:$PORT/web/"
echo "[web] (serving from crate root so ../pkg/ resolves)"
cd "$CRATE_DIR"
exec python3 -m http.server "$PORT"
