#!/usr/bin/env bash
# Fetch jieba's default dictionary and process to phrases_jieba.tsv.
# Idempotent: skips if data/phrases_jieba.tsv already exists.
#
# Source: https://github.com/fxsjy/jieba (MIT license — see LICENSE-JIEBA).
# See DECISIONS.md D2 for the audit + decision rationale.

set -euo pipefail

URL="https://raw.githubusercontent.com/fxsjy/jieba/master/jieba/dict.txt"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DATA_DIR="$SCRIPT_DIR/../data"
RAW="$DATA_DIR/jieba/dict.txt"
OUT="$DATA_DIR/phrases_jieba.tsv"

mkdir -p "$DATA_DIR/jieba"

if [[ -f "$OUT" ]]; then
    echo "Already present: $OUT ($(du -h "$OUT" | cut -f1))"
    echo "Delete the file to regenerate."
    exit 0
fi

if [[ ! -f "$RAW" ]]; then
    echo "Downloading $URL …"
    curl -sSL --max-time 120 "$URL" -o "$RAW"
    SIZE=$(du -h "$RAW" | cut -f1)
    SHA=$(shasum -a 256 "$RAW" | cut -d' ' -f1)
    echo "Downloaded: $SIZE   sha256: $SHA"
fi

# Process: drop POS tag column. jieba dict.txt is space-separated:
#   "AT&T 3 nz" → "AT&T\t3"
# Skip lines without exactly 3 fields (defensive).
echo "Processing → $OUT"
awk 'NF==3 { print $1 "\t" $2 }' "$RAW" > "$OUT.tmp"
mv "$OUT.tmp" "$OUT"

LINES=$(wc -l < "$OUT" | tr -d ' ')
SIZE=$(du -h "$OUT" | cut -f1)
echo "OK: $OUT ($LINES lines, $SIZE)"
