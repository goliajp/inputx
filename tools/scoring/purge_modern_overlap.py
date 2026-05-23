#!/usr/bin/env python3
"""Purge `pinyin_modern_v1.tsv` rows where the word already exists in
`weights.tsv` with sufficient base freq.

Problem (user-reported 2026-05-24): `lixiang` was ranking 立项 #1 and
理想 #2. Root cause traced to two compounding bugs:
  1. build_fst.rs overlay used REPLACE instead of MAX (FIXED separately).
  2. pinyin_modern_v1.tsv assigned every entry a blanket 40k-65k freq
     regardless of how common the word actually is. Even with MAX
     semantics, modern_vocab's 立项=50000 still beats base 立项=17687
     and (via the same-pinyin sort) displaces 理想=35168.

Policy: if the word already has base freq >= BASE_THRESHOLD at the
same pinyin, the modern_vocab entry is REDUNDANT and HARMFUL — the
word doesn't need a boost (corpus already covers it), and the boost
displaces same-pinyin peers. Delete the row.

Words with base freq < BASE_THRESHOLD stay in modern_vocab: those are
the genuinely under-represented modern terms (新现代用语 / 网络词 /
品牌 / IT 术语 not in 1M-sentence Wikipedia).

Run: python3 tools/scoring/purge_modern_overlap.py
"""
from __future__ import annotations
import shutil
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
WEIGHTS = ROOT / "core/crates/inputx-pinyin/data/weights/weights.tsv"
MODERN = ROOT / "tools/scoring/data/supplemental/pinyin_modern_v1.tsv"

# Words with base freq >= this are considered "well-represented in
# corpus" — modern_vocab boost is redundant for them. Empirical pick:
# the 5k mark roughly corresponds to "word appears 1×/200 sentences in
# the 1M-sent corpus" — well above noise floor.
BASE_THRESHOLD = 5000

def main() -> int:
    if not WEIGHTS.exists():
        print(f"missing: {WEIGHTS}", file=sys.stderr)
        return 1
    if not MODERN.exists():
        print(f"missing: {MODERN}", file=sys.stderr)
        return 1

    # Build (pinyin, word) → base_freq lookup.
    base: dict[tuple[str, str], int] = {}
    with WEIGHTS.open() as f:
        for raw in f:
            line = raw.rstrip("\n")
            if not line or line.startswith("#"):
                continue
            parts = line.split("\t")
            if len(parts) < 3:
                continue
            try:
                freq = int(parts[2])
            except ValueError:
                continue
            base[(parts[0], parts[1])] = freq

    backup = MODERN.with_suffix(".tsv.preoverlap-backup")
    if not backup.exists():
        shutil.copy(MODERN, backup)
        print(f"[purge-overlap] backup: {backup}", file=sys.stderr)

    kept_lines: list[str] = []
    kept = 0
    purged = 0
    purged_examples: list[tuple[str, str, int, int]] = []  # (pinyin, word, modern_freq, base_freq)
    total = 0
    with MODERN.open() as f:
        for raw in f:
            line = raw.rstrip("\n")
            if not line or line.startswith("#"):
                kept_lines.append(line)
                continue
            parts = line.split("\t")
            if len(parts) < 3:
                kept_lines.append(line)
                continue
            total += 1
            pinyin, word = parts[0], parts[1]
            try:
                modern_freq = int(parts[2].split("#")[0].strip())
            except ValueError:
                kept_lines.append(line)
                continue
            base_freq = base.get((pinyin, word), 0)
            if base_freq >= BASE_THRESHOLD:
                purged += 1
                if len(purged_examples) < 20:
                    purged_examples.append((pinyin, word, modern_freq, base_freq))
                continue
            kept_lines.append(line)
            kept += 1

    with MODERN.open("w") as f:
        for line in kept_lines:
            f.write(line + "\n")

    print(f"[purge-overlap] total={total} purged={purged} kept={kept} "
          f"(threshold base_freq >= {BASE_THRESHOLD})", file=sys.stderr)
    print(f"[purge-overlap] sample purged rows:", file=sys.stderr)
    for p, w, mf, bf in purged_examples:
        print(f"  {p:<14} {w:<8} mod={mf:>6} base={bf:>6}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
