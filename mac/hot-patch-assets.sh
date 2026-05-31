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

copy_one() {
    local src="$1"
    local dst="$2"
    if [[ ! -f "$src" ]]; then
        echo "[hot-patch] ✗ source $src missing" >&2
        exit 1
    fi
    cp "$src" "$dst"
    local sha=$(shasum "$src" | awk '{print $1}' | cut -c1-12)
    echo "[hot-patch] ✓ $(basename "$src") ($sha)"
}

case "$KIND" in
    icon)
        copy_one "mac/Resources/inputx_app_icon.icns" "$APP/Contents/Resources/inputx_app_icon.icns"
        ;;
    menu-icon)
        # `inputx_menu_icon.tiff` is the per-IME indicator shown in
        # System Settings → 文本输入 → 输入法 + the menu-bar input
        # picker. Distinct from the bundle .icns (which is for Dock
        # / Finder). 32×32 template TIFF (RGBA, black on transparent,
        # TISIconIsTemplate=true). Wired in Info.plist as
        # `tsInputMethodIconFileKey` + `tsInputModeMenuIconFileKey`
        # + `tsInputModePaletteIconFileKey`.
        copy_one "mac/Resources/inputx_menu_icon.tiff" "$APP/Contents/Resources/inputx_menu_icon.tiff"
        # TIS caches the indicator. Purge IntlDataCache + restart
        # TextInputMenuAgent so the new TIFF gets re-read.
        CACHE=$(getconf DARWIN_USER_CACHE_DIR 2>/dev/null || echo /tmp)
        rm -f "$CACHE"/com.apple.IntlDataCache.le* 2>/dev/null || true
        killall TextInputMenuAgent 2>/dev/null || true
        echo "[hot-patch] ✓ IntlDataCache purged + TextInputMenuAgent restarted"
        ;;
    all)
        copy_one "mac/Resources/inputx_app_icon.icns" "$APP/Contents/Resources/inputx_app_icon.icns"
        copy_one "mac/Resources/inputx_menu_icon.tiff" "$APP/Contents/Resources/inputx_menu_icon.tiff"
        CACHE=$(getconf DARWIN_USER_CACHE_DIR 2>/dev/null || echo /tmp)
        rm -f "$CACHE"/com.apple.IntlDataCache.le* 2>/dev/null || true
        killall TextInputMenuAgent 2>/dev/null || true
        echo "[hot-patch] ✓ IntlDataCache purged + TextInputMenuAgent restarted"
        ;;
    "")
        echo "Usage: $0 <kind> [--no-dock]" >&2
        echo "  kinds: icon | menu-icon | all" >&2
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
