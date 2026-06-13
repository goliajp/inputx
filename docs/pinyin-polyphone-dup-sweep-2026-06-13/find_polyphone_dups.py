#!/usr/bin/env python3
"""
Polyphone-duplicate sweep — find pinyin dict entries that are the wrong-reading
copy of a word whose correct reading already exists in the dict.

Origin (user report 2026-06-13, daiban polish): the corpus-digest pipeline
mechanically duplicated whole groups of words onto a WRONG code, keyed off a
secondary/copy reading of one character. E.g. 大办 (correct: daban, 大=dà) was
also written under daiban (大 mis-read as dài). The fingerprint is exact:
the copy carries the IDENTICAL freq as the correct-reading row.

Detection (high precision,漏报-not-误报 by design):
  For each multi-char, all-CJK word W that appears under >=2 codes:
    - prim = pypinyin word-level primary reading (uses the phrase dict, so
      真·词级异读 like 会计=kuaiji / 重庆=chongqing resolve correctly).
    - het  = pypinyin per-char heteronym readings (phrase-dict disambiguated
      for known words; full char readings for unknown words/proper nouns).
    - If prim is present among W's codes (so deleting a copy never loses the
      word — the correct-reading row stays), then any OTHER code c that
        (a) can be segmented from het, AND
        (b) uses a non-primary reading on at least one char, AND
        (c) has freq IDENTICAL to the prim row's freq
      is a Tier-1 wrong-reading copy → deletion candidate.

Safety: because prim stays in the dict at the same freq, removing the Tier-1
copies costs the user nothing — the word is still typable via its correct
reading. The only residual risk is pypinyin mis-judging the primary reading of
a word it doesn't know (rare proper nouns/place names); those surface in the
by-source report for human review before any apply.

Run (needs pypinyin in a venv — PEP 668 blocks system pip):
    python3 -m venv /tmp/inputx_audit_venv
    /tmp/inputx_audit_venv/bin/pip install pypinyin
    cd <repo-root>
    /tmp/inputx_audit_venv/bin/python docs/pinyin-polyphone-dup-sweep-2026-06-13/find_polyphone_dups.py

Outputs (written next to this script):
    candidates.tsv  — one deletion candidate per line:
        code  word  freq  primary_code  misread_char  prim_syl  wrong_syl
    by_source.md    — grouped by (char, prim→wrong) for human review.
"""
import collections, functools, os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", ".."))
LIB = os.path.join(REPO, "core/crates/inputx-pinyin/data/library.tsv")

try:
    from pypinyin import pinyin, lazy_pinyin, Style
except ImportError:
    sys.exit("pypinyin missing — create the venv per this script's docstring.")


def load():
    word_codes = collections.defaultdict(dict)  # word -> {code: freq}
    for ln in open(LIB, encoding="utf-8"):
        a = ln.rstrip("\n").split("\t")
        if len(a) < 3:
            continue
        code, word, freq = a[0], a[1], a[2]
        try:
            freq = int(freq)
        except ValueError:
            continue
        if len(word) >= 2 and all("一" <= ch <= "鿿" for ch in word):
            word_codes[word][code] = max(word_codes[word].get(code, 0), freq)
    return word_codes


@functools.lru_cache(maxsize=None)
def het(word):
    return tuple(tuple(x) for x in pinyin(word, style=Style.NORMAL, heteronym=True))


def segment(code, h):
    """Split code into len(h) syllables, each drawn from h[i]. Return tuple|None."""
    n = len(h)

    def go(i, pos):
        if i == n:
            return () if pos == len(code) else None
        for syl in h[i]:
            if code.startswith(syl, pos):
                rest = go(i + 1, pos + len(syl))
                if rest is not None:
                    return (syl,) + rest
        return None

    return go(0, 0)


def main():
    word_codes = load()
    candidates = []  # (code, word, freq, prim_code, char, prim_syl, wrong_syl)
    groups = collections.defaultdict(lambda: {"n": 0, "freq": 0, "ex": []})

    for word, codes in word_codes.items():
        if len(codes) < 2:
            continue
        h = het(word)
        if len(h) != len(word):
            continue
        prim = "".join(x[0] for x in h)
        if prim not in codes:
            continue
        pf = codes[prim]
        for c, f in codes.items():
            if c == prim or f != pf:
                continue
            seg = segment(c, h)
            if seg is None:
                continue
            mis = [i for i in range(len(word)) if seg[i] != h[i][0]]
            if not mis:
                continue
            i = mis[0]
            candidates.append((c, word, f, prim, word[i], h[i][0], seg[i]))
            k = (word[i], h[i][0], seg[i])
            g = groups[k]
            g["n"] += 1
            g["freq"] += f
            if len(g["ex"]) < 6:
                g["ex"].append(f"{c}={word}")

    candidates.sort(key=lambda x: -x[2])
    with open(os.path.join(HERE, "candidates.tsv"), "w", encoding="utf-8") as fh:
        fh.write("code\tword\tfreq\tprimary_code\tmisread_char\tprim_syl\twrong_syl\n")
        for row in candidates:
            fh.write("\t".join(str(x) for x in row) + "\n")

    with open(os.path.join(HERE, "by_source.md"), "w", encoding="utf-8") as fh:
        fh.write("# Polyphone-duplicate misread sources\n\n")
        fh.write(f"Total Tier-1 deletion candidates: **{len(candidates)}** words, "
                 f"across **{len(groups)}** (char, prim→wrong) sources.\n\n")
        fh.write("| char | prim→wrong | words | sum_freq | examples |\n")
        fh.write("|------|-----------|------:|---------:|----------|\n")
        for (ch, pr, wr), g in sorted(groups.items(), key=lambda x: -x[1]["n"]):
            ex = ", ".join(g["ex"])
            fh.write(f"| {ch} | {pr}→{wr} | {g['n']} | {g['freq']} | {ex} |\n")

    print(f"candidates: {len(candidates)} words, {len(groups)} sources")
    print(f"wrote {os.path.join(HERE, 'candidates.tsv')}")
    print(f"wrote {os.path.join(HERE, 'by_source.md')}")


if __name__ == "__main__":
    main()
