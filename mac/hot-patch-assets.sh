#!/usr/bin/env bash
# Hot-patch resource files into the installed Inputx.app bundle WITHOUT
# killing the running Inputx process. Use this for changes that don't
# affect the Rust/Swift binary or Info.plist: icons, READMEs, polish-
# log templates, anything under Contents/Resources/.
#
# Why this exists: mac/reinstall.sh's standard flow does `pkill -x Inputx`
# + launchd respawn. Even though launchd brings the new PID up cleanly,
# host apps' IMK clients hold a Mach-port reference to the OLD PID and
# don't auto-rebind. Result: any app that had a TextField focused
# before reinstall can't reach the new Inputx until the app itself
# restarts. For pure-asset changes (e.g., icon updates) this is pure
# cost — the IMK runtime sees no change.
#
# This script:
#   1. Copies the source asset(s) into the installed bundle.
#   2. Touches the bundle mtime so LaunchServices re-scans.
#   3. `lsregister -f` to refresh LS's icon / metadata cache.
#   4. `killall Dock` so the Dock picks up new icon.
#   5. LEAVES Inputx alone — no PID churn, no IMK breakage.
#
# Currently handles the icon. Extend the FILES list if you add other
# pure-asset changes.
#
# Usage:
#   mac/hot-patch-assets.sh icon          # patch the .icns
#   mac/hot-patch-assets.sh icon --no-dock  # skip the Dock restart

set -euo pipefail
cd "$(dirname "$0")/.."

LSREGISTER=/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister
APP="$HOME/Library/Input Methods/Inputx.app"

if [[ ! -d "$APP" ]]; then
    echo "[hot-patch] ✗ Inputx not installed at $APP" >&2
    echo "[hot-patch]   run ./mac/reinstall.sh first" >&2
    exit 1
fi

KIND="${1:-}"
shift || true
RESTART_DOCK=1
for arg in "$@"; do
    [[ "$arg" == "--no-dock" ]] && RESTART_DOCK=0
done

case "$KIND" in
    icon)
        SRC="mac/Resources/inputx_app_icon.icns"
        DST="$APP/Contents/Resources/inputx_app_icon.icns"
        if [[ ! -f "$SRC" ]]; then
            echo "[hot-patch] ✗ source $SRC missing — run python3 mac/generate_icon.py" >&2
            exit 1
        fi
        cp "$SRC" "$DST"
        SRC_SHA=$(shasum "$SRC" | awk '{print $1}' | cut -c1-12)
        echo "[hot-patch] ✓ icon copied ($SRC_SHA)"
        ;;
    "")
        echo "Usage: $0 <kind> [--no-dock]" >&2
        echo "  kinds: icon" >&2
        exit 2
        ;;
    *)
        echo "[hot-patch] unknown kind: $KIND" >&2
        exit 2
        ;;
esac

# Force LS to re-read the bundle metadata + icon. Without -f, LS caches
# the prior icon ref by bundle ID + mtime.
touch "$APP"
$LSREGISTER -f "$APP" 2>&1
echo "[hot-patch] ✓ lsregister -f"

if (( RESTART_DOCK )); then
    killall Dock 2>/dev/null || true
    echo "[hot-patch] ✓ Dock restarted"
fi

# Verify Inputx wasn't disturbed.
PID=$(pgrep -x Inputx | head -1 || true)
if [[ -n "$PID" ]]; then
    echo "[hot-patch] ✓ Inputx still running, PID=$PID (no IMK breakage)"
else
    echo "[hot-patch] ⚠ Inputx not running — was it already down? Try ./mac/reinstall.sh" >&2
fi
