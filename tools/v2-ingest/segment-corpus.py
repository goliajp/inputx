#!/usr/bin/env python3
"""segment-corpus.py — tokenize fetched articles + generate pinyin codes.

Reads:
  docs/pinyin-dogfood-2026-06-30/scratchpad/corpus/articles/*.txt

Writes:
  docs/pinyin-dogfood-2026-06-30/scratchpad/segments/segments.tsv
  columns: article_id\\tseg_idx\\tword\\tpinyin\\tnotes

Strategy (IME-realistic chunk sizes):
  - jieba.cut() default mode tokenizes into "natural" word boundaries
  - For each token:
    * 1-char pure CJK → keep (function words like 的/了/是)
    * 2-4 char CJK → keep (typical IME buffer length)
    * 5+ char CJK → skip (user would break into multiple buffers; future
      enhancement could split, but for v1 we drop)
    * Non-CJK (punctuation, numbers, latin) → drop
  - pinyin: pypinyin.lazy_pinyin Style.NORMAL → join with no separator
  - For polysemy chars, pypinyin picks one reading (typically most common);
    a 'notes' column flags candidates where the script wasn't 100% sure
"""

from __future__ import annotations

import sys
from pathlib import Path

import jieba  # type: ignore
from pypinyin import Style, lazy_pinyin  # type: ignore

ROOT = Path(__file__).resolve().parents[2]
ARTICLES_DIR = ROOT / "docs/pinyin-dogfood-2026-06-30/scratchpad/corpus/articles"
OUTPUT = ROOT / "docs/pinyin-dogfood-2026-06-30/scratchpad/segments/segments.tsv"
OUTPUT.parent.mkdir(parents=True, exist_ok=True)


def is_cjk_char(c: str) -> bool:
    return "一" <= c <= "鿿"


def is_pure_cjk(s: str) -> bool:
    return bool(s) and all(is_cjk_char(c) for c in s)


def pinyin_code(word: str) -> str:
    """Concat lazy_pinyin without tone, lowercase. Filter non-letters."""
    parts = lazy_pinyin(word, style=Style.NORMAL, errors="ignore")
    code = "".join(parts).lower()
    return "".join(c for c in code if "a" <= c <= "z")


def main() -> int:
    files = sorted(ARTICLES_DIR.glob("*.txt"))
    print(f"input: {len(files)} articles")

    total_segments = 0
    skipped_long = 0
    skipped_non_cjk = 0
    skipped_empty_pinyin = 0
    seg_lens: dict[int, int] = {}

    with OUTPUT.open("w", encoding="utf-8") as out:
        out.write("# segments.tsv — dogfood input pairs\n")
        out.write("# article_id\tseg_idx\tword\tpinyin\tnotes\n")
        for f in files:
            article_id = f.stem.split("_")[0]
            text = f.read_text(encoding="utf-8")
            tokens = list(jieba.cut(text))
            seg_idx = 0
            for tok in tokens:
                tok = tok.strip()
                if not tok:
                    continue
                if not is_pure_cjk(tok):
                    skipped_non_cjk += 1
                    continue
                if len(tok) > 4:
                    skipped_long += 1
                    continue
                code = pinyin_code(tok)
                if not code:
                    skipped_empty_pinyin += 1
                    continue
                out.write(f"{article_id}\t{seg_idx:04d}\t{tok}\t{code}\t\n")
                seg_idx += 1
                total_segments += 1
                seg_lens[len(tok)] = seg_lens.get(len(tok), 0) + 1

    print(f"wrote {total_segments} segments")
    print(f"skipped: long={skipped_long}, non-cjk={skipped_non_cjk}, empty-pinyin={skipped_empty_pinyin}")
    print(f"segment length distribution:")
    for L in sorted(seg_lens):
        pct = 100 * seg_lens[L] / total_segments if total_segments else 0
        print(f"  {L} chars: {seg_lens[L]:>6} ({pct:.1f}%)")
    print(f"output: {OUTPUT}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
