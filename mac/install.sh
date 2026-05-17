#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

APP_NAME="Inputx"
PROJECT_ROOT="$(cd .. && pwd)"
APP_SRC="$PROJECT_ROOT/build/$APP_NAME.app"
APP_DST="$HOME/Library/Input Methods/$APP_NAME.app"

if [ ! -d "$APP_SRC" ]; then
    echo "Build not found at $APP_SRC. Run ./build.sh first." >&2
    exit 1
fi

mkdir -p "$HOME/Library/Input Methods"

# Kill running instance so the binary isn't busy
pkill -x "$APP_NAME" 2>/dev/null || true

rm -rf "$APP_DST"
cp -R "$APP_SRC" "$APP_DST"

echo "[install] copied to: $APP_DST"
echo
echo "Next steps:"
echo "  1) Open System Settings → Keyboard → Input Sources → Edit (or +)"
echo "  2) Add: Chinese (Simplified) → Inputx 输入法"
echo "  3) Switch to it via the menu-bar input source picker"
echo
echo "If it doesn't show up, log out and back in (or reboot) — macOS caches input sources."
