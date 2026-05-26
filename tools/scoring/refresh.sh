#!/usr/bin/env bash
# refresh.sh — periodic pinyin dict refresh (the Layer-3 mechanism).
#
# Composes four steps into one cron-able command:
#   1. EXPAND   — regenerate hand-curated supplemental TSVs (modern vocab).
#   2. RESCORE  — mine polish-log into a fresh quickfix_boost.tsv (lifts
#                 entries the user reached past #0 to pick).
#   3. AUDIT    — run audit_pinyin_overlays.py to reject garbage rows
#                 from supplemental + quickfix (non-CJK words, punctuation,
#                 malformed freq, duplicates).
#   4. REBUILD  — apply low-freq cutoff + overlays in pinyin-build-fst,
#                 producing the new core/crates/inputx-pinyin/data/pinyin.fst.
#
# Output: diff report listing what changed so the operator can decide
# whether to commit + reinstall.
#
# Usage:
#   ./tools/scoring/refresh.sh                # full refresh, no install
#   ./tools/scoring/refresh.sh --reinstall    # also rebuild .app + reinstall
#
# Idempotent: re-running on a clean state is a no-op.

set -uo pipefail
cd "$(dirname "$0")"

REPO_ROOT="$(cd ../.. && pwd)"
FST_PATH="$REPO_ROOT/core/crates/inputx-pinyin/data/pinyin.fst"
MODERN_TSV="$REPO_ROOT/tools/scoring/data/supplemental/pinyin_modern_v1.tsv"
QUICKFIX_TSV="$REPO_ROOT/tools/scoring/data/polish_reports/quickfix_boost.tsv"

# Snapshot pre-state for diff report.
FST_SIZE_BEFORE=$(stat -f%z "$FST_PATH" 2>/dev/null || echo 0)
MODERN_LINES_BEFORE=$(wc -l < "$MODERN_TSV" 2>/dev/null | tr -d ' ' || echo 0)
QUICKFIX_LINES_BEFORE=$(wc -l < "$QUICKFIX_TSV" 2>/dev/null | tr -d ' ' || echo 0)

echo "[refresh] step 1/4 — EXPAND: regenerate supplemental TSVs"
python3 build_pinyin_modern.py || { echo "[refresh] expand failed"; exit 1; }

echo "[refresh] step 2/4 — RESCORE: mine polish-log into quickfix_boost.tsv"
python3 07_validate/aggregate_polish_log.py 2>&1 | tail -10 || { echo "[refresh] rescore failed"; exit 1; }

echo "[refresh] step 3/4 — AUDIT: validate + clean overlays"
python3 audit_pinyin_overlays.py || { echo "[refresh] audit failed"; exit 1; }

echo "[refresh] step 4/4 — REBUILD: apply cutoff + overlays → pinyin.fst"
(cd "$REPO_ROOT/core" && cargo run --release --features tools --bin pinyin-build-fst 2>&1 | tail -6) \
  || { echo "[refresh] FST build failed"; exit 1; }

# Diff report.
FST_SIZE_AFTER=$(stat -f%z "$FST_PATH" 2>/dev/null || echo 0)
MODERN_LINES_AFTER=$(wc -l < "$MODERN_TSV" 2>/dev/null | tr -d ' ' || echo 0)
QUICKFIX_LINES_AFTER=$(wc -l < "$QUICKFIX_TSV" 2>/dev/null | tr -d ' ' || echo 0)

cat <<EOF

[refresh] === DIFF ===
  pinyin.fst:                 $FST_SIZE_BEFORE → $FST_SIZE_AFTER bytes
  modern_v1.tsv lines:        $MODERN_LINES_BEFORE → $MODERN_LINES_AFTER
  quickfix_boost.tsv lines:   $QUICKFIX_LINES_BEFORE → $QUICKFIX_LINES_AFTER

  next: review tsv diffs, then commit + reinstall:
    git diff tools/scoring/data/
    ./mac/reinstall.sh

EOF

# Optional: rebuild + reinstall the .app if --reinstall passed.
if [ "${1:-}" = "--reinstall" ]; then
  echo "[refresh] reinstalling Inputx.app"
  "$REPO_ROOT/mac/reinstall.sh"
fi
