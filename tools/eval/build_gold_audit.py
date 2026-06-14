#!/usr/bin/env python3
"""Stratified sampling for the gold-1000 evaluation set (CP-0.4).

Reads miu_set_paired.tsv, samples 1000 rows with stratification
70% polyphone_default / 30% non-polyphone (representative of where
pypinyin can plausibly be wrong vs trivially correct).

Outputs:
- tools/eval/gold_1000_pending.tsv   format: pinyin\thanzi\tflags\tquality_verified\taudit_note
  All rows start quality_verified=false. The LLM-audit step
  (audit_gold_1000.py + agents) fills this in.

- tools/eval/silver_full.tsv         copy of miu_set_paired.tsv,
  archived as the "silver" baseline (machine-only pinyin, no audit).

Determinism: --seed flag (default 42). Same seed + same input ->
byte-equal output.

Usage:
  tools/eval/.venv/bin/python tools/eval/build_gold_audit.py
"""

from __future__ import annotations

import argparse
import random
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
DEFAULT_IN = ROOT / "tools" / "eval" / "miu_set_paired.tsv"
DEFAULT_OUT = ROOT / "tools" / "eval" / "gold_1000_pending.tsv"
SILVER_PATH = ROOT / "tools" / "eval" / "silver_full.tsv"

TARGET = 1000
POLY_RATIO = 0.70  # 70% polyphone rows, 30% non-polyphone


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--seed", type=int, default=42)
    parser.add_argument("--target", type=int, default=TARGET)
    parser.add_argument("--in", dest="inp", default=str(DEFAULT_IN))
    parser.add_argument("--out", default=str(DEFAULT_OUT))
    args = parser.parse_args()

    src = Path(args.inp)
    if not src.exists():
        print(f"ERROR: missing {src}\n  Run CP-0.3 (pinyinize.py) first.",
              file=sys.stderr)
        return 1

    random.seed(args.seed)

    poly_rows: list = []
    nonpoly_rows: list = []
    with src.open("r", encoding="utf-8") as f:
        header = f.readline().rstrip("\n").split("\t")
        # header should be pinyin\thanzi\tflags
        for line in f:
            parts = line.rstrip("\n").split("\t")
            if len(parts) != 3:
                continue
            if parts[2] == "polyphone_default":
                poly_rows.append(parts)
            else:
                nonpoly_rows.append(parts)

    n_poly = int(args.target * POLY_RATIO)
    n_nonpoly = args.target - n_poly

    # Sanity: enough of each?
    if len(poly_rows) < n_poly:
        print(f"ERROR: only {len(poly_rows)} polyphone rows, need {n_poly}",
              file=sys.stderr)
        return 1
    if len(nonpoly_rows) < n_nonpoly:
        print(f"WARN: only {len(nonpoly_rows)} non-polyphone rows, "
              f"need {n_nonpoly} - using all of them",
              file=sys.stderr)
        n_nonpoly = len(nonpoly_rows)
        n_poly = args.target - n_nonpoly

    random.shuffle(poly_rows)
    random.shuffle(nonpoly_rows)
    chosen = poly_rows[:n_poly] + nonpoly_rows[:n_nonpoly]
    # Sort deterministically for stable output: by hanzi then pinyin
    chosen.sort(key=lambda r: (r[1], r[0]))

    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    with out.open("w", encoding="utf-8") as f:
        f.write("pinyin\thanzi\tflags\tquality_verified\taudit_note\n")
        for pinyin, hanzi, flags in chosen:
            f.write(f"{pinyin}\t{hanzi}\t{flags}\tfalse\t\n")

    print(f"wrote {len(chosen):,} rows -> {out}")
    print(f"  polyphone: {n_poly:,} ({100*n_poly/len(chosen):.1f}%)")
    print(f"  non-polyphone: {n_nonpoly:,} ({100*n_nonpoly/len(chosen):.1f}%)")

    # Silver = full paired set, archived
    if not SILVER_PATH.exists() or SILVER_PATH.stat().st_size != src.stat().st_size:
        shutil.copy(src, SILVER_PATH)
        print(f"copied silver_full.tsv ({src.stat().st_size:,} bytes)")
    else:
        print(f"silver_full.tsv already up-to-date")

    return 0


if __name__ == "__main__":
    sys.exit(main())
