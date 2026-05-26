#!/usr/bin/env python3
"""CP3d pollution filter — merged/weights.tsv → filtered/weights.tsv.

Designed-fresh replacement for the old in-place ad-hoc strip_*.py scripts.
Reads the per-source-normalised merged weights and writes a *separate*
filtered copy (never edits merged in place), so the deletion is diffable and
build_dict/gate1 can be pointed at either.

Scope: only rows with freq >= --min-freq (default 100) are filter candidates.
build_dict already drops freq < 100 (its MIN_FREQ cutoff), so deleting those
here is redundant — we leave them in filtered/ and let build_dict handle them.
This keeps the filter focused on pollution that actually reaches candidates.

Four deletion rules (user-locked 2026-05-25, "quality first"):
  ① super-long    word length >= 8         (place/org names; Viterbi composes)
  ② traditional   OpenCC t2s(word) != word (mixed-source trad leakage)
  ③ function-frag word len >= 3 with a function word (的/了/地/得/着/在/是) in a
                  NON-edge position, freq < --frag-protect (default 2000) →
                  矛盾的特(1195) deleted; idioms 搬弄是非(7626)/办得好(4409) and
                  来得及/目的地 kept (real words are higher-frequency).
  ④ surname-name  surname (excl. nickname prefixes 阿/小/老/大) + given-name/
                  title chars, freq < --protect → 丁氏/侯总 deleted;
                  鹿晗(11959)/李子(20422)/阿炳(6034 real person) protected.

A surname-LESS pure-given-name rule (to catch 佩珊) was tried and dropped: pure
given-name shapes are indistinguishable by freq or char-frequency from real
words (佩珊 4152 vs 兵书 5431 / 波峰 5127), so it deleted real words. Quality-
first: never delete real words; 佩珊-class fragments are backlog.

Why a dictionary gate not a bare freq cutoff: 榨干(3632) is rarer than
侯总(4500) yet must survive — 榨 ∉ surname ∉ name-char, so it is never a ④
candidate. surnames/name_chars come from 03b_pollution/{surnames,name_chars}.tsv
(wainshine corpus, Apache-2.0).

Run from anywhere:
  python3 tools/scoring/03b_pollution/run_all.py
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

try:
    from opencc import OpenCC
except ImportError:
    raise SystemExit(
        "pip3 install --user --break-system-packages opencc-python-reimplemented"
    )

HERE = Path(__file__).resolve().parent
SCORING = HERE.parent
MERGED = SCORING / "data/merged/weights.tsv"
FILTERED_DIR = SCORING / "data/filtered"
FILTERED = FILTERED_DIR / "weights.tsv"

FUNCTION_WORDS = set("的了地得着在是")
TITLE_CHARS = set("氏总工董事姐哥爷帅")      # surname-followed role chars (丁氏/侯总)
NICKNAME_PREFIX = set("阿小老大")            # 昵称前缀, not real surnames here

# NOTE on 佩珊: PLAN listed it as a delete target, but 佩 is not a surname —
# it's a surname-less given-name fragment. The only rule that catches it (pure
# given-name chars) cannot be made precise: 佩珊(4152) is indistinguishable by
# freq or char-frequency from real words like 兵书(5431)/波峰(5127). Deleting it
# means deleting those, so we DROP that rule (quality-first: never delete real
# words). 佩珊-class fragments are backlog. The core negative gate is 矛盾的特
# (rule ③).
EXPECT_DELETE = ["丁氏", "侯总", "矛盾的特"]
EXPECT_KEEP = ["鹿晗", "杰伦", "李子", "王牌", "榨干", "给力", "网红", "靠前", "靠谱",
               "来得及", "不得不", "目的地", "也就是说", "看得见",
               "岸壁", "昂藏", "搬弄是非", "办得好", "阿炳",
               "爱心", "安全", "安静", "爱好", "兵书", "波峰", "本章"]

RULES = ["super_long", "traditional", "function_frag", "name_surname"]


def load_counts(path: Path) -> dict[str, int]:
    out: dict[str, int] = {}
    if not path.exists():
        raise SystemExit(f"missing {path}; run build_name_chars.py first")
    for line in path.open(encoding="utf-8"):
        if line.startswith("#") or "\t" not in line:
            continue
        ch, cnt = line.rstrip("\n").split("\t")
        out[ch] = int(cnt)
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--min-freq", type=int, default=100,
                    help="only rows with freq >= this are filtered (build_dict "
                         "MIN_FREQ drops the rest); default 100")
    # Thresholds calibrated for the CP3d-cutover HYBRID freq_score distribution
    # (skews higher than per-source-log-count): 矛盾的特 3686 < frag 6000 < 办得好
    # 8220; 侯总 9831 / 丁氏 7508 < ④ 15000 < 鹿晗 26402 / 李子 25353.
    ap.add_argument("--frag-protect", type=int, default=6000,
                    help="rule ③ keeps function-frag words freq >= this (default 6000)")
    ap.add_argument("--protect", type=int, default=15000,
                    help="rule ④ keeps surname-names freq >= this (default 15000)")
    ap.add_argument("--name-char-min", type=int, default=30,
                    help="char is a given-name char (rule ④) if count >= this")
    ap.add_argument("--report-samples", type=int, default=12)
    args = ap.parse_args()

    if not MERGED.exists():
        print(f"missing {MERGED}; run `make 05-merge` first", file=sys.stderr)
        return 1

    surnames = set(load_counts(HERE / "surnames.tsv")) - NICKNAME_PREFIX
    name_count = load_counts(HERE / "name_chars.tsv")
    name_a = {c for c, n in name_count.items() if n >= args.name_char_min}
    t2s = OpenCC("t2s")
    print(f"[filter] {len(surnames)} surnames (excl nickname), "
          f"{len(name_a)} name-chars(min{args.name_char_min}); "
          f"min_freq>={args.min_freq} frag<{args.frag_protect} ④<{args.protect}",
          file=sys.stderr)

    def name_rule(word: str, freq: int) -> str | None:
        if not (2 <= len(word) <= 4):
            return None
        cs = list(word)
        if (cs[0] in surnames and all(c in name_a or c in TITLE_CHARS for c in cs[1:])
                and freq < args.protect):
            return "name_surname"
        return None

    FILTERED_DIR.mkdir(parents=True, exist_ok=True)
    counts = {k: 0 for k in RULES}
    samples: dict[str, list[str]] = {k: [] for k in RULES}
    hifreq: dict[str, list[str]] = {k: [] for k in RULES}
    deleted_words: set[str] = set()
    kept = 0

    with MERGED.open(encoding="utf-8") as fin, FILTERED.open("w", encoding="utf-8") as fout:
        for line in fin:
            if line.startswith("#"):
                fout.write(line)
                continue
            row = line.rstrip("\n").split("\t")
            if len(row) != 3:
                fout.write(line)
                continue
            word, freq = row[1], int(row[2])

            rule = None
            if freq >= args.min_freq:
                if len(word) >= 8:
                    rule = "super_long"
                elif t2s.convert(word) != word:
                    rule = "traditional"
                elif (len(word) >= 3 and any(c in FUNCTION_WORDS for c in word[1:-1])
                      and freq < args.frag_protect):
                    rule = "function_frag"
                else:
                    rule = name_rule(word, freq)

            if rule is None:
                fout.write(line)
                kept += 1
            else:
                counts[rule] += 1
                deleted_words.add(word)
                if len(samples[rule]) < args.report_samples:
                    samples[rule].append(f"{word}({freq})")
                if freq >= 4000 and len(hifreq[rule]) < args.report_samples:
                    hifreq[rule].append(f"{word}({freq})")

    total_del = sum(counts.values())
    print(f"\n[filter] kept {kept:,} rows; deleted {total_del:,} rows "
          f"(freq>={args.min_freq} only)", file=sys.stderr)
    for rule in RULES:
        print(f"  {rule:14s} {counts[rule]:>6,}  e.g. {' '.join(samples[rule])}",
              file=sys.stderr)
    print("\n[audit] deleted with freq>=4000 (real-word误删 risk):", file=sys.stderr)
    for rule in RULES:
        if hifreq[rule]:
            print(f"  {rule:14s} {' '.join(hifreq[rule])}", file=sys.stderr)

    print("\n[diag] negatives (must be DELETED):", file=sys.stderr)
    ok = True
    for w in EXPECT_DELETE:
        hit = w in deleted_words
        print(f"   {'✓' if hit else '✗ STILL PRESENT'}  {w}", file=sys.stderr)
        ok = ok and hit
    print("[diag] protected (must be KEPT):", file=sys.stderr)
    for w in EXPECT_KEEP:
        hit = w not in deleted_words
        print(f"   {'✓' if hit else '✗ WRONGLY DELETED'}  {w}", file=sys.stderr)
        ok = ok and hit

    print(f"\n[filter] wrote {FILTERED}", file=sys.stderr)
    print(f"[filter] probe check: {'ALL PASS' if ok else 'FAILURES ABOVE'}",
          file=sys.stderr)
    return 0 if ok else 2


if __name__ == "__main__":
    raise SystemExit(main())
