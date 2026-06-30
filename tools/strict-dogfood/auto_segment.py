#!/usr/bin/env python3
"""auto_segment.py — read article, jieba-segment EXHAUSTIVELY, emit segments TSV.

Strategy:
- jieba.cut() default mode
- Keep all pure-CJK tokens (1-4+ char)
- Drop punctuation, latin, numerals
- Drop duplicates (each unique word once)
- Skip very-trivial single chars likely 100% PASS (no-ambiguity particles)
  — 的 / 了 / 是 / 在 / 和 / 与 / 我 / 你 / 他 / 这 / 那 / 也 / 都 / 就
  (these would pass without polish anyway)
- Generate pinyin via pypinyin
- Output: idx \\t word \\t pinyin \\t reason

Usage:
    auto_segment.py <article_path> <output_tsv>
"""

import sys
from pathlib import Path

import jieba  # type: ignore
from pypinyin import Style, lazy_pinyin  # type: ignore


SKIP_TRIVIAL_SINGLE = set("的了是在和与我你他她这那也都就把被让从对给把要会能可于和与及或还又只更很太呢吗啊吧呀啦哦嗯哎哇咦哈嘿耶都也而且但又也或还但则即各每")


def is_cjk(s: str) -> bool:
    return bool(s) and all('一' <= c <= '鿿' for c in s)


def pinyin_code(word: str) -> str:
    parts = lazy_pinyin(word, style=Style.NORMAL, errors='ignore')
    code = "".join(parts).lower()
    return "".join(c for c in code if 'a' <= c <= 'z')


def main():
    if len(sys.argv) < 3:
        print("usage: auto_segment.py <article_path> <output_tsv>")
        sys.exit(1)
    article = Path(sys.argv[1]).read_text(encoding="utf-8")
    body = "\n".join(article.split("\n")[1:])  # skip title

    seen = set()
    rows = []
    idx = 0
    for tok in jieba.cut(body):
        tok = tok.strip()
        if not tok or not is_cjk(tok):
            continue
        if len(tok) > 4:
            continue
        # Skip ultra-trivial single-char particles
        if len(tok) == 1 and tok in SKIP_TRIVIAL_SINGLE:
            continue
        if tok in seen:
            continue
        seen.add(tok)
        idx += 1
        code = pinyin_code(tok)
        if not code:
            continue
        rows.append((f"{idx:03d}", tok, code, "auto-jieba"))

    out_path = Path(sys.argv[2])
    with out_path.open("w", encoding="utf-8") as f:
        f.write("# auto-segmented exhaustively via jieba (skips ultra-trivial single chars)\n")
        f.write("# idx\tword\tpinyin\treason\n")
        for r in rows:
            f.write("\t".join(r) + "\n")
    print(f"wrote {len(rows)} segments to {out_path}")


if __name__ == "__main__":
    main()
