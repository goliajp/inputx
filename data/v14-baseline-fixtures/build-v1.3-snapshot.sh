#!/usr/bin/env bash
#
# Build data/v14-baseline-fixtures/v1.3-snapshot.json — the v1.3 baseline
# fixture for the v1.4 cycle. Each WU-* cutover diff'd against this snapshot
# must be byte-rank identical (PLAN.md §Baseline fixture spec).
#
# Generates 32 buffers × 2 JP modes (--jp on/off) + 2 mode-switch cases = 66 entries.

set -euo pipefail

cd "$(dirname "$0")/../.."

PROBE=/Volumes/INTEL2T/workspace-cache/cargo-target/release/inputx-probe
OUT="data/v14-baseline-fixtures/v1.3-snapshot.json"

if [ ! -x "$PROBE" ]; then
  echo "probe binary missing at $PROBE — run: cd core && cargo build --release --bin inputx-probe" >&2
  exit 1
fi

POLISH=(
  jixu sheji lixiang jiazai juti tongyi aiyi shinjuku shinjuk pianni
  zho lianxiang mo nihaomawojiao kaopu yongbuliao taikexi houxuanqu
  famiriaare vaiorin fami famin fa----- ko-hi- jieni
)
WUBI=(
  ggg j jjjj wwww q ahxg
)
EDGE_BUFS=(
  qwxzy hellox
)

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT

probe_one() {
  local buf="$1" mode_flag="$2" jp_flag="$3" category="$4"
  local jp_bool
  if [ -n "$jp_flag" ]; then jp_bool=true; else jp_bool=false; fi
  local raw
  raw="$("$PROBE" "$buf" $mode_flag $jp_flag)"
  printf '%s' "$raw" | jq -c \
      --arg buf "$buf" \
      --argjson jp "$jp_bool" \
      --arg cat "$category" \
      '{buffer: $buf, mode: .mode, jp: $jp, category: $cat,
        preedit: .preedit,
        top10: [.candidates[] | {word, source, score}]}'
}

{
  echo '['
  first=1
  emit() {
    if [ $first -eq 1 ]; then first=0; else echo ','; fi
    printf '  '
  }

  for buf in "${POLISH[@]}"; do
    for jp in "" "--jp"; do
      emit
      probe_one "$buf" "" "$jp" "polish"
    done
  done

  for buf in "${WUBI[@]}"; do
    for jp in "" "--jp"; do
      emit
      probe_one "$buf" "" "$jp" "wubi"
    done
  done

  for buf in "${EDGE_BUFS[@]}"; do
    for jp in "" "--jp"; do
      emit
      probe_one "$buf" "" "$jp" "edge"
    done
  done

  # Empty buffer (probe arg ""); skip --jp since jp irrelevant for empty input.
  emit
  probe_one "" "" "" "edge-empty"

  # Mode switch case 1: "nihao" in WubiOnly mode — preedit cleared (5 chars
  # exceeds Wubi 86 4-char window). Captures observable end state.
  emit
  probe_one "nihao" "--mode wubi" "" "mode-switch-wubi-overflow"

  # Mode switch case 2: "nihao" in default Mixed mode, space-commit would
  # commit top1. Captures top1 candidate (= 你好 expected).
  emit
  probe_one "nihao" "" "" "mode-switch-pinyin-default"

  echo
  echo ']'
} > "$tmp"

# Pretty-print + sanity-validate JSON
jq . "$tmp" > "$OUT"

count="$(jq '. | length' "$OUT")"
echo "wrote $OUT — $count entries"
if [ "$count" -lt 40 ]; then
  echo "FAIL: entry count $count < 40 required by PLAN.md WU-α step 4" >&2
  exit 1
fi
