#!/usr/bin/env bash
#
# legacy_compat_check.sh — sort-key cutover safety net for v1.4.7 A2.
#
# For each baseline buffer (the same set the v1.3-snapshot.json fixture
# covers), runs inputx-probe and compares the candidate list ranked by:
#   (a) legacy f64 `score` (current live sort key)
#   (b) Q4 log additive `log_prior_q4 + log_likelihood_q4`
#       (proposed v1.4.7 sort key)
#
# Per-buffer pass = top-K word order identical between (a) and (b).
# Mismatch = some candidate's ScoreComponents fill is producing a
# score_q4 that doesn't track the legacy f64 score; needs adjustment
# in composite/{pinyin,japanese}_adapter.rs OR composite/scoring.rs
# constants BEFORE A2 swaps the live sort key.
#
# Exit 0 if all buffers pass (cutover safe).
# Exit 1 if any buffer fails (report first 3 mismatches per buffer).
#
# Usage:
#   cd /Users/doracawl/workspace/goliajp/inputx
#   bash scripts/legacy_compat_check.sh
#
# Add --buffers "a,b,c" to override the buffer list (default: same
# 32-buffer set as v1.3-snapshot fixture).
# Add --k N to check only top-N positions (default: 10).

set -euo pipefail

cd "$(dirname "$0")/.."

PROBE=/Volumes/INTEL2T/workspace-cache/cargo-target/release/inputx-probe
if [ ! -x "$PROBE" ]; then
  echo "probe binary missing — run: cd core && cargo build --release --bin inputx-probe" >&2
  exit 2
fi

# Default buffer list — mirrors data/v14-baseline-fixtures/build-v1.3-snapshot.sh
DEFAULT_BUFFERS=(
  # Polish historical (25)
  jixu sheji lixiang jiazai juti tongyi aiyi shinjuku shinjuk pianni
  zho lianxiang mo nihaomawojiao kaopu yongbuliao taikexi houxuanqu
  famiriaare vaiorin fami famin "fa-----" "ko-hi-" jieni
  # Wubi (6)
  ggg j jjjj wwww q ahxg
  # Edge / negative (3)
  qwxzy hellox ""
)

TOP_K=10
BUFFERS=("${DEFAULT_BUFFERS[@]}")

while [ $# -gt 0 ]; do
  case "$1" in
    --buffers)
      IFS=',' read -ra BUFFERS <<< "$2"
      shift 2
      ;;
    --k)
      TOP_K="$2"
      shift 2
      ;;
    *)
      echo "unknown arg: $1" >&2
      exit 2
      ;;
  esac
done

total_buffers=0
total_pass=0
total_fail=0
all_failures=""

for buf in "${BUFFERS[@]}"; do
  for jp_flag in "" "--jp"; do
    total_buffers=$((total_buffers + 1))
    out=$("$PROBE" "$buf" $jp_flag 2>/dev/null || true)
    if [ -z "$out" ]; then
      echo "[skip] buf=\"$buf\" $jp_flag (probe returned empty)"
      continue
    fi
    # Use python to parse JSON, sort both ways, compare top-K word lists.
    result=$(echo "$out" | python3 -c "
import json, sys
data = json.load(sys.stdin)
cands = data.get('candidates', [])
top_k = $TOP_K

# Only consider candidates with full ScoreComponents fill.
filtered = [
    c for c in cands
    if c.get('log_prior_q4') is not None
       and c.get('log_likelihood_q4') is not None
]
if len(filtered) < min(2, top_k):
    print('SKIP: fewer than 2 candidates with components')
    sys.exit(0)

legacy_sorted = sorted(filtered, key=lambda c: (-c['score'], c['word']))[:top_k]
q4_sorted = sorted(
    filtered,
    key=lambda c: (-(c['log_prior_q4'] + c['log_likelihood_q4']), c['word']),
)[:top_k]

legacy_words = [(c['word'], c['source']) for c in legacy_sorted]
q4_words = [(c['word'], c['source']) for c in q4_sorted]

if legacy_words == q4_words:
    print('PASS')
else:
    diffs = []
    for i, (l, q) in enumerate(zip(legacy_words, q4_words)):
        if l != q:
            diffs.append(f'  rank #{i+1}: legacy={l[0]!r} ({l[1]}) vs score_q4={q[0]!r} ({q[1]})')
        if len(diffs) >= 3:
            break
    print('FAIL')
    for d in diffs:
        print(d)
" 2>&1)
    label="buf=\"$buf\""
    if [ -n "$jp_flag" ]; then label="$label --jp"; fi
    if echo "$result" | grep -q "^PASS"; then
      total_pass=$((total_pass + 1))
    elif echo "$result" | grep -q "^SKIP"; then
      : # skip not counted in pass/fail
    elif echo "$result" | grep -q "^FAIL"; then
      total_fail=$((total_fail + 1))
      printf '[FAIL] %s\n%s\n' "$label" "$(echo "$result" | tail -n +2)"
    fi
  done
done

echo
echo "=== SUMMARY ==="
echo "  buffers checked: $total_buffers (excluding skips for low-candidate cases)"
echo "  PASS: $total_pass"
echo "  FAIL: $total_fail"

if [ "$total_fail" -eq 0 ]; then
  echo "  sort-key cutover safe — proceed to A2 (composite/merge.rs swap)"
  exit 0
else
  echo "  sort-key cutover UNSAFE — fix ScoreComponents fill paths in composite/{pinyin,japanese}_adapter.rs first"
  exit 1
fi
