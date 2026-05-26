#!/usr/bin/env python3
"""Import multi-kanji jukugo from mozc dictionary_oss (BSD-3) into a TSV that
`build_jukugo_rs.py` consumes.

Source: tools/jp/.mozc_cache/dictionary0[0-9].txt (gitignored, ~59 MB; fetch
via the curl loop in the C2.1 task / this file's docstring). Each mozc line is
    reading(hiragana) \t lid \t rid \t cost \t word(kanji+)
Lower cost = more common.

Pipeline:
  1. Filter to 2-6 char PURE-kanji words (real noun jukugo; excludes okurigana
     like 食べる and compounds with kana like 東京都生まれ) under COST_MAX.
  2. Convert the hiragana reading to Inputx romaji (Hepburn canonical, matching
     core/crates/inputx-jp/src/romaji.rs so the user's typed romaji hits it).
  3. cost -> freq (0-100, lower cost = higher freq).
  4. Dedup by (romaji, word) keeping the lowest cost (highest freq).
  5. Cross-check the converter against the hand-curated jp_jukugo_v1.tsv: for
     words present in BOTH, our romaji must match the hand-written one. Prints
     the match rate as a correctness gate.

Output: tools/scoring/data/supplemental/jp_jukugo_mozc_v1.tsv
        (word \t romaji \t freq) — same shape as jp_jukugo_v1.tsv.
Re-run build_jukugo_rs.py afterward to regenerate jukugo.rs.
"""
from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
CACHE = Path(__file__).resolve().parent / ".mozc_cache"
SUPP = ROOT / "tools/scoring/data/supplemental"
EXISTING = SUPP / "jp_jukugo_v1.tsv"
OUT = SUPP / "jp_jukugo_mozc_v1.tsv"

# Scale control (user: "控规模导入"). Only words whose mozc cost is below this
# import. Lower = tighter/smaller. Tune against the printed size summary.
COST_MAX = 5000

# --- hiragana -> romaji (canonical Hepburn, mirrors romaji.rs TABLE) ---------
# 拗音 digraphs (2-hira) first for greedy match, then basic mora.
YOUON = {
    "きゃ": "kya", "きゅ": "kyu", "きょ": "kyo",
    "ぎゃ": "gya", "ぎゅ": "gyu", "ぎょ": "gyo",
    "しゃ": "sha", "しゅ": "shu", "しょ": "sho",
    "ちゃ": "cha", "ちゅ": "chu", "ちょ": "cho",
    "じゃ": "ja",  "じゅ": "ju",  "じょ": "jo",
    "ぢゃ": "ja",  "ぢゅ": "ju",  "ぢょ": "jo",
    "にゃ": "nya", "にゅ": "nyu", "にょ": "nyo",
    "ひゃ": "hya", "ひゅ": "hyu", "ひょ": "hyo",
    "びゃ": "bya", "びゅ": "byu", "びょ": "byo",
    "ぴゃ": "pya", "ぴゅ": "pyu", "ぴょ": "pyo",
    "みゃ": "mya", "みゅ": "myu", "みょ": "myo",
    "りゃ": "rya", "りゅ": "ryu", "りょ": "ryo",
}
BASIC = {
    "あ": "a", "い": "i", "う": "u", "え": "e", "お": "o",
    "か": "ka", "き": "ki", "く": "ku", "け": "ke", "こ": "ko",
    "が": "ga", "ぎ": "gi", "ぐ": "gu", "げ": "ge", "ご": "go",
    "さ": "sa", "し": "shi", "す": "su", "せ": "se", "そ": "so",
    "ざ": "za", "じ": "ji", "ず": "zu", "ぜ": "ze", "ぞ": "zo",
    "た": "ta", "ち": "chi", "つ": "tsu", "て": "te", "と": "to",
    "だ": "da", "ぢ": "di", "づ": "du", "で": "de", "ど": "do",
    "な": "na", "に": "ni", "ぬ": "nu", "ね": "ne", "の": "no",
    "は": "ha", "ひ": "hi", "ふ": "fu", "へ": "he", "ほ": "ho",
    "ば": "ba", "び": "bi", "ぶ": "bu", "べ": "be", "ぼ": "bo",
    "ぱ": "pa", "ぴ": "pi", "ぷ": "pu", "ぺ": "pe", "ぽ": "po",
    "ま": "ma", "み": "mi", "む": "mu", "め": "me", "も": "mo",
    "や": "ya", "ゆ": "yu", "よ": "yo",
    "ら": "ra", "り": "ri", "る": "ru", "れ": "re", "ろ": "ro",
    "わ": "wa", "を": "wo", "ゐ": "i", "ゑ": "e",
}
VOWEL_KANA = set("あいうえおやゆよ")  # after ん -> use "nn" to disambiguate


def hira_to_romaji(s: str) -> str | None:
    """Return Inputx romaji, or None if the reading has a char we can't map
    (small kana fragments, chōonpu, etc.) — unmappable readings are dropped
    rather than guessed."""
    out: list[str] = []
    geminate = False
    i = 0
    n = len(s)
    while i < n:
        ch = s[i]
        pair = s[i:i + 2]
        if ch == "っ":
            geminate = True
            i += 1
            continue
        if ch == "ん":
            nxt = s[i + 1] if i + 1 < n else ""
            # Word-final ん and ん-before-consonant → single "n" (hoken→ほけん,
            # shinjuku→しんじゅく — matches how users type and the hand TSV).
            # Only ん-before-vowel/や行 needs "nn" to disambiguate (kinnen→
            # きんえん, else kinen→きねん).
            out.append("nn" if (nxt in VOWEL_KANA or nxt == "ん") else "n")
            i += 1
            continue
        if ch == "ー":  # long-vowel mark — repeat previous vowel if any
            if out and out[-1] and out[-1][-1] in "aiueo":
                out.append(out[-1][-1])
            i += 1
            continue
        if pair in YOUON:
            r = YOUON[pair]
            i += 2
        elif ch in BASIC:
            r = BASIC[ch]
            i += 1
        else:
            return None  # unmappable -> drop the entry
        if geminate:
            r = r[0] + r  # double the leading consonant (がっこう -> gakkou)
            geminate = False
        out.append(r)
    if geminate:  # trailing っ — abnormal, drop
        return None
    return "".join(out)


def is_pure_kanji(w: str) -> bool:
    return all("一" <= c <= "鿿" for c in w)


def cost_to_freq(cost: int) -> int:
    # Lower cost = more common = higher freq. Linear map; clamp 1..100.
    f = round((6000 - cost) / 55)
    return max(1, min(100, f))


def main() -> int:
    files = sorted(CACHE.glob("dictionary0[0-9].txt"))
    if not files:
        print(f"ERROR: no mozc files in {CACHE} — fetch dictionary0[0-9].txt first")
        return 1

    # (romaji, word) -> lowest cost seen
    best: dict[tuple[str, str], int] = {}
    seen_lines = dropped_unmappable = 0
    for fp in files:
        with fp.open(encoding="utf-8") as fh:
            for line in fh:
                p = line.rstrip("\n").split("\t")
                if len(p) < 5:
                    continue
                reading, cost_s, word = p[0], p[3], p[4]
                if not (2 <= len(word) <= 6) or not is_pure_kanji(word):
                    continue
                try:
                    cost = int(cost_s)
                except ValueError:
                    continue
                if cost >= COST_MAX:
                    continue
                seen_lines += 1
                rom = hira_to_romaji(reading)
                if rom is None:
                    dropped_unmappable += 1
                    continue
                key = (rom, word)
                if key not in best or cost < best[key]:
                    best[key] = cost

    # --- converter self-check vs hand-curated TSV --------------------------
    hand: dict[str, str] = {}  # word -> hand romaji
    if EXISTING.exists():
        with EXISTING.open(encoding="utf-8") as fh:
            for line in fh:
                q = line.rstrip("\n").split("\t")
                if len(q) >= 2:
                    hand.setdefault(q[0], q[1])
    checked = matched = 0
    mismatches: list[str] = []
    for (rom, word), _ in best.items():
        if word in hand:
            checked += 1
            if hand[word] == rom:
                matched += 1
            elif len(mismatches) < 15:
                mismatches.append(f"{word}: mozc={rom} hand={hand[word]}")

    # --- write output ------------------------------------------------------
    rows = sorted(((w, r, cost_to_freq(c)) for (r, w), c in best.items()),
                  key=lambda t: (-t[2], t[0], t[1]))
    with OUT.open("w", encoding="utf-8") as fh:
        fh.write("# Generated by tools/jp/build_mozc_jukugo.py from mozc "
                 "dictionary_oss (BSD-3). word\\tromaji\\tfreq. DO NOT hand-edit.\n")
        for w, r, f in rows:
            fh.write(f"{w}\t{r}\t{f}\n")

    print(f"mozc lines passing kanji+cost<{COST_MAX} filter: {seen_lines}")
    print(f"dropped (unmappable reading): {dropped_unmappable}")
    print(f"unique (romaji,word) rows written: {len(rows)} -> {OUT.name}")
    print(f"est. jukugo.rs growth: ~{len(rows)*38//1024} KB")
    rate = (matched / checked * 100) if checked else 0.0
    print(f"converter self-check vs hand TSV: {matched}/{checked} match ({rate:.1f}%)")
    if mismatches:
        print("sample mismatches (mozc reading variant vs hand canonical):")
        for m in mismatches:
            print("   ", m)
    return 0


if __name__ == "__main__":
    sys.exit(main())
