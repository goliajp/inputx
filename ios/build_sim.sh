#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

DEVICE="${SIMULATOR_DEVICE:-sim-inputx}"
SCHEME="InputxApp"
CONTAINER_BUNDLE="jp.golia.inputx"

CONFIG="Debug"
[ -n "${IOS_CONFIG:-}" ] && CONFIG="$IOS_CONFIG"

echo "[ios] regenerating Xcode project (xcodegen)"
xcodegen generate --quiet

# Rust core for the iOS Simulator. project.yml's LIBRARY_SEARCH_PATHS for
# iphonesimulator points at `aarch64-apple-ios-sim/release`, so the sim
# build needs this target rebuilt whenever Rust source changes (otherwise
# the linker hits old symbol tables — e.g. fails on `_inputx_session_warmup`
# until a fresh cargo build emits it).
# Cargo's effective target dir may be redirected by a global wrapper
# (see ~/.claude-shared/global/cargo-target-dir.md). project.yml's
# LIBRARY_SEARCH_PATHS expects core/target/<arch>/release/, so make
# core/target a symlink to the real target dir.
CARGO_REAL_TARGET=$(cd ../core && cargo metadata --no-deps --format-version 1 \
    | /usr/bin/python3 -c 'import json,sys;print(json.load(sys.stdin)["target_directory"])')
if [ ! -L ../core/target ] || [ "$(readlink ../core/target)" != "$CARGO_REAL_TARGET" ]; then
    [ -e ../core/target ] && rm -rf ../core/target
    ln -s "$CARGO_REAL_TARGET" ../core/target
fi

echo "[ios] building Rust core for aarch64-apple-ios-sim"
(cd ../core && cargo build --release --target aarch64-apple-ios-sim)

# Resolve simulator UDID by name (more reliable than passing the name to simctl).
SIMULATOR_UDID="${SIMULATOR_UDID:-}"
if [ -z "$SIMULATOR_UDID" ]; then
    SIMULATOR_UDID=$(SIMULATOR_DEVICE="$DEVICE" /usr/bin/python3 -c '
import json, os, subprocess, sys
target = os.environ["SIMULATOR_DEVICE"]
out = subprocess.check_output(["xcrun", "simctl", "list", "devices", "--json"])
data = json.loads(out)
for runtime, devices in data["devices"].items():
    for dev in devices:
        if dev["name"] == target and dev.get("isAvailable", False):
            print(dev["udid"])
            sys.exit(0)
sys.exit(1)
')
fi
[ -n "$SIMULATOR_UDID" ] || { echo "Simulator '$DEVICE' not found"; exit 1; }

echo "[ios] booting simulator: $DEVICE ($SIMULATOR_UDID)"
xcrun simctl boot "$SIMULATOR_UDID" 2>/dev/null || true
# Scope the Simulator.app window to THIS sim — without -CurrentDeviceUDID
# `open -a Simulator` brings whichever booted device the app had front,
# which can pull an unrelated sim (e.g. iPhone 17 Pro) into focus.
open -a Simulator --args -CurrentDeviceUDID "$SIMULATOR_UDID"

echo "[ios] xcodebuild ($CONFIG, iPhoneSimulator)"
xcodebuild \
    -project Inputx.xcodeproj \
    -scheme "$SCHEME" \
    -configuration "$CONFIG" \
    -destination "id=$SIMULATOR_UDID" \
    -derivedDataPath build \
    -quiet \
    build

APP_PATH="build/Build/Products/${CONFIG}-iphonesimulator/InputxApp.app"
[ -d "$APP_PATH" ] || { echo "Build product not found at $APP_PATH"; exit 1; }

echo "[ios] installing $APP_PATH"
xcrun simctl install "$SIMULATOR_UDID" "$APP_PATH"

echo "[ios] launching"
xcrun simctl launch "$SIMULATOR_UDID" "$CONTAINER_BUNDLE" >/dev/null

cat <<EOF

[ios] done. In the Simulator:
  1) Close the Inputx app (swipe up)
  2) Settings → General → Keyboard → Keyboards → Add New Keyboard → Inputx
  3) Open Notes → tap 🌐 to switch to Inputx → tap any letter
EOF
