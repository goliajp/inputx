#!/usr/bin/env bash
# Build KenLM CLI tools (lmplz / build_binary / query) for Phase-2 LM
# training. Idempotent: re-running is a no-op if binaries already exist.
#
# Why this script exists, not `brew install kenlm`:
#   - Homebrew has no `kenlm` formula.
#   - `pip install kenlm` only ships libkenlm.dylib + Python query binding,
#     no CLI tools.
#   - `git clone kpu/kenlm + cmake` out-of-the-box fails on Boost 1.90:
#     find_package(Boost ... COMPONENTS system thread ...) does not find
#     `boost_system` because boost::system is header-only on Boost 1.90
#     and brew ships no `libboost_system.dylib` anymore.
#
# Fix: patch CMakeLists.txt to drop the `system` component (it's header-
# only and not needed for linking). thread / program_options / unit_test_
# framework are still real libs (brew Boost 1.90 ships them) and stay.
#
# Prerequisites: brew install boost eigen cmake
#
# Output: tools/scoring/09_bigram_lm/kenlm_src/build/bin/{lmplz,build_binary,query}

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
KENLM_DIR="$SCRIPT_DIR/kenlm_src"
BUILD_DIR="$KENLM_DIR/build"

if [[ -x "$BUILD_DIR/bin/lmplz" \
   && -x "$BUILD_DIR/bin/build_binary" \
   && -x "$BUILD_DIR/bin/query" ]]; then
  echo "[build_kenlm] binaries already built at $BUILD_DIR/bin — skip"
  exit 0
fi

# 1. Clone KenLM
if [[ ! -d "$KENLM_DIR/.git" ]]; then
  echo "[build_kenlm] cloning kpu/kenlm…"
  git clone --quiet --depth 1 https://github.com/kpu/kenlm "$KENLM_DIR"
fi

# 2. Patch CMakeLists for Boost 1.90 (remove header-only `system`)
CMAKE_FILE="$KENLM_DIR/CMakeLists.txt"
if grep -q '  system$' "$CMAKE_FILE"; then
  echo "[build_kenlm] patching CMakeLists.txt for Boost 1.90 (drop system component)…"
  # The component block runs `program_options / system / thread / unit_test_framework`
  # — drop only `system`.
  sed -i.bak '/^  system$/d' "$CMAKE_FILE"
fi

# 3. Configure + build lmplz, build_binary, query
mkdir -p "$BUILD_DIR"
cd "$BUILD_DIR"
echo "[build_kenlm] cmake config…"
cmake .. -DCMAKE_BUILD_TYPE=Release > cmake.log 2>&1
echo "[build_kenlm] building lmplz, build_binary, query…"
make -j8 lmplz build_binary query > make.log 2>&1

# 4. Verify
echo "[build_kenlm] done. Binaries at $BUILD_DIR/bin/"
ls -la "$BUILD_DIR/bin/" | grep -E "lmplz|build_binary|query"
