#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

APP_NAME="Inputx"
PROJECT_ROOT="$(cd .. && pwd)"
CORE_DIR="$PROJECT_ROOT/core"
APPLE_PKG="$PROJECT_ROOT/platform/apple"
BUILD_DIR="$PROJECT_ROOT/build"
APP_DIR="$BUILD_DIR/$APP_NAME.app"

# Universal Mach-O (arm64 + x86_64) — empirical gate for macOS 26 input-source
# picker enumeration. Apple's 17 bundled IMEs are universal (x86_64+arm64e),
# SogouWBInput (the one third-party IME that enumerates on this system) is
# universal (x86_64+arm64); Inputx as thin arm64 was filtered. Building all
# three layers (Rust core, SPM InputxKit, swiftc IMK glue) per-arch and
# lipo-merging is the cheapest fix that aligns with the only known-working shape.

echo "[build] building Rust core (release, per-arch for universal)"
(cd "$CORE_DIR" && cargo build --release --target aarch64-apple-darwin)
(cd "$CORE_DIR" && cargo build --release --target x86_64-apple-darwin)

# Resolve cargo's real target directory (honors $CARGO_TARGET_DIR / wrapper
# redirection to external SSD), then lipo per-arch .a into a universal one
# at a stable local path build.sh can reference unconditionally.
CARGO_TARGET_ROOT="$(cd "$CORE_DIR" && cargo metadata --no-deps --format-version 1 \
    | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
CORE_UNIVERSAL_DIR="$BUILD_DIR/rust-universal"
mkdir -p "$CORE_UNIVERSAL_DIR"
lipo -create \
    "$CARGO_TARGET_ROOT/aarch64-apple-darwin/release/libinputx_core.a" \
    "$CARGO_TARGET_ROOT/x86_64-apple-darwin/release/libinputx_core.a" \
    -output "$CORE_UNIVERSAL_DIR/libinputx_core.a"

echo "[build] building shared Swift layer (platform/apple/InputxKit) per-arch"
# SPM in `--arch arm64 --arch x86_64` mode produces only universal .o (no
# .a archive), which we can't `-lInputxKit` against. So build per-arch and
# lipo the .a's ourselves.
(cd "$APPLE_PKG" && swift build --arch arm64   --configuration release)
(cd "$APPLE_PKG" && swift build --arch x86_64 --configuration release)
SWIFTKIT_ARM64_DIR="$(cd "$APPLE_PKG" && swift build --arch arm64   --configuration release --show-bin-path)"
SWIFTKIT_X86_DIR="$(cd "$APPLE_PKG" && swift build --arch x86_64 --configuration release --show-bin-path)"
SWIFTKIT_UNIVERSAL_DIR="$BUILD_DIR/swiftkit-universal"
mkdir -p "$SWIFTKIT_UNIVERSAL_DIR"
lipo -create \
    "$SWIFTKIT_ARM64_DIR/libInputxKit.a" \
    "$SWIFTKIT_X86_DIR/libInputxKit.a" \
    -output "$SWIFTKIT_UNIVERSAL_DIR/libInputxKit.a"

echo "[build] cleaning $APP_DIR"
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS"
mkdir -p "$APP_DIR/Contents/Resources"

echo "[build] compiling IMK glue per-arch + lipo-merging into universal binary"
SWIFT_SOURCES=(
    Sources/main.swift
    Sources/Globals.swift
    Sources/IMEController.swift
    Sources/CandidatePanel.swift
    Sources/MenubarSettings.swift
)
# Each swiftc invocation needs the matching-arch .swiftmodule on -I (Swift
# refuses to load a module compiled for a different arch). Per-arch SPM
# builds put `Modules/` next to the per-arch bin-path.
for ARCH_PAIR in "arm64:$SWIFTKIT_ARM64_DIR" "x86_64:$SWIFTKIT_X86_DIR"; do
    ARCH="${ARCH_PAIR%%:*}"
    SWIFTKIT_DIR="${ARCH_PAIR#*:}"
    swiftc \
        -target "${ARCH}-apple-macos13.0" \
        -framework Cocoa \
        -framework InputMethodKit \
        -I "$SWIFTKIT_DIR/Modules" \
        -I "$APPLE_PKG/Sources/InputxCoreC" \
        -L "$SWIFTKIT_DIR" \
        -lInputxKit \
        -L "$CORE_UNIVERSAL_DIR" \
        -linputx_core \
        -O \
        -o "$BUILD_DIR/$APP_NAME.$ARCH" \
        "${SWIFT_SOURCES[@]}"
done
lipo -create \
    "$BUILD_DIR/$APP_NAME.arm64" \
    "$BUILD_DIR/$APP_NAME.x86_64" \
    -output "$APP_DIR/Contents/MacOS/$APP_NAME"
rm -f "$BUILD_DIR/$APP_NAME.arm64" "$BUILD_DIR/$APP_NAME.x86_64"

echo "[build] verifying Mach-O is universal"
lipo -info "$APP_DIR/Contents/MacOS/$APP_NAME"

echo "[build] copying Info.plist + Resources"
cp Info.plist "$APP_DIR/Contents/Info.plist"
# Resources are referenced by Info.plist's tsInputMethodIconFileKey etc.;
# without the actual files in place, macOS' input-source picker treats the
# bundle as malformed and silently filters it out of enumeration.
cp -R Resources/. "$APP_DIR/Contents/Resources/"

# Generate `.icns` from the TIFF menu icon so CFBundleIconFile /
# CFBundleIconName resolve to a real macOS-standard app icon. Without this,
# LaunchServices' `icons:` / `iconName:` / `icon flags:` fields stay empty
# for the bundle (vChewing's bundle has them, ours did not until we added
# this step). Single-resolution `.icns` is fine for an IME menu icon.
sips -s format icns Resources/inputx_menu_icon.tiff \
    --out "$APP_DIR/Contents/Resources/inputx_app_icon.icns" >/dev/null

printf "APPLINPX" > "$APP_DIR/Contents/PkgInfo"

# GOLIA K.K. Apple Development cert. The (W6GKU3U95X) suffix in the cert CN
# is a per-cert identifier, NOT the team ID — the team ID is KF79DRC524.
# We sign by SHA so the team-ID distinction doesn't matter here, but it does
# matter for iOS xcodebuild's DEVELOPMENT_TEAM (see ios/project.yml).
# Override with: SIGN_IDENTITY="..." ./build.sh
SIGN_IDENTITY="${SIGN_IDENTITY:-159E4E05CB2166A0641FAF1A8AE61A0FE0277D0D}"
echo "[build] signing as: $SIGN_IDENTITY"
# Default to secure Apple TSA timestamp — Apple notarytool rejects
# signatures without one. Local-only smoke tests can `SIGN_TIMESTAMP=none
# ./build.sh` to skip the TSA round-trip (saves ~1s).
TIMESTAMP_ARG="--timestamp"
[ "${SIGN_TIMESTAMP:-}" = "none" ] && TIMESTAMP_ARG="--timestamp=none"
codesign --force --deep \
    --options runtime \
    --entitlements Inputx.entitlements \
    "$TIMESTAMP_ARG" \
    --sign "$SIGN_IDENTITY" \
    "$APP_DIR"

echo "[build] verifying signature"
codesign --verify --verbose=2 "$APP_DIR" 2>&1 | tail -5

echo "[build] done: $APP_DIR"
