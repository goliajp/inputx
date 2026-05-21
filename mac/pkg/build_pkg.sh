#!/usr/bin/env bash
# Build a .pkg installer for Inputx — production-grade distribution.
#
# Output: build/Inputx-1.0.0.pkg
# Layout: payload installs to /Library/Input Methods/Inputx.app, then
# `scripts/postinstall` registers with Launch Services and prompts the
# user to log out + back in (one-time macOS requirement for fresh IME).
#
# Signing & notarization:
#   - If a Developer ID Installer certificate is in the keychain, the
#     .pkg is signed + notarized + stapled. Gatekeeper accepts silently.
#   - If not, the .pkg ships unsigned; on first open the user has to
#     control-click → Open to bypass Gatekeeper. The .app *inside* is
#     still fully Dev-ID-signed + notarized so it runs fine after install.

set -euo pipefail
cd "$(dirname "$0")/.."   # mac/

APP_NAME="Inputx"
VERSION="$(/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" Info.plist)"
PROJECT_ROOT="$(cd .. && pwd)"
BUILD_DIR="$PROJECT_ROOT/build"
APP_SRC="$BUILD_DIR/${APP_NAME}.app"
STAGE="$BUILD_DIR/pkg-stage"
PKG_OUT="$BUILD_DIR/${APP_NAME}-${VERSION}.pkg"
NOTARY_PROFILE="${NOTARY_PROFILE:-inputx-notary}"

if [ ! -d "$APP_SRC" ]; then
    echo "[pkg] No build at $APP_SRC — run ./build.sh first." >&2
    exit 1
fi

# Stage the payload: only Inputx.app, into a tree that mirrors the install
# location. `pkgbuild --install-location /Library/Input\ Methods` then
# unpacks STAGE into that root.
rm -rf "$STAGE"
mkdir -p "$STAGE"
cp -R "$APP_SRC" "$STAGE/"

# Resolve Installer signing identity (optional). If we have one, use it;
# otherwise build unsigned and warn the operator.
INSTALLER_ID=""
if [ -n "${INSTALLER_SIGN_IDENTITY:-}" ]; then
    INSTALLER_ID="$INSTALLER_SIGN_IDENTITY"
else
    CAND=$(/usr/bin/security find-identity -v -p basic | /usr/bin/grep "Developer ID Installer" | /usr/bin/head -1 | /usr/bin/awk -F'"' '{print $2}' || true)
    [ -n "$CAND" ] && INSTALLER_ID="$CAND"
fi

PKGBUILD_ARGS=(
    --root "$STAGE"
    --install-location "/Library/Input Methods"
    --scripts pkg/scripts
    --identifier "jp.golia.inputmethod.wubi.pkg"
    --version "$VERSION"
    --ownership recommended
)
if [ -n "$INSTALLER_ID" ]; then
    echo "[pkg] signing with: $INSTALLER_ID"
    PKGBUILD_ARGS+=(--sign "$INSTALLER_ID")
else
    echo "[pkg] no 'Developer ID Installer' cert in keychain — building UNSIGNED .pkg."
    echo "      Add the Installer cert at developer.apple.com (same flow as Application cert)"
    echo "      to make the .pkg pass Gatekeeper without the right-click-Open dance."
fi

echo "[pkg] pkgbuild → $PKG_OUT"
/usr/bin/pkgbuild "${PKGBUILD_ARGS[@]}" "$PKG_OUT"

# Notarize the .pkg (only if signed AND we have a stored notarytool profile).
if [ -n "$INSTALLER_ID" ] && /usr/bin/security find-generic-password -s "com.apple.gke.notary.tool" -a "$NOTARY_PROFILE" >/dev/null 2>&1; then
    echo "[pkg] notarytool submit $PKG_OUT"
    /usr/bin/xcrun notarytool submit "$PKG_OUT" \
        --keychain-profile "$NOTARY_PROFILE" --wait
    /usr/bin/xcrun stapler staple "$PKG_OUT"
    /usr/bin/xcrun stapler validate "$PKG_OUT"
else
    echo "[pkg] skipping notarization (unsigned .pkg, or no '$NOTARY_PROFILE' notary profile)"
fi

# Purge the staging .app — once the .pkg is sealed, the loose stage copy
# only pollutes LaunchServices (see mac/_purge_ls.sh for the full story).
# Override with KEEP_BUILD_APP=1 to keep $STAGE for inspection.
if [ "${KEEP_BUILD_APP:-0}" != "1" ]; then
    # shellcheck source=../_purge_ls.sh
    source "$(dirname "$0")/../_purge_ls.sh"
    purge_ls_app "$STAGE/$APP_NAME.app"
    rmdir "$STAGE" 2>/dev/null || true
fi

echo
echo "[pkg] ✓ done: $PKG_OUT"
ls -lh "$PKG_OUT"
/usr/bin/shasum -a 256 "$PKG_OUT"
