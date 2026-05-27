#!/usr/bin/env python3
"""Build given-name character + surname tables from a Chinese names corpus.

CP3d (dict-pipeline pollution filter): the proper-name filter rule needs to
recognise (surname, given-name-char) pairs so it can spot person-name
fragments (丁氏 / 佩珊 / 侯总) that pollute the pinyin dict, while protecting
real high-frequency star names (鹿晗 / 杰伦).

Data source: wainshine/Chinese-Names-Corpus (Apache-2.0) — the 120-万 common
full-name list `Chinese_Names_Corpus（120W）.txt`. Each line is one full name
(surname + given name), e.g. 卢存 / 卢存国. We:
  1. count leading characters → infer the single-surname set (leading count
     >= --surname-min) plus a public-domain compound-surname (复姓) allowlist;
  2. strip the surname (longest compound match first) → given-name chars;
  3. emit per-character given-name frequencies.

Outputs (committed, small — checked into git):
  03b_pollution/surnames.tsv    surname<TAB>leading_count   (desc)
  03b_pollution/name_chars.tsv  char<TAB>given_name_count   (desc)

Cache (gitignored, ~12 MB):
  data/cache/name_corpus/chinese_names_120w.txt
    (re-)download with: build_name_chars.py --download

Deterministic + idempotent. Run from anywhere:
  python3 tools/scoring/03b_pollution/build_name_chars.py
"""
from __future__ import annotations

import argparse
import sys
import urllib.request
from collections import Counter
from pathlib import Path

HERE = Path(__file__).resolve().parent
SCORING = HERE.parent
CACHE = SCORING / "data/cache/name_corpus/chinese_names_120w.txt"
SRC_URL = (
    "https://raw.githubusercontent.com/wainshine/Chinese-Names-Corpus/master/"
    "Chinese_Names_Corpus/Chinese_Names_Corpus%EF%BC%88120W%EF%BC%89.txt"
)

# Public-domain compound surnames (复姓). Stripped before single surnames via
# longest-match so 欧阳娜娜 → given "娜娜" (not "阳娜娜"). Not exhaustive; the
# rare ones contribute negligibly to the given-name char distribution.
COMPOUND_SURNAMES = [
    "欧阳", "太史", "端木", "上官", "司马", "东方", "独孤", "南宫", "万俟",
    "闻人", "夏侯", "诸葛", "尉迟", "公羊", "赫连", "澹台", "皇甫", "宗政",
    "濮阳", "公冶", "太叔", "申屠", "公孙", "慕容", "仲孙", "钟离", "长孙",
    "宇文", "司徒", "鲜于", "司空", "闾丘", "子车", "亓官", "司寇", "巫马",
    "公西", "颛孙", "壤驷", "公良", "漆雕", "乐正", "宰父", "谷梁", "拓跋",
    "夹谷", "轩辕", "令狐", "段干", "百里", "呼延", "东郭", "南门", "羊舌",
    "微生", "梁丘", "左丘", "东门", "西门", "南郭",
]


def is_cjk(ch: str) -> bool:
    return "一" <= ch <= "鿿"


def read_names(path: Path) -> list[str]:
    names: list[str] = []
    with path.open(encoding="utf-8") as f:
        for line in f:
            s = line.strip().lstrip("﻿")
            # skip header/junk: require all-CJK and length >= 2
            if len(s) >= 2 and all(is_cjk(c) for c in s):
                names.append(s)
    return names


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--download", action="store_true",
                    help="(re-)download the names corpus into the cache")
    ap.add_argument("--surname-min", type=int, default=50,
                    help="leading-char count >= this is treated as a surname "
                         "(default 50; 114万 corpus, filters rare fragments)")
    args = ap.parse_args()

    if args.download or not CACHE.exists():
        CACHE.parent.mkdir(parents=True, exist_ok=True)
        print(f"[name-chars] downloading → {CACHE}", file=sys.stderr)
        urllib.request.urlretrieve(SRC_URL, CACHE)

    if not CACHE.exists():
        print(f"[name-chars] missing cache: {CACHE}\n"
              f"  run with --download", file=sys.stderr)
        return 1

    names = read_names(CACHE)
    print(f"[name-chars] {len(names):,} names loaded", file=sys.stderr)

    # pass 1 — infer single-surname set from leading-char frequency
    leading = Counter(n[0] for n in names)
    single_surnames = {c for c, n in leading.items() if n >= args.surname_min}
    print(f"[name-chars] {len(single_surnames):,} single surnames "
          f"(leading count >= {args.surname_min})", file=sys.stderr)

    compound = sorted(COMPOUND_SURNAMES, key=len, reverse=True)

    # pass 2 — strip surname, count given-name chars
    given_chars: Counter[str] = Counter()
    surname_count: Counter[str] = Counter()
    stripped = skipped = 0
    for n in names:
        sur = None
        for cs in compound:
            if n.startswith(cs) and len(n) > len(cs):
                sur = cs
                break
        if sur is None and n[0] in single_surnames and len(n) >= 2:
            sur = n[0]
        if sur is None:
            skipped += 1
            continue
        stripped += 1
        surname_count[sur] += 1
        for ch in n[len(sur):]:
            given_chars[ch] += 1

    print(f"[name-chars] stripped {stripped:,} / skipped {skipped:,}; "
          f"{len(given_chars):,} distinct given-name chars", file=sys.stderr)

    header = (
        "# Derived from wainshine/Chinese-Names-Corpus (Apache-2.0), the\n"
        "# Chinese_Names_Corpus（120W）.txt full-name list. Regenerate via\n"
        "# tools/scoring/03b_pollution/build_name_chars.py — DO NOT EDIT BY HAND.\n"
    )

    sur_path = HERE / "surnames.tsv"
    with sur_path.open("w", encoding="utf-8") as f:
        f.write(header)
        f.write("# surname<TAB>leading_count (compound surnames listed with count 0 if unseen)\n")
        for c, n in sorted(surname_count.items(), key=lambda kv: (-kv[1], kv[0])):
            f.write(f"{c}\t{n}\n")

    nc_path = HERE / "name_chars.tsv"
    with nc_path.open("w", encoding="utf-8") as f:
        f.write(header)
        f.write("# char<TAB>given_name_count (desc)\n")
        for c, n in sorted(given_chars.items(), key=lambda kv: (-kv[1], kv[0])):
            f.write(f"{c}\t{n}\n")

    print(f"[name-chars] wrote {sur_path.name} ({len(surname_count)} rows), "
          f"{nc_path.name} ({len(given_chars)} rows)", file=sys.stderr)

    # diagnostics
    top_sur = surname_count.most_common(15)
    top_given = given_chars.most_common(20)
    print("[diag] top surnames:", " ".join(f"{c}:{n}" for c, n in top_sur),
          file=sys.stderr)
    print("[diag] top given chars:", " ".join(f"{c}:{n}" for c, n in top_given),
          file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
