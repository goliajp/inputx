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

pkill -x "$APP_NAME" 2>/dev/null || true

# Remove any prior install. If a `.pkg` previously installed Inputx, the
# bundle may be root-owned and need an admin prompt to clear.
if [ -d "$APP_DST" ]; then
    if [ -w "$APP_DST" ] && [ -w "$APP_DST/Contents" ]; then
        rm -rf "$APP_DST"
    else
        echo "[install] removing root-owned prior install (admin password)"
        osascript -e "do shell script \"rm -rf '$APP_DST'\" with administrator privileges"
    fi
fi

echo "[install] → $APP_DST"
mkdir -p "$(dirname "$APP_DST")"
cp -R "$APP_SRC" "$APP_DST"

# Register with TIS + enable each declared input mode (see main.swift
# `install` CLI handler). Runs as the current user so the registration
# lands in the user's TIS database.
"$APP_DST/Contents/MacOS/$APP_NAME" install

# Restart TextInputMenuAgent so the menu-bar input-source picker re-reads
# its display cache after our new bundle entered TIS.
killall TextInputMenuAgent 2>/dev/null || true

# Install + (re)load the user LaunchAgent that keeps the IME binary always
# running. Without this, macOS 26's imklaunchagent silently refuses to launch
# our binary when a host app requests the IME service, and typing produces
# nothing even though the picker shows us as selectable. See
# LaunchAgent.plist.template and docs/macos-ime-recipe-2026.md §Investigation 6.
LA_DST="$HOME/Library/LaunchAgents/jp.golia.inputmethod.wubi.plist"
mkdir -p "$(dirname "$LA_DST")"
sed "s|__APP_PATH__|$APP_DST|g" "$(dirname "$0")/Resources/LaunchAgent.plist.template" > "$LA_DST"
launchctl bootout "gui/$(id -u)/jp.golia.inputmethod.wubi" 2>/dev/null || true
launchctl bootstrap "gui/$(id -u)" "$LA_DST"
echo "[install] LaunchAgent loaded — binary will auto-start at login + restart on crash"

# Unregister the build/ source bundle from LaunchServices so the csstore
# doesn't shadow the install with a stale entry. See _purge_ls.sh.
if [ "${KEEP_BUILD_APP:-0}" != "1" ]; then
    # shellcheck source=./_purge_ls.sh
    source "$(dirname "$0")/_purge_ls.sh"
    purge_ls_app "$APP_SRC"
fi

cat <<EOF

Installed. Next steps:
  1. System Settings → Keyboard → Text Input → Edit → +
  2. Add: 简体中文 → Inputx 五笔
  3. Approve the third-party IME prompt
EOF
