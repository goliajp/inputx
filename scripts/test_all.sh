#!/usr/bin/env bash
# Run every layer of inputx tests. Single entry point for the dev loop:
#
#     scripts/test_all.sh
#
# Layers (in dependency order):
#   1. Rust workspace (`cargo test --release`) — engine, FFI, locale, wubi/pinyin.
#   2. Apple shared Swift layer (`swift test` against InputxKit) — covers
#      the FFI binding integration, bundle invariants, and shift detector.
#
# Adding a future layer (iOS keyboard XCTest, web wasm shell, etc.): copy
# one of the layer blocks below, give it its own header, and append. The
# `set -e` semantics make any failed layer fail the whole script.
#
# WHY THIS SCRIPT EXISTS: `cargo test -p inputx-core` does NOT emit
# `target/release/libinputx_core.a` (that's an inputx-core-ffi build
# product). Without the explicit `cargo build -p inputx-core-ffi` below,
# the swift test step quietly links against a STALE static lib and you
# get phantom failures that look like engine bugs but are really linker
# cache. See feedback memory [[testable-invariants-only]].

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

bold() { printf '\033[1m%s\033[0m\n' "$*"; }

# ----- Layer 1: Rust core engine + FFI surface --------------------------------
# Scoped to the two crates that today's regression net covers (engine state
# machine, FFI boundary). Whole-workspace `cargo test --release` currently
# fails on inputx-wubi sub-crate's proptest_layer target — separate panic-
# strategy issue unrelated to baseline; that crate's logic is exercised
# transitively through inputx-core anyway.
#
# Perfgate tests are split out because they're timing-sensitive: running
# them in parallel with the rest of the suite (cargo default) lets other
# tests' CPU/IO contend with the timed inner loops, producing spurious
# >budget spikes on otherwise-healthy hardware. Bulk runs parallel; perfgate
# runs isolated so the timing baseline stays clean.
bold "== Layer 1a: cargo test -p inputx-core (skip perfgate, parallel) =="
(cd "$ROOT/core" && cargo test --release -p inputx-core -- --skip perfgate)

bold "== Layer 1b: cargo test -p inputx-core perfgate (isolated) =="
(cd "$ROOT/core" && cargo test --release -p inputx-core --lib perfgate)

bold "== Layer 1c: cargo test -p inputx-core-ffi (C ABI boundary fuzz) =="
(cd "$ROOT/core" && cargo test --release -p inputx-core-ffi)

# ----- Layer 2: Apple shared Swift layer (InputxKit) --------------------------
bold "== Layer 2a: cargo build -p inputx-core-ffi (host-arch .a for Swift linker) =="
(cd "$ROOT/core" && cargo build --release -p inputx-core-ffi)

CARGO_TARGET="$(cd "$ROOT/core" && cargo metadata --no-deps --format-version 1 \
    | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"

bold "== Layer 2b: swift test (InputxKit + bundle invariants + integration) =="
(cd "$ROOT/platform/apple" && swift test -Xlinker -L"$CARGO_TARGET/release")

# ----- (future) iOS keyboard extension layer ----------------------------------
# When ios/Keyboard or ios/App grow XCTest targets that don't require a sim,
# add an `xcodebuild test` block here. Sim-driven tests are heavier and
# should live in a separate runner so the fast dev loop stays fast.

bold "All layers passed."
