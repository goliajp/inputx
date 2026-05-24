#!/usr/bin/env python3
"""Strip entries with char count >= 8 from weights.tsv.

User principle 2026-05-24: "短句是拼出来的，不应该是单个词拉长，质量
要一条条过关". Long phrases (organization names, place names, etc.)
shouldn't occupy single dict slots — Viterbi composition handles them
from their 2-3 char building blocks. Removing them:
  - Reduces FST size
  - Removes heteronym duplicate explosion
    (e.g. alabofuxingshehuidang vs alabofuxingshekuaidang)
  - Frees up multi-syllable input from being captured by long entries
    that the user rarely actually types

Threshold: >= 8 chars. Most 4-6 char common phrases retained.
"""
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
WEIGHTS = ROOT / "core/crates/inputx-pinyin/data/weights/weights.tsv"
MAX_CHAR_LEN = 7  # entries with char count > this get stripped

def main() -> int:
    if not WEIGHTS.exists():
        print(f"missing: {WEIGHTS}", file=sys.stderr)
        return 1
    backup = WEIGHTS.with_suffix(".tsv.prelong-backup")
    if not backup.exists():
        shutil.copy(WEIGHTS, backup)
        print(f"[strip-long] backup: {backup}", file=sys.stderr)
    kept = []
    stripped = 0
    total = 0
    with WEIGHTS.open() as f:
        for raw in f:
            line = raw.rstrip("\n")
            if not line or line.startswith("#"):
                kept.append(line)
                continue
            parts = line.split("\t")
            if len(parts) < 3:
                kept.append(line)
                continue
            total += 1
            if len(parts[1]) > MAX_CHAR_LEN:
                stripped += 1
                continue
            kept.append(line)
    with WEIGHTS.open("w") as f:
        for line in kept:
            f.write(line + "\n")
    print(f"[strip-long] total={total} stripped={stripped} "
          f"({100*stripped/total:.1f}%) max_char={MAX_CHAR_LEN}",
          file=sys.stderr)
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
