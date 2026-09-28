#!/usr/bin/env bash
# Run `cargo test` as a gate: fail on any test failure, and fail when fewer
# tests passed than the floor, so a filtered-out or uncompiled test target
# cannot turn the gate green. Raise the floor when tests are added in bulk;
# lower it only when tests are deliberately removed.
#
# usage: scripts/cargo-test-gate.sh <min_passed> <cargo test args...>
set -euo pipefail

min_passed=$1
shift

out=$(mktemp)
trap 'rm -f "$out"' EXIT

cargo test "$@" 2>&1 | tee "$out" | tail -3

passed=$(grep -E '^test result:' "$out" \
    | sed -E 's/.* ([0-9]+) passed.*/\1/' \
    | awk '{ total += $1 } END { print total + 0 }')

if (( passed < min_passed )); then
    echo "[gate] cargo test $*: $passed passed, expected at least $min_passed" >&2
    exit 1
fi
echo "[gate] cargo test $*: $passed passed (floor $min_passed)"
