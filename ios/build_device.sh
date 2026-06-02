#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

# No device defaults are committed — supply your own device via env vars:
#   IOS_DEVICE_NAME / IOS_HW_UDID / IOS_COREDEV_ID
# Avoid ${VAR:-default} forms with apostrophes — bash chokes on them.
DEVICE_NAME="My iPhone"
HW_UDID=""        # set via IOS_HW_UDID  — xcodebuild -destination id=...
COREDEV_ID=""     # set via IOS_COREDEV_ID — devicectl --device
[ -n "${IOS_DEVICE_NAME:-}" ] && DEVICE_NAME="$IOS_DEVICE_NAME"
[ -n "${IOS_HW_UDID:-}" ] && HW_UDID="$IOS_HW_UDID"
[ -n "${IOS_COREDEV_ID:-}" ] && COREDEV_ID="$IOS_COREDEV_ID"
SCHEME="InputxApp"
CONTAINER_BUNDLE="jp.golia.inputx"

# Build configuration. Default Debug for fast dev iter; pass IOS_CONFIG=Release
# to test real-world perf (Swift -O + WMO — keyboard ops actually feel native).
CONFIG="Debug"
[ -n "${IOS_CONFIG:-}" ] && CONFIG="$IOS_CONFIG"

echo "[ios-device] regenerating Xcode project (xcodegen)"
xcodegen generate --quiet

# Cargo's effective target dir may be redirected by a global wrapper
# (see ~/.claude-shared/global/cargo-target-dir.md). project.yml's
# LIBRARY_SEARCH_PATHS expects core/target/<arch>/release/, so when
# the redirect IS active, symlink core/target to the real target dir.
# When NO redirect is active, cargo's real target already IS
# core/target — leave it alone (linking to itself creates a circular
# dead-symlink that crashes `cargo build --release` with "Not a
# directory", which is exactly how the iOS build broke 2026-06-02).
CORE_TARGET_ABS=$(cd ../core && pwd)/target
CARGO_REAL_TARGET=$(cd ../core && cargo metadata --no-deps --format-version 1 \
    | /usr/bin/python3 -c 'import json,sys;print(json.load(sys.stdin)["target_directory"])')
if [ "$CARGO_REAL_TARGET" != "$CORE_TARGET_ABS" ]; then
    if [ ! -L ../core/target ] || [ "$(readlink ../core/target)" != "$CARGO_REAL_TARGET" ]; then
        [ -e ../core/target ] && rm -rf ../core/target
        ln -s "$CARGO_REAL_TARGET" ../core/target
    fi
fi

echo "[ios-device] building Rust core for aarch64-apple-ios"
(cd ../core && cargo build --release --target aarch64-apple-ios)

echo "[ios-device] target: $DEVICE_NAME (hw=$HW_UDID, core=$COREDEV_ID)"
echo "[ios-device] xcodebuild ($CONFIG, iphoneos)"
xcodebuild \
    -project Inputx.xcodeproj \
    -scheme "$SCHEME" \
    -configuration "$CONFIG" \
    -destination "id=$HW_UDID" \
    -derivedDataPath build-device \
    -allowProvisioningUpdates \
    -allowProvisioningDeviceRegistration \
    -quiet \
    build

APP_PATH="build-device/Build/Products/${CONFIG}-iphoneos/InputxApp.app"
[ -d "$APP_PATH" ] || { echo "Build product not found at $APP_PATH"; exit 1; }

echo "[ios-device] installing on device"
xcrun devicectl device install app --device "$COREDEV_ID" "$APP_PATH"

cat <<EOF

[ios-device] done. On the iPhone:
  1) (First-time only) Settings → General → VPN & Device Management → Apple
     Development → Trust this developer. Skip if you've installed dev apps
     from GOLIA K.K. before.
  2) Open Inputx app once to confirm the install.
  3) Settings → 通用 → 键盘 → 键盘 → 添加新键盘 → Inputx
  4) Open Notes → tap 🌐 → switch to Inputx → tap any letter
EOF
