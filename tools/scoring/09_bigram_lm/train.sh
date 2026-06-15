#!/usr/bin/env bash
# Phase-2 CP-2.3: train bigram KenLM, build trie binary, smoke-query it.
# Run from project root: tools/scoring/09_bigram_lm/train.sh
#
# Output files (all under tools/scoring/09_bigram_lm/data/):
#   bigram.arpa     — ARPA-format LM, ~3-5 GB
#   bigram.binary   — KenLM trie binary, ~300 MB-1 GB (mmap-loadable)
#   training.log    — lmplz stderr (ngram counts, OOV rate, memory peak,
#                     time)
#   query.log       — smoke-query output, sanity log_probs
#
# Prerequisites: build_kenlm.sh has run (or rerun it; it's idempotent).
#                cleaned.txt exists (from clean.py --all).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DATA="$SCRIPT_DIR/data"
BIN="$SCRIPT_DIR/kenlm_src/build/bin"

INPUT="$DATA/cleaned.txt"
ARPA="$DATA/bigram.arpa"
BINARY="$DATA/bigram.binary"
TLOG="$DATA/training.log"
QLOG="$DATA/query.log"

[[ -x "$BIN/lmplz" && -x "$BIN/build_binary" && -x "$BIN/query" ]] || {
  echo "[train] KenLM CLI missing — run build_kenlm.sh first"
  exit 1
}
[[ -f "$INPUT" ]] || {
  echo "[train] cleaned.txt missing — run clean.py --all first"
  exit 1
}

echo "[train] input: $INPUT ($(wc -l < "$INPUT") lines)"

# 1. lmplz: count + smooth, output ARPA. order=2 (bigram), no pruning.
if [[ -f "$ARPA" && -s "$ARPA" ]]; then
  echo "[train] $ARPA already exists ($(du -h "$ARPA" | cut -f1)) — skip lmplz"
else
  echo "[train] lmplz -o 2 --prune 0 0 (10-30 min on 41M sentences) …"
  time "$BIN/lmplz" -o 2 --prune 0 0 < "$INPUT" > "$ARPA" 2> "$TLOG"
  echo "[train] ARPA: $(du -h "$ARPA" | cut -f1)"
fi

# 2. build_binary: convert ARPA to mmap-loadable trie binary.
if [[ -f "$BINARY" && -s "$BINARY" ]]; then
  echo "[train] $BINARY already exists ($(du -h "$BINARY" | cut -f1)) — skip build_binary"
else
  echo "[train] build_binary trie (a few minutes) …"
  time "$BIN/build_binary" trie "$ARPA" "$BINARY"
  echo "[train] binary: $(du -h "$BINARY" | cut -f1)"
fi

# 3. Smoke query: well-formed pair should be > -4, malformed pair < -8.
echo "[train] smoke queries (acceptance gate: 中国 人 > -4, 中国 嘎 < -8):"
{
  echo "==== query 1: 中国 人 (expected log_prob > -4)"
  echo "中国 人" | "$BIN/query" "$BINARY"
  echo
  echo "==== query 2: 中国 嘎 (expected log_prob < -8)"
  echo "中国 嘎" | "$BIN/query" "$BINARY"
  echo
  echo "==== query 3: 我 是 中国 人 (composed sentence)"
  echo "我 是 中国 人" | "$BIN/query" "$BINARY"
} 2>&1 | tee "$QLOG"

echo
echo "[train] done."
echo "       ARPA   $ARPA   $(du -h "$ARPA" | cut -f1)"
echo "       binary $BINARY $(du -h "$BINARY" | cut -f1)"
echo "       train  $TLOG"
echo "       query  $QLOG"
