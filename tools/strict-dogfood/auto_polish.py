#!/usr/bin/env python3
"""auto_polish.py — from article JSON, generate polish entries for all failures.

Strategy:
- HARD multi-char → modern_vocab (freq based on char length + provincial bonus)
- HARD single-char → skip (modern_vocab can't add single chars meaningfully; need quickfix)
- SOFT (any length) → quickfix 30000

Usage: auto_polish.py <article_id_json>
Outputs:
  <stem>.modern_vocab.tsv — append to tools/scoring/data/polish/modern_vocab_v1.tsv
  <stem>.quickfix.tsv     — append to tools/scoring/data/polish/quickfix_boost.tsv
"""

import json
import sys
from pathlib import Path

PROVINCES = {'甘肃', '河南', '湖南', '福建', '广西', '陕西', '江西', '浙江', '四川',
             '湖北', '广东', '山东', '河北', '辽宁', '吉林', '黑龙江', '安徽', '云南',
             '贵州', '江苏', '青海', '海南'}


def freq_for(word: str, level: str) -> int:
    """Pick a freq based on word length + significance."""
    if level == 'HARD':
        if word in PROVINCES:
            return 50000
        if len(word) == 2:
            return 35000
        if len(word) == 3:
            return 30000
        if len(word) == 4:
            return 30000
        return 25000  # 5+ char
    return 30000  # SOFT quickfix


def main():
    if len(sys.argv) < 2:
        print("usage: auto_polish.py <article_id.json>")
        sys.exit(1)
    j = Path(sys.argv[1])
    d = json.loads(j.read_text(encoding="utf-8"))
    article_id = d['id']

    mv_rows = []   # modern_vocab entries
    qf_rows = []   # quickfix entries

    for s in d['segments']:
        if s['verdict'] == 'PASS':
            continue
        word = s['expected']
        pinyin = s['pinyin']
        verdict = s['verdict']
        if verdict == 'HARD':
            if len(word) == 1:
                # Single-char HARD usually needs quickfix (modern_vocab as word)
                qf_rows.append(f"{pinyin}\t{word}\t30000\t# strict-{article_id} HARD single")
            else:
                freq = freq_for(word, 'HARD')
                mv_rows.append(f"{pinyin}\t{word}\t{freq}")
        else:  # SOFT
            qf_rows.append(f"{pinyin}\t{word}\t30000")

    out_mv = j.with_suffix('.mv.tsv')
    out_qf = j.with_suffix('.qf.tsv')
    out_mv.write_text(
        f"\n# strict-{article_id} auto-polish ({len(mv_rows)} modern_vocab from HARD)\n"
        + "\n".join(mv_rows) + "\n",
        encoding="utf-8"
    )
    out_qf.write_text(
        f"\n# strict-{article_id} auto-polish ({len(qf_rows)} quickfix from SOFT+single-HARD)\n"
        + "\n".join(qf_rows) + "\n",
        encoding="utf-8"
    )
    print(f"wrote {len(mv_rows)} modern_vocab + {len(qf_rows)} quickfix entries")
    print(f"  {out_mv}")
    print(f"  {out_qf}")
    print()
    print("Append with:")
    print(f"  cat {out_mv} >> tools/scoring/data/polish/modern_vocab_v1.tsv")
    print(f"  cat {out_qf} >> tools/scoring/data/polish/quickfix_boost.tsv")


if __name__ == "__main__":
    main()
