#!/usr/bin/env bash
set -euo pipefail

APP_NAME="Inputx"
APP_DST="$HOME/Library/Input Methods/$APP_NAME.app"

pkill -x "$APP_NAME" 2>/dev/null || true

if [ -d "$APP_DST" ]; then
    rm -rf "$APP_DST"
    echo "[uninstall] removed $APP_DST"
else
    echo "[uninstall] nothing at $APP_DST"
fi

echo
echo "If the input source still appears in System Settings, remove it manually:"
echo "  Settings → Keyboard → Input Sources → select Inputx 输入法 → '-'"
