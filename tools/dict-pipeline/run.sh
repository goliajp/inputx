#!/usr/bin/env bash
# Orchestrator — harvest fresh corpus + diff against shipped dicts.
#
# What it does:
#   1. Run corpus-harvest for each configured source (default: zh-wikipedia).
#      Skips download/parse if output TSV already exists for the date.
#   2. For each (corpus, engine) pair, run diff_corpus.py.
#      Output: tools/dict-pipeline/output/<source>-vs-<engine>/{new,dropped,freq}.tsv
#   3. Print summary table.
#
# What it does NOT do (yet — v1.7.1.c / v1.8):
#   - rebuild .idf files (vNEXT snapshot) from fresh corpus + dict merge
#   - automatically verify baseline_quality_test invariant
#   - sequence multiple harvest dates / sources beyond zh-wikipedia
#
# Usage:
#   tools/dict-pipeline/run.sh                          # default date from manifest
#   tools/dict-pipeline/run.sh 20260501                 # specific date
#   tools/dict-pipeline/run.sh 20260501 --dry-run       # smoke test (100 articles)

set -euo pipefail
cd "$(dirname "$0")/../.."  # repo root

DATE="${1:-}"
DRY_RUN_FLAG=""
SUFFIX=""
for arg in "$@"; do
    if [[ "$arg" == "--dry-run" ]]; then
        DRY_RUN_FLAG="--dry-run"
        SUFFIX="-dry"
    fi
done

# Engines to diff against. Add nihongo when its weights.tsv path is wired.
ENGINES=(pinyin wubi)
SOURCE="zh-wikipedia"

# 1. Harvest
echo "[dict-pipeline] harvest: $SOURCE"
if [[ -n "$DATE" ]]; then
    python3 tools/corpus-harvest/harvest_zh_wikipedia.py --date "$DATE" $DRY_RUN_FLAG 2>&1 \
        | grep -E "^=== stats|articles_parsed|unique_words|output_path|wrote" | head -8
else
    python3 tools/corpus-harvest/harvest_zh_wikipedia.py $DRY_RUN_FLAG 2>&1 \
        | grep -E "^=== stats|articles_parsed|unique_words|output_path|wrote" | head -8
fi

# Locate the harvest output (the harvester picks the path; we infer it)
DATE_EFFECTIVE="${DATE:-20260501}"  # mirrors manifest sample_date default
HARVEST_TSV="tools/corpus-harvest/output/${SOURCE}/${DATE_EFFECTIVE}${SUFFIX}.tsv"
if [[ ! -f "$HARVEST_TSV" ]]; then
    echo "[dict-pipeline] ERROR: expected harvest output not found: $HARVEST_TSV" >&2
    exit 1
fi

# 2. Diff against each engine
for ENGINE in "${ENGINES[@]}"; do
    WEIGHTS_PATH="core/crates/inputx-${ENGINE}/data/weights/weights.tsv"
    if [[ ! -f "$WEIGHTS_PATH" ]]; then
        echo "[dict-pipeline] WARN: $WEIGHTS_PATH missing, skipping $ENGINE diff" >&2
        continue
    fi
    OUT_DIR="tools/dict-pipeline/output/${SOURCE}-vs-${ENGINE}"
    echo ""
    echo "[dict-pipeline] diff: $SOURCE vs $ENGINE"
    python3 tools/dict-pipeline/diff_corpus.py \
        --harvest "$HARVEST_TSV" \
        --weights "$WEIGHTS_PATH" \
        --weights-schema "$ENGINE" \
        --out "$OUT_DIR" 2>&1 | grep -E "new_entries|dropped|freq_changes|wrote" | tail -4
done

# 3. Summary
echo ""
echo "[dict-pipeline] summary"
printf "  %-35s  %12s  %12s  %12s\n" "diff" "new_entries" "dropped" "freq_changes"
for ENGINE in "${ENGINES[@]}"; do
    OUT_DIR="tools/dict-pipeline/output/${SOURCE}-vs-${ENGINE}"
    [[ -d "$OUT_DIR" ]] || continue
    NEW=$(($(wc -l < "$OUT_DIR/new_entries.tsv") - 1))         # subtract header
    DROP=$(($(wc -l < "$OUT_DIR/dropped_candidates.tsv") - 1))
    FREQ=$(($(wc -l < "$OUT_DIR/freq_changes.tsv") - 1))
    printf "  %-35s  %12d  %12d  %12d\n" "${SOURCE}-vs-${ENGINE}" "$NEW" "$DROP" "$FREQ"
done

echo ""
echo "[dict-pipeline] reports: tools/dict-pipeline/output/*"
