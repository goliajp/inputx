#!/usr/bin/env python3
"""PHASE-SIP-sweep apply — D1 delete all library.tsv rows whose word contains
any char in the Supplementary Ideographic Plane (U+20000+, i.e. CJK
Extension B/C/D/E/F/G/H).

Rationale (2026-06-28 user "有这样标记的全部删除"):
- SIP chars have no glyph in most installed fonts → user can't see them
- 99.99% have freq=0 in our corpus → zero observed usage
- Criterion is mechanical (codepoint-based), not whitelist heuristic
- Failed test cases would surface in baseline (D1 is reversible via the
  audit trail written here + git revert)

Outputs:
  deleted-rows.tsv        — full list of removed rows (audit trail, in-repo)
  ../../../core/crates/inputx-pinyin/data/library.tsv (modified)
  ../../../tools/scoring/data/polish/corpus_garbage_filter_v1.tsv
                          (appended with all removed (code, word) pairs)
"""

from __future__ import annotations
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
LIBRARY = REPO / "core/crates/inputx-pinyin/data/library.tsv"
GARBAGE = REPO / "tools/scoring/data/polish/corpus_garbage_filter_v1.tsv"
TODAY = "2026-06-28"


def has_sip(word: str) -> bool:
    return any(ord(c) >= 0x20000 for c in word)


def main() -> int:
    lines = LIBRARY.read_text().splitlines(keepends=True)
    keep: list[str] = []
    removed: list[tuple[str, str, str]] = []  # (code, word, freq)
    for ln in lines:
        if not ln.strip() or ln.startswith("#"):
            keep.append(ln)
            continue
        parts = ln.rstrip("\n").split("\t")
        if len(parts) != 4:
            keep.append(ln)
            continue
        code, word, freq, _src = parts
        if has_sip(word):
            removed.append((code, word, freq))
        else:
            keep.append(ln)

    if not removed:
        print("[apply] no SIP rows found — nothing to do", file=sys.stderr)
        return 0

    # 1. write audit trail
    trail = HERE / "deleted-rows.tsv"
    with trail.open("w") as f:
        f.write(f"# PHASE-SIP-sweep deleted rows ({TODAY})\n")
        f.write("# criterion: word contains any codepoint >= U+20000 (SIP plane)\n")
        f.write("# format: code\\tword\\tfreq\\tcodepoints\n")
        for code, word, freq in removed:
            cps = " ".join(f"U+{ord(c):04X}" for c in word)
            f.write(f"{code}\t{word}\t{freq}\t{cps}\n")

    # 2. rewrite library
    LIBRARY.write_text("".join(keep))

    # 3. append to garbage filter
    with GARBAGE.open("a") as f:
        f.write(f"# {TODAY} PHASE-SIP-sweep — bulk D1 of {len(removed)} SIP rows (U+20000+, no font glyph + freq=0)\n")
        for code, word, _freq in removed:
            f.write(f"{code}\t{word}\t{TODAY}\t# SIP plane (no glyph)\n")

    print(f"[apply] removed {len(removed)} rows from library.tsv", file=sys.stderr)
    print(f"[apply] audit trail → {trail}", file=sys.stderr)
    print(f"[apply] garbage filter appended → {GARBAGE}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
