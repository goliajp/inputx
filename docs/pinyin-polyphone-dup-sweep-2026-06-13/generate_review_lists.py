#!/usr/bin/env python3
"""
Generate human-review lists from candidates.tsv (run find_polyphone_dups.py first).

Splits the Tier-1 candidates into:
  - SAFE  : real-word polyphones (pypinyin primary reliable) — safe_detail.md
  - RISKY : structural particles / colloquial 异读 (地/得/的/了/谁/那/哪/么) where
            pypinyin's word-level primary can be backwards — risky_detail.md

Each file lists every candidate grouped by (char, prim→wrong), freq-descending,
so the user can circle exceptions per group before any apply.
"""
import collections, os

HERE = os.path.dirname(os.path.abspath(__file__))
CAND = os.path.join(HERE, "candidates.tsv")
RISKY_CHARS = set("得地的了谁那哪么")

rows = []
for ln in open(CAND, encoding="utf-8"):
    a = ln.rstrip("\n").split("\t")
    if a[0] == "code" or len(a) < 7:
        continue
    a[2] = int(a[2])
    rows.append(a)  # code word freq primary_code misread_char prim_syl wrong_syl


def write_grouped(path, subset, title, note):
    groups = collections.defaultdict(list)
    for code, word, freq, pc, ch, ps, ws in subset:
        groups[(ch, ps, ws)].append((code, word, freq, pc))
    order = sorted(groups.items(), key=lambda x: -sum(1 for _ in x[1]))
    with open(path, "w", encoding="utf-8") as fh:
        fh.write(f"# {title}\n\n{note}\n\n")
        fh.write(f"Total: **{len(subset)}** words across **{len(groups)}** sources. "
                 "Each section = one misread source; the listed code is the WRONG "
                 "(deletion-candidate) reading, `primary` is the correct reading "
                 "kept in the dict at the same freq.\n\n")
        fh.write("To keep an exception, mark the line (or tell me the word).\n\n")
        for (ch, ps, ws), items in order:
            items.sort(key=lambda x: -x[2])
            fh.write(f"## {ch}  {ps}→{ws}  ({len(items)} words)\n\n")
            fh.write("| wrong_code | word | freq | primary |\n|---|---|---:|---|\n")
            for code, word, freq, pc in items:
                fh.write(f"| {code} | {word} | {freq} | {pc} |\n")
            fh.write("\n")
    return len(subset), len(groups)


# SAFE, freq-banded (the >=3000 band is what actually reaches the user)
safe_all = [r for r in rows if r[4] not in RISKY_CHARS]
safe_hi = [r for r in safe_all if r[2] >= 3000]
safe_lo = [r for r in safe_all if r[2] < 3000]
risky = [r for r in rows if r[4] in RISKY_CHARS]

n1, g1 = write_grouped(
    os.path.join(HERE, "safe_detail.md"), safe_hi,
    "SAFE — real-word polyphone misread copies (freq ≥ 3000)",
    "These are实词 multi-reading chars whose secondary reading was mass-copied "
    "onto words that don't use it. pypinyin primary is reliable here; deleting "
    "the wrong-reading copy never loses the word (correct reading stays at同 freq). "
    "daiban 大办 was this class.")

n2, g2 = write_grouped(
    os.path.join(HERE, "risky_detail.md"), risky,
    "RISKY — structural-particle / colloquial readings (NEEDS human call)",
    "Chars 地/得/的/了/谁/那/哪/么. pypinyin's word-level primary can point the "
    "WRONG way here (e.g. 助词「地」reads轻声 de but pypinyin gives primary …di), "
    "so the 'wrong code' column may actually be the CORRECT one. Many of these "
    "rows are also jieba sub-word noise (动词+助词 伪词 like 哀求地) that "
    "shouldn't exist at all regardless of reading — a separate '是不是真词' call.")

print(f"safe_detail.md : {n1} words / {g1} groups (freq>=3000)")
print(f"  (safe low-freq <3000 not listed: {len(safe_lo)} words — harmless, deep in lists)")
print(f"risky_detail.md: {n2} words / {g2} groups")
