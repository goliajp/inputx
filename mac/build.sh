#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

APP_NAME="Inputx"
PROJECT_ROOT="$(cd .. && pwd)"
CORE_DIR="$PROJECT_ROOT/core"
APPLE_PKG="$PROJECT_ROOT/platform/apple"
BUILD_DIR="$PROJECT_ROOT/build"
APP_DIR="$BUILD_DIR/$APP_NAME.app"

echo "[build] building Rust core (release)"
(cd "$CORE_DIR" && cargo build --release)

echo "[build] building shared Swift layer (platform/apple/InputxKit)"
# Build the shared Apple Swift layer (InputxKit + InputxCoreC system lib
# wrapper) as a Swift package; produces .swiftmodule + .a we link against
# from the IME executable. Doing this here (instead of using `swiftc` to
# compile all sources flat) preserves a clean module boundary between
# `InputxKit` (cross-host shared code) and `InputxApp` (Mac-IMK glue).
(cd "$APPLE_PKG" && swift build --configuration release)

# SPM places artifacts under a target-arch subdir (e.g. arm64-apple-macosx);
# resolve dynamically rather than hard-coding so a future x86_64 build still
# works.
SWIFTKIT_BUILD="$(cd "$APPLE_PKG" && swift build --configuration release --show-bin-path)"
SWIFTKIT_MODULES="$SWIFTKIT_BUILD/Modules"

echo "[build] cleaning $APP_DIR"
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS"
mkdir -p "$APP_DIR/Contents/Resources"

echo "[build] compiling IMK glue + linking InputxKit + libinputx_core"
swiftc \
    -target arm64-apple-macos13.0 \
    -framework Cocoa \
    -framework InputMethodKit \
    -I "$SWIFTKIT_MODULES" \
    -I "$APPLE_PKG/Sources/InputxCoreC" \
    -L "$SWIFTKIT_BUILD" \
    -lInputxKit \
    -L "$CORE_DIR/target/release" \
    -linputx_core \
    -O \
    -o "$APP_DIR/Contents/MacOS/$APP_NAME" \
    Sources/main.swift \
    Sources/Globals.swift \
    Sources/IMEController.swift \
    Sources/CandidatePanel.swift \
    Sources/MenubarSettings.swift

echo "[build] copying Info.plist"
cp Info.plist "$APP_DIR/Contents/Info.plist"

printf "APPL????" > "$APP_DIR/Contents/PkgInfo"

# GOLIA K.K. Apple Development cert. The (W6GKU3U95X) suffix in the cert CN
# is a per-cert identifier, NOT the team ID — the team ID is KF79DRC524.
# We sign by SHA so the team-ID distinction doesn't matter here, but it does
# matter for iOS xcodebuild's DEVELOPMENT_TEAM (see ios/project.yml).
# Override with: SIGN_IDENTITY="..." ./build.sh
SIGN_IDENTITY="${SIGN_IDENTITY:-159E4E05CB2166A0641FAF1A8AE61A0FE0277D0D}"
echo "[build] signing as: $SIGN_IDENTITY"
codesign --force --deep \
    --options runtime \
    --timestamp=none \
    --sign "$SIGN_IDENTITY" \
    "$APP_DIR"

echo "[build] verifying signature"
codesign --verify --verbose=2 "$APP_DIR" 2>&1 | tail -5

echo "[build] done: $APP_DIR"
