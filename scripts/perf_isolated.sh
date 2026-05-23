#!/usr/bin/env bash
# Run the inputx-core perfgate in isolation — single-threaded, no other
# workspace crates running in parallel. This is the honest perf gate;
# the in-tree test uses p95 (not max) so it survives parallel-cargo-test
# contention, but the real story is verified here, with both p95 AND max
# checked against the 16ms frame budget.
#
# Usage:
#   scripts/perf_isolated.sh            # default: run inputx-core perfgate
#   scripts/perf_isolated.sh --release  # explicit release flag (default on)
#
# Exits non-zero on any FAIL line in test output.

set -euo pipefail

cd "$(dirname "$0")/.."

# `cargo test` exits 0 even if test asserts fail because of how we surface
# results via `eprintln + assert!(all_passed || cfg!(debug_assertions))`.
# Capture output, then grep for FAIL.
OUT=$(cd core && cargo test --release -p inputx-core --lib \
  --features perfgate \
  perfgate_refresh_candidates_under_budget \
  -- --nocapture --test-threads=1 2>&1)

echo "$OUT"

if echo "$OUT" | grep -q "FAIL:"; then
  echo
  echo "[perf_isolated] FAIL — see perfgate output above"
  exit 1
fi

# Also require the test binary itself exited green (no panic).
if echo "$OUT" | grep -q "test result: FAILED"; then
  echo "[perf_isolated] FAIL — test panicked"
  exit 1
fi

echo
echo "[perf_isolated] OK"
