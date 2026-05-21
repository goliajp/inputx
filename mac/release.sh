#!/usr/bin/env bash
# Mac release pipeline: build → Developer-ID sign → notarize → staple → dmg.
#
# Prerequisites (one-time setup):
#
#   1. **Developer ID Application certificate** — different from the
#      "Apple Development" cert used by `build.sh` for local testing.
#      Create one at developer.apple.com → Certificates → "+" →
#      "Developer ID Application", download and install into Keychain.
#      Verify with:
#          security find-identity -v -p codesigning | grep "Developer ID"
#
#   2. **App-specific password for notarytool** — at appleid.apple.com →
#      Sign In and Security → App-Specific Passwords. Store it for
#      `notarytool store-credentials`:
#          xcrun notarytool store-credentials inputx-notary \
#              --apple-id <your-apple-id> \
#              --team-id KF79DRC524 \
#              --password <app-specific-password>
#
# Env overrides:
#   RELEASE_SIGN_IDENTITY  Developer ID Application SHA-1 / common name.
#                          Auto-detected if exactly one is in the keychain.
#   NOTARY_PROFILE         notarytool keychain profile name (default
#                          inputx-notary).
#   RELEASE_VERSION        Marketing version baked into the .dmg name.
#                          Defaults to Info.plist's CFBundleShortVersionString.
#   SKIP_NOTARIZE=1        Build + sign only; skip notarize + staple +
#                          dmg. Useful for local smoke-testing.

set -euo pipefail
cd "$(dirname "$0")"

APP_NAME="Inputx"
PROJECT_ROOT="$(cd .. && pwd)"
BUILD_DIR="$PROJECT_ROOT/build"
APP_DIR="$BUILD_DIR/$APP_NAME.app"

NOTARY_PROFILE="${NOTARY_PROFILE:-inputx-notary}"

VERSION="${RELEASE_VERSION:-$(/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" Info.plist)}"
DMG_NAME="${APP_NAME}-${VERSION}.dmg"
DMG_PATH="$BUILD_DIR/$DMG_NAME"

# --- Resolve Developer ID signing identity --------------------------------
if [ -z "${RELEASE_SIGN_IDENTITY:-}" ]; then
    DEVID_CERTS="$(security find-identity -v -p codesigning | grep "Developer ID Application" || true)"
    if [ -z "$DEVID_CERTS" ]; then
        cat <<'EOF' >&2

[release] ✗ No "Developer ID Application" cert in keychain.

This script needs a Developer ID Application certificate to produce a
notarizable .app — the cert used by ./build.sh ("Apple Development") is
NOT suitable for distribution. Create one at:

    https://developer.apple.com/account/resources/certificates/list

Then re-run this script. See the comment block at the top for full setup
(including notarytool credentials).

EOF
        exit 1
    fi
    RELEASE_SIGN_IDENTITY="$(echo "$DEVID_CERTS" | head -1 | awk -F'"' '{print $2}')"
fi

echo "[release] sign identity: $RELEASE_SIGN_IDENTITY"
echo "[release] notary profile: $NOTARY_PROFILE"
echo "[release] version:        $VERSION"

# --- 1. Build (release) -----------------------------------------------------
echo "[release] running build.sh with Developer ID identity"
SIGN_IDENTITY="$RELEASE_SIGN_IDENTITY" ./build.sh

if [ "${SKIP_NOTARIZE:-0}" = "1" ]; then
    echo "[release] SKIP_NOTARIZE=1 — stopping after build+sign."
    echo "[release] App at: $APP_DIR"
    exit 0
fi

# --- 2. Zip the .app for notarytool submission -----------------------------
# notarytool requires a flat archive; .dmg works but slows the loop. Use a
# transient .zip for the upload, then build the user-facing .dmg later.
TMP_ZIP="$BUILD_DIR/${APP_NAME}-${VERSION}-notarize.zip"
echo "[release] zipping for notarytool: $TMP_ZIP"
rm -f "$TMP_ZIP"
( cd "$BUILD_DIR" && /usr/bin/ditto -c -k --keepParent "$APP_NAME.app" "$TMP_ZIP" )

# --- 3. Submit to Apple notary, wait for result ----------------------------
echo "[release] notarytool submit (this takes a few minutes)"
NOTARY_OUTPUT="$(xcrun notarytool submit "$TMP_ZIP" \
    --keychain-profile "$NOTARY_PROFILE" \
    --wait \
    --output-format plist 2>&1)"
echo "$NOTARY_OUTPUT"

STATUS="$(echo "$NOTARY_OUTPUT" | /usr/libexec/PlistBuddy -c "Print :status" /dev/stdin 2>/dev/null || true)"
SUB_ID="$(echo "$NOTARY_OUTPUT" | /usr/libexec/PlistBuddy -c "Print :id" /dev/stdin 2>/dev/null || true)"

if [ "$STATUS" != "Accepted" ]; then
    echo "[release] ✗ notarization status = '$STATUS' (expected Accepted)."
    echo "[release] fetch log:"
    xcrun notarytool log "$SUB_ID" --keychain-profile "$NOTARY_PROFILE" || true
    exit 1
fi
rm -f "$TMP_ZIP"

# --- 4. Staple the ticket onto the .app -----------------------------------
echo "[release] stapling ticket onto $APP_DIR"
xcrun stapler staple "$APP_DIR"
xcrun stapler validate "$APP_DIR"

# --- 5. Build the distributable .dmg --------------------------------------
# Plain .dmg via hdiutil. For the marketing-friendly "drag to Applications"
# layout, you'd add an Applications symlink + background image; v1 keeps
# it minimal.
echo "[release] building $DMG_PATH"
rm -f "$DMG_PATH"
hdiutil create \
    -volname "$APP_NAME $VERSION" \
    -srcfolder "$APP_DIR" \
    -ov \
    -format UDZO \
    "$DMG_PATH"

# Sign the .dmg too (notarized .app inside an unsigned .dmg still warns
# Gatekeeper on first open).
codesign --force --sign "$RELEASE_SIGN_IDENTITY" "$DMG_PATH"
echo "[release] verifying .dmg signature"
codesign --verify --verbose=2 "$DMG_PATH" 2>&1 | tail -2

# --- 6. Optional: notarize the .dmg too ------------------------------------
# Belt-and-suspenders. Many distributors notarize both the .app and the
# wrapping .dmg so first-mount on a Gatekeeper-stricter machine doesn't
# stall.
echo "[release] notarizing .dmg"
xcrun notarytool submit "$DMG_PATH" \
    --keychain-profile "$NOTARY_PROFILE" \
    --wait
xcrun stapler staple "$DMG_PATH"
xcrun stapler validate "$DMG_PATH"

# --- 7. Purge intermediate .app from LS + disk ----------------------------
# The .dmg above is the sealed deliverable; keeping build/Inputx.app around
# lets LaunchServices register it and shadow the real install under
# /Library/Input Methods/ (see mac/_purge_ls.sh for the full story).
# Override with KEEP_BUILD_APP=1 if you need the loose .app for inspection.
if [ "${KEEP_BUILD_APP:-0}" != "1" ]; then
    # shellcheck source=./_purge_ls.sh
    source "$(dirname "$0")/_purge_ls.sh"
    purge_ls_app "$APP_DIR"
fi

echo
echo "[release] ✓ done."
echo "[release] artifact: $DMG_PATH"
echo "[release] sha256:"
shasum -a 256 "$DMG_PATH"
