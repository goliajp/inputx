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
    Sources/InputModeToast.swift
    Sources/SettingsWindow.swift
    Sources/PolishLog.swift
    Sources/PerfTimer.swift
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
# CP-5.2 step-3: ship the bundled cell-dict packs from `docs/cell-dicts/`
# at runtime path `Resources/cell-dicts/*.toml`. SettingsWindow scans
# this directory via `InputxCellDictRegistry.bundled(in: .main)` and
# renders one toggle per pack; user prefs key each pack by filename stem.
CELL_DICT_SRC="../docs/cell-dicts"
CELL_DICT_DST="$APP_DIR/Contents/Resources/cell-dicts"
if [ -d "$CELL_DICT_SRC" ]; then
    mkdir -p "$CELL_DICT_DST"
    cp -f "$CELL_DICT_SRC"/*.toml "$CELL_DICT_DST/" 2>/dev/null || true
    echo "[build] bundled cell-dicts: $(ls "$CELL_DICT_DST"/*.toml 2>/dev/null | wc -l | tr -d ' ') pack(s)"
fi

# v1.15 hot-reload: ship the pinyin data blobs into the bundle so
# reinstall.py's data-only fast path can atomically replace them
# without killing Inputx.app. Startup reads them via
# InputxCore.setPinyinDataDirectory before IMKServer construction.
PINYIN_DATA_DST="$APP_DIR/Contents/Resources/data"
mkdir -p "$PINYIN_DATA_DST"
cp -f "$PROJECT_ROOT/core/crates/inputx-pinyin-data-core/data/pinyin.dict"    "$PINYIN_DATA_DST/"
cp -f "$PROJECT_ROOT/core/crates/inputx-pinyin-helpers/data/words.idf"        "$PINYIN_DATA_DST/"
cp -f "$PROJECT_ROOT/core/crates/inputx-pinyin-helpers/data/bigrams.ngm"      "$PINYIN_DATA_DST/"
cp -f "$PROJECT_ROOT/core/crates/inputx-pinyin-helpers/data/bigrams_inter.ngm" "$PINYIN_DATA_DST/"

# v1.16 hot-reload: v2 engine reads polish overlay TSVs at runtime
# via ArcSwap slots. Ship the 6 polish TSVs so Session::reload_pinyin_data
# can find them at `Contents/Resources/data/polish/` and swap them in
# on SIGUSR1 without a binary swap.
POLISH_DST="$PINYIN_DATA_DST/polish"
mkdir -p "$POLISH_DST"
cp -f "$PROJECT_ROOT/tools/scoring/data/polish/tier_overlay.tsv"             "$POLISH_DST/"
cp -f "$PROJECT_ROOT/tools/scoring/data/polish/quickfix_boost.tsv"           "$POLISH_DST/"
cp -f "$PROJECT_ROOT/tools/scoring/data/polish/exclusions_v1.tsv"            "$POLISH_DST/"
cp -f "$PROJECT_ROOT/tools/scoring/data/polish/prior_corrections_v1.tsv"     "$POLISH_DST/"
cp -f "$PROJECT_ROOT/tools/scoring/data/polish/modern_vocab_v1.tsv"          "$POLISH_DST/"
cp -f "$PROJECT_ROOT/tools/scoring/data/polish/corpus_garbage_filter_v1.tsv" "$POLISH_DST/"

shasum -a 256 "$PINYIN_DATA_DST"/*.dict "$PINYIN_DATA_DST"/*.idf "$PINYIN_DATA_DST"/*.ngm \
    > "$PINYIN_DATA_DST/manifest.sha256"
shasum -a 256 "$POLISH_DST"/*.tsv > "$POLISH_DST/manifest.sha256"
echo "[build] bundled pinyin data: $(ls "$PINYIN_DATA_DST" | grep -Ev '^manifest|^polish' | wc -l | tr -d ' ') file(s) + $(ls "$POLISH_DST" | grep -v '^manifest' | wc -l | tr -d ' ') polish tsv(s)"

printf "APPLINPX" > "$APP_DIR/Contents/PkgInfo"

# ----- Codesign -----
# Default to the Apple Development cert; override with SIGN_IDENTITY="..."
# for Developer ID / distribution signing.
# Apple's cert team-ID is the OU field, NOT the parenthesized identifier
# in the cert CN — see notes in mac/release.sh.
SIGN_IDENTITY="${SIGN_IDENTITY:-491B13377E1850BDBFB56310CDF1A94B31CF8AE6}"
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
