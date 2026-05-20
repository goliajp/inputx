#!/usr/bin/env bash
# Fetch pypinyin's phrases_dict.json — ~47k hand-curated phrase readings
# with per-character pinyin disambiguation for heteronym phrases (e.g.,
# 暖和 → [["nuǎn"],["huo"]], 着陆 → [["zhuó"],["lù"]]).
#
# Why we need this on top of the jieba × Unihan cartesian:
#   - jieba dict only gives (phrase, freq), no phrase-level pinyin
#   - cartesian product over per-char Unihan readings emits all
#     combinatorial variants, most of which are wrong for any given phrase
#     (e.g., 暖和 → nuanhe + nuanhuo, only the latter is correct).
#   - pypinyin's phrases_dict is the most-complete openly-licensed source
#     of phrase-level disambiguation we found.
#
# Source: https://github.com/mozillazg/python-pinyin (MIT license — see
# LICENSE-PYPINYIN). Copyright (c) 2016 mozillazg, 闲耘 <hotoo.cn@gmail.com>.
#
# Idempotent: skips if data/pypinyin_phrases.json already exists.

set -euo pipefail

URL="https://raw.githubusercontent.com/mozillazg/python-pinyin/master/pypinyin/phrases_dict.json"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DATA_DIR="$SCRIPT_DIR/../data"
OUT="$DATA_DIR/pypinyin_phrases.json"

mkdir -p "$DATA_DIR"

if [[ -f "$OUT" ]]; then
    echo "Already present: $OUT ($(du -h "$OUT" | cut -f1))"
    echo "Delete the file to refresh from upstream."
    exit 0
fi

echo "Downloading $URL …"
curl -sSL --max-time 120 "$URL" -o "$OUT"
SIZE=$(du -h "$OUT" | cut -f1)
SHA=$(shasum -a 256 "$OUT" | cut -d' ' -f1)
LINES=$(wc -l < "$OUT" | tr -d ' ')
echo "OK: $OUT ($LINES lines, $SIZE, sha256: $SHA)"
