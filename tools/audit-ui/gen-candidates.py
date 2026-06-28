#!/usr/bin/env python3
"""Generate per-phase candidates.tsv from library.tsv for the audit UI.

Each phase is a slice of (char-length, freq-tier) of the pinyin library;
output format consumed by tools/audit-ui/index.html: tab-separated
`code\tword\tfreq\toptional-extra`.

Usage:
    python3 tools/audit-ui/gen-candidates.py <phase> [--out path.tsv]

Phases (matches docs/pinyin-library-audit-2026-06-28/PLAN.md):
    D       — freq >= 50000 (155 rows, eyeball pass)
    7plus   — char-length >= 7 (all freq tiers)
    56      — char-length 5 or 6 (all freq tiers)
    A1      — 1c f=0
    A2      — 2c f=0
    A3      — 3c f=0
    A4      — 4c f=0
    B1      — 1c f1k-10k
    B2      — 2c f1k-10k
    B3      — 3c f1k-10k
    B4      — 4c f1k-10k
    C1      — 1c f10k-50k
    C2      — 2c f10k-50k
    C3      — 3c f10k-50k
    C4      — 4c f10k-50k

Output is sorted by (-freq, code) so the densest / highest-priority rows
appear first in the audit UI.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

LIBRARY = Path(__file__).resolve().parents[2] / "core/crates/inputx-pinyin/data/library.tsv"


def char_class(word: str) -> str:
    n = len(word)  # python str length = codepoints; fine for CJK BMP
    if n == 1: return "1c"
    if n == 2: return "2c"
    if n == 3: return "3c"
    if n == 4: return "4c"
    if n <= 6: return "56"
    return "7plus"


def freq_class(f: int) -> str:
    if f == 0: return "f0"
    if f < 10000: return "f1k-10k"
    if f < 50000: return "f10k-50k"
    return "f50k+"


PHASE_RULES = {
    "D":     lambda lc, fc: fc == "f50k+",
    "7plus": lambda lc, fc: lc == "7plus",
    "56":    lambda lc, fc: lc == "56",
    "A1":    lambda lc, fc: lc == "1c" and fc == "f0",
    "A2":    lambda lc, fc: lc == "2c" and fc == "f0",
    "A3":    lambda lc, fc: lc == "3c" and fc == "f0",
    "A4":    lambda lc, fc: lc == "4c" and fc == "f0",
    "B1":    lambda lc, fc: lc == "1c" and fc == "f1k-10k",
    "B2":    lambda lc, fc: lc == "2c" and fc == "f1k-10k",
    "B3":    lambda lc, fc: lc == "3c" and fc == "f1k-10k",
    "B4":    lambda lc, fc: lc == "4c" and fc == "f1k-10k",
    "C1":    lambda lc, fc: lc == "1c" and fc == "f10k-50k",
    "C2":    lambda lc, fc: lc == "2c" and fc == "f10k-50k",
    "C3":    lambda lc, fc: lc == "3c" and fc == "f10k-50k",
    "C4":    lambda lc, fc: lc == "4c" and fc == "f10k-50k",
}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("phase", choices=sorted(PHASE_RULES.keys()))
    ap.add_argument("--out", type=Path, default=None)
    ap.add_argument("--library", type=Path, default=LIBRARY)
    args = ap.parse_args()

    rule = PHASE_RULES[args.phase]
    rows: list[tuple[str, str, int]] = []
    with args.library.open() as f:
        for ln in f:
            if not ln.strip() or ln.startswith("#"):
                continue
            parts = ln.rstrip("\n").split("\t")
            if len(parts) != 4:
                continue
            code, word, freq_s, _src = parts
            try:
                freq = int(freq_s)
            except ValueError:
                continue
            if rule(char_class(word), freq_class(freq)):
                rows.append((code, word, freq))

    # densest first (high freq before low; tiebreak code asc for determinism)
    rows.sort(key=lambda r: (-r[2], r[0]))

    out = args.out or (Path(__file__).resolve().parent / f"candidates-{args.phase}.tsv")
    with out.open("w") as f:
        f.write(f"# Phase {args.phase} — {len(rows)} candidates from library.tsv\n")
        f.write("# format: code\\tword\\tfreq\n")
        for code, word, freq in rows:
            f.write(f"{code}\t{word}\t{freq}\n")

    print(f"[gen] phase {args.phase}: {len(rows)} rows → {out}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
