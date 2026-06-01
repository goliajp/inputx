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
# What it does NOT do (yet — v1.8):
#   - merge fresh corpus into build_weights stage (today: vNEXT snapshot
#     is byte-identical to shipped because input data unchanged; serves
#     as path verification + baseline invariant proof)
#   - automatically run baseline_quality_test (manual: cargo test
#     -p inputx-core --lib baseline)
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


# 3. vNEXT .idf snapshot — rebuild from current dict source
# Skipped in --dry-run (cargo build is expensive).
if [[ -z "$DRY_RUN_FLAG" ]]; then
    echo ""
    echo "[dict-pipeline] vNEXT .idf snapshot rebuild"
    mkdir -p data/private-dict/vNEXT/pinyin data/private-dict/vNEXT/wubi
    pushd core >/dev/null
    cargo run --release --bin idf-from-pinyin-dict -- \
        --output ../data/private-dict/vNEXT/pinyin/words.idf 2>&1 \
        | grep -E "^wrote|loaded|baked" | tail -4
    cargo run --release --bin idf-from-wubi-tables -- \
        --output ../data/private-dict/vNEXT/wubi/words.idf 2>&1 \
        | grep -E "^wrote|loaded" | tail -3
    popd >/dev/null
    # Verify byte-identical vs shipped (proves baseline invariant — same
    # input bytes → same .idf bytes, so ranking can't have drifted).
    echo ""
    echo "[dict-pipeline] vNEXT byte-identity check (vs shipped .idf):"
    for engine in pinyin wubi; do
        shipped_idf="core/crates/inputx-${engine}-$([[ $engine == pinyin ]] && echo helpers || echo data)/data/words.idf"
        vnext_idf="data/private-dict/vNEXT/${engine}/words.idf"
        if [[ -f "$shipped_idf" && -f "$vnext_idf" ]]; then
            shipped_sha=$(shasum "$shipped_idf" | awk '{print $1}')
            vnext_sha=$(shasum "$vnext_idf" | awk '{print $1}')
            if [[ "$shipped_sha" == "$vnext_sha" ]]; then
                printf "  %-7s ✓ byte-identical (sha %s)\n" "$engine" "${vnext_sha:0:12}"
            else
                printf "  %-7s ✗ DIFFER (shipped=%s vNEXT=%s)\n" "$engine" "${shipped_sha:0:12}" "${vnext_sha:0:12}"
            fi
        fi
    done
fi

echo ""
echo "[dict-pipeline] reports: tools/dict-pipeline/output/*"
echo "[dict-pipeline] vNEXT:   data/private-dict/vNEXT/*"
