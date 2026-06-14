#!/usr/bin/env python3
"""Pinyin back-translation pairing for the MIU evaluation set.

For each MIU (汉字) in `miu_set.tsv`, generate the corresponding pinyin
sequence using pypinyin's lazy_pinyin with Style.NORMAL (no tones).

For polyphone characters (字符有多个读音), pypinyin defaults to its
primary reading. We mark those rows with `polyphone_default` flag so
they can be manually verified in CP-0.4 (human-audit gold-1000 set).

Output:
- tools/eval/miu_set_paired.tsv   format: pinyin\thanzi\tflags

Requires: pypinyin (see tools/eval/requirements.txt + venv)

Usage:
  tools/eval/.venv/bin/python tools/eval/pinyinize.py
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

try:
    from pypinyin import lazy_pinyin, pinyin, Style
except ImportError:
    print("ERROR: pypinyin not installed.\n"
          "  Run: tools/eval/.venv/bin/pip install pypinyin\n"
          "  Or:  python3 -m venv tools/eval/.venv && "
          "tools/eval/.venv/bin/pip install -r tools/eval/requirements.txt",
          file=sys.stderr)
    sys.exit(1)

ROOT = Path(__file__).resolve().parent.parent.parent
DEFAULT_IN = ROOT / "tools" / "eval" / "miu_set.tsv"
DEFAULT_OUT = ROOT / "tools" / "eval" / "miu_set_paired.tsv"

# Polyphone detection: a hanzi char is polyphone iff pypinyin returns
# more than one reading for it (heteronym=True surfaces all readings).
_polyphone_cache: dict = {}


def is_polyphone(ch: str) -> bool:
    if ch in _polyphone_cache:
        return _polyphone_cache[ch]
    readings = pinyin(ch, style=Style.NORMAL, heteronym=True)
    # readings is list-of-list; readings[0] = all readings for ch[0]
    poly = len(readings[0]) > 1 if readings else False
    _polyphone_cache[ch] = poly
    return poly


def miu_flags(miu: str) -> str:
    """Return flags string for a MIU. Currently single flag set:
    `polyphone_default` if any char in the MIU is polyphone.
    Empty string means no flags.
    """
    if any(is_polyphone(c) for c in miu):
        return "polyphone_default"
    return ""


def pinyinize_miu(miu: str) -> str:
    """Return space-separated default pinyin syllables."""
    return " ".join(lazy_pinyin(miu, style=Style.NORMAL))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--in", dest="inp", default=str(DEFAULT_IN))
    parser.add_argument("--out", default=str(DEFAULT_OUT))
    args = parser.parse_args()

    src = Path(args.inp)
    if not src.exists():
        print(f"ERROR: missing {src}\n  Run CP-0.2 (build_miu_set.py) first.",
              file=sys.stderr)
        return 1

    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)

    n_total = 0
    n_poly = 0
    n_failed = 0
    with src.open("r", encoding="utf-8") as fin, out.open("w", encoding="utf-8") as fout:
        # Header
        fout.write("pinyin\thanzi\tflags\n")
        for line in fin:
            miu = line.rstrip("\n")
            if not miu:
                continue
            n_total += 1
            try:
                py = pinyinize_miu(miu)
            except Exception as e:
                n_failed += 1
                continue
            flags = miu_flags(miu)
            if flags:
                n_poly += 1
            fout.write(f"{py}\t{miu}\t{flags}\n")

    print(f"wrote {n_total - n_failed:,} pairs -> {out}")
    print(f"  with polyphone_default flag: {n_poly:,} ({100*n_poly/max(n_total,1):.1f}%)")
    if n_failed:
        print(f"  WARN: {n_failed} rows failed pinyin conversion", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
