#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

APP_NAME="Inputx"
PROJECT_ROOT="$(cd .. && pwd)"
APP_SRC="$PROJECT_ROOT/build/$APP_NAME.app"

# macOS 26.x (Tahoe) won't enumerate user-local third-party IMEs in
# `~/Library/Input Methods/` reliably anymore — the System Settings
# "Add Input Source" picker only sees system-wide installs at
# `/Library/Input Methods/`. Default to system-wide here so the .app
# actually shows up after install. Set `USER_INSTALL=1` to keep the old
# behaviour (e.g., for testing without sudo).
if [ "${USER_INSTALL:-0}" = "1" ]; then
    APP_DST="$HOME/Library/Input Methods/$APP_NAME.app"
    INSTALL_CMD=(cp -R "$APP_SRC" "$APP_DST")
    NEEDS_SUDO=false
else
    APP_DST="/Library/Input Methods/$APP_NAME.app"
    INSTALL_CMD=(sudo cp -R "$APP_SRC" "$APP_DST")
    NEEDS_SUDO=true
fi

if [ ! -d "$APP_SRC" ]; then
    echo "Build not found at $APP_SRC. Run ./build.sh first." >&2
    exit 1
fi

# Kill running instance so the binary isn't busy.
pkill -x "$APP_NAME" 2>/dev/null || true

if [ "$NEEDS_SUDO" = "true" ]; then
    echo "[install] system-wide install (sudo required) → $APP_DST"
    sudo rm -rf "$APP_DST"
    sudo mkdir -p "$(dirname "$APP_DST")"
else
    echo "[install] user-local install → $APP_DST"
    rm -rf "$APP_DST"
    mkdir -p "$(dirname "$APP_DST")"
fi

"${INSTALL_CMD[@]}"
echo "[install] copied to: $APP_DST"

# Force the IME catalog to re-enumerate. Without this the picker may
# still show the cached list from before the install.
killall -KILL TextInputMenuAgent 2>/dev/null || true

echo
echo "Next steps:"
echo "  1) Open System Settings → Keyboard → Input Sources → Edit (or +)"
echo "  2) Add: Chinese (Simplified) → Inputx 输入法"
echo "  3) Switch to it via the menu-bar input source picker"
echo
echo "If it doesn't show up, log out and back in (or reboot) — macOS caches input sources."
