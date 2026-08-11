#!/usr/bin/env bash
# mac/scripts/purge-stray-ls.sh — sweep stray `.app` bundles in the
# project tree out of macOS LaunchServices.
#
# Why: macOS LaunchServices auto-registers any `.app` directory it
# encounters under `~/` — including iOS-platform bundles. The project's
# `ios/build-{device,sim}/Build/Products/*/InputxApp.app` artifacts
# (created by `xcodebuild -destination "iOS Device"`) get registered
# as platform=iOS apps and surface alongside the macOS IME in the
# input-source picker — user sees "two Inputx" after any iOS build.
#
# Run this:
#   - automatically: every `mac/reinstall.py` (via the same logic in
#     reinstall.py's `purge_stray_project_bundles_from_launchservices`)
#   - manually: after `xcodebuild` for iOS, if you don't plan to do
#     a reinstall soon and notice the duplicate. `make purge-stray-ls`
#     is the convenience target.
#
# What it preserves:
#   - The canonical macOS install at ~/Library/Input Methods/Inputx.app
#   - The build/ output at <repo>/build/Inputx.app
#   - The iOS bundle files on disk (only their LS registration is
#     dropped — devicectl install still works)
#
# Exit 0 always; missing bundles are no-ops.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
APP_NAME="Inputx"
INSTALL_DST="$HOME/Library/Input Methods/${APP_NAME}.app"
BUILD_SRC="$PROJECT_ROOT/build/${APP_NAME}.app"
LSREG="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"

# Resolve symlinks for comparison.
resolve() { python3 -c 'import sys, pathlib; print(pathlib.Path(sys.argv[1]).resolve())' "$1"; }
KEEP_INSTALL="$(resolve "$INSTALL_DST" 2>/dev/null || echo "$INSTALL_DST")"
KEEP_BUILD="$(resolve "$BUILD_SRC" 2>/dev/null || echo "$BUILD_SRC")"

found=0
while IFS= read -r -d '' app; do
    resolved="$(resolve "$app" 2>/dev/null || echo "$app")"
    if [ "$resolved" = "$KEEP_INSTALL" ] || [ "$resolved" = "$KEEP_BUILD" ]; then
        continue
    fi
    rel="${app#$PROJECT_ROOT/}"
    echo "[purge-stray-ls] unregistering: $rel"
    "$LSREG" -u "$app" >/dev/null 2>&1 || true
    found=$((found + 1))
done < <(find "$PROJECT_ROOT" -type d -name '*.app' -print0 2>/dev/null)

if [ "$found" -eq 0 ]; then
    echo "[purge-stray-ls] no stray bundles found"
else
    echo "[purge-stray-ls] cleaned $found stray entries"
    # Cascade: invalidate IntlDataCache + restart picker so the picker
    # re-enumerates without the just-removed entries.
    CACHE="$(getconf DARWIN_USER_CACHE_DIR 2>/dev/null || true)"
    if [ -n "${CACHE:-}" ]; then
        rm -f "$CACHE"/com.apple.IntlDataCache.le "$CACHE"/com.apple.IntlDataCache.le.kbdx 2>/dev/null || true
    fi
    killall TextInputMenuAgent 2>/dev/null || true
fi
