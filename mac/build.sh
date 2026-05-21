#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

APP_NAME="Inputx"
PROJECT_ROOT="$(cd .. && pwd)"
CORE_DIR="$PROJECT_ROOT/core"
APPLE_PKG="$PROJECT_ROOT/platform/apple"
BUILD_DIR="$PROJECT_ROOT/build"
APP_DIR="$BUILD_DIR/$APP_NAME.app"

# ----- Rust core: per-arch build + lipo into universal static lib -----
echo "[build] Rust core (release, per-arch)"
(cd "$CORE_DIR" && cargo build --release --target aarch64-apple-darwin)
(cd "$CORE_DIR" && cargo build --release --target x86_64-apple-darwin)

# Resolve cargo's actual target directory (honors $CARGO_TARGET_DIR / a
# user-defined wrapper redirecting to external SSD).
CARGO_TARGET_ROOT="$(cd "$CORE_DIR" && cargo metadata --no-deps --format-version 1 \
    | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
CORE_UNIVERSAL_DIR="$BUILD_DIR/rust-universal"
mkdir -p "$CORE_UNIVERSAL_DIR"
lipo -create \
    "$CARGO_TARGET_ROOT/aarch64-apple-darwin/release/libinputx_core.a" \
    "$CARGO_TARGET_ROOT/x86_64-apple-darwin/release/libinputx_core.a" \
    -output "$CORE_UNIVERSAL_DIR/libinputx_core.a"

# ----- InputxKit (shared Swift layer): per-arch build + lipo -----
# SwiftPM's multi-arch mode produces only universal `.o` files (no `.a`),
# which we can't link against. Per-arch builds + manual lipo give us a
# static archive we can `-lInputxKit` from the IMK glue compile.
echo "[build] InputxKit (release, per-arch)"
(cd "$APPLE_PKG" && swift build --arch arm64  --configuration release)
(cd "$APPLE_PKG" && swift build --arch x86_64 --configuration release)
SWIFTKIT_ARM64_DIR="$(cd "$APPLE_PKG" && swift build --arch arm64  --configuration release --show-bin-path)"
SWIFTKIT_X86_DIR="$(cd "$APPLE_PKG" && swift build --arch x86_64 --configuration release --show-bin-path)"
SWIFTKIT_UNIVERSAL_DIR="$BUILD_DIR/swiftkit-universal"
mkdir -p "$SWIFTKIT_UNIVERSAL_DIR"
lipo -create \
    "$SWIFTKIT_ARM64_DIR/libInputxKit.a" \
    "$SWIFTKIT_X86_DIR/libInputxKit.a" \
    -output "$SWIFTKIT_UNIVERSAL_DIR/libInputxKit.a"

# ----- IMK glue: per-arch swiftc + lipo into the .app's Mach-O -----
echo "[build] preparing $APP_DIR"
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources"

SWIFT_SOURCES=(
    Sources/main.swift
    Sources/Globals.swift
    Sources/IMEController.swift
    Sources/CandidatePanel.swift
    Sources/MenubarSettings.swift
    Sources/InputModeToast.swift
    Sources/SettingsWindow.swift
    Sources/PolishLog.swift
)
# swiftc refuses cross-arch .swiftmodule loads, so compile each arch
# against the matching-arch SPM bin-path's Modules/ directory.
for ARCH_PAIR in "arm64:$SWIFTKIT_ARM64_DIR" "x86_64:$SWIFTKIT_X86_DIR"; do
    ARCH="${ARCH_PAIR%%:*}"
    SWIFTKIT_DIR="${ARCH_PAIR#*:}"
    swiftc \
        -target "${ARCH}-apple-macos13.0" \
        -framework Cocoa \
        -framework InputMethodKit \
        -I "$SWIFTKIT_DIR/Modules" \
        -I "$APPLE_PKG/Sources/InputxCoreC" \
        -L "$SWIFTKIT_DIR" -lInputxKit \
        -L "$CORE_UNIVERSAL_DIR" -linputx_core \
        -O \
        -o "$BUILD_DIR/$APP_NAME.$ARCH" \
        "${SWIFT_SOURCES[@]}"
done
lipo -create \
    "$BUILD_DIR/$APP_NAME.arm64" \
    "$BUILD_DIR/$APP_NAME.x86_64" \
    -output "$APP_DIR/Contents/MacOS/$APP_NAME"
rm -f "$BUILD_DIR/$APP_NAME.arm64" "$BUILD_DIR/$APP_NAME.x86_64"
lipo -info "$APP_DIR/Contents/MacOS/$APP_NAME"

# ----- Bundle resources -----
cp Info.plist "$APP_DIR/Contents/Info.plist"
# `Resources/inputx_app_icon.icns` is a pre-built multi-resolution .icns
# (16/32/64/128/256/512 + @2x) made via `iconutil -c icns iconset/`.
# Do NOT regenerate from the TIFF via `sips -s format icns` — that produces
# a 1-resolution legacy `il32` blob that crashes host apps on input-source
# switch. See mac/Info.plist comment on CFBundleIconFile for details.
cp -R Resources/. "$APP_DIR/Contents/Resources/"
printf "APPLINPX" > "$APP_DIR/Contents/PkgInfo"

# ----- Codesign -----
# Default to the Apple Development cert; override with SIGN_IDENTITY="..."
# for Developer ID / distribution signing.
# Apple's cert team-ID is the OU field, NOT the parenthesized identifier
# in the cert CN — see notes in mac/release.sh.
SIGN_IDENTITY="${SIGN_IDENTITY:-159E4E05CB2166A0641FAF1A8AE61A0FE0277D0D}"
TIMESTAMP_ARG="--timestamp"
# Local-only smoke tests can `SIGN_TIMESTAMP=none ./build.sh` to skip the
# TSA round-trip (~1s). Apple's notarytool rejects un-timestamped sigs.
[ "${SIGN_TIMESTAMP:-}" = "none" ] && TIMESTAMP_ARG="--timestamp=none"
echo "[build] signing as $SIGN_IDENTITY"
codesign --force --deep \
    --options runtime \
    --entitlements Inputx.entitlements \
    "$TIMESTAMP_ARG" \
    --sign "$SIGN_IDENTITY" \
    "$APP_DIR"
codesign --verify --verbose=2 "$APP_DIR" 2>&1 | tail -3

echo "[build] $APP_DIR"
