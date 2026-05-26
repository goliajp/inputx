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

# Purge policy v2 (2026-05-24): peer-ratio aware. Only purge if a
# peer at the SAME pinyin has SIGNIFICANTLY higher base freq (i.e. the
# modern overlay would unfairly displace a clearly-more-common word).
#
# Cases this distinguishes:
#   - lixiang: 立项(17687) vs 理想(35168). ratio 2.0 → purge 立项
#     (modern boost would displace clearly-better 理想).
#   - yuming: 域名(22550) vs 余名(23014). ratio 1.02 → keep 域名
#     (peer is barely ahead; modern signal is reasonable).
PEER_RATIO_THRESHOLD = 1.5  # peer must be 1.5× higher than the modern word
BASE_FLOOR = 5000           # very low-base words always keep modern boost

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
            # Find top peer at same pinyin (excluding `word` itself).
            peer_top = 0
            for (p, w), f in base.items():
                if p == pinyin and w != word and f > peer_top:
                    peer_top = f
            # Purge ONLY if BOTH conditions hold:
            #   - word's own base is above the floor (it's in the
            #     corpus, modern overlay is at least redundant)
            #   - AND a peer at same pinyin clearly dominates it
            #     (peer_top / base_freq >= PEER_RATIO_THRESHOLD)
            # Otherwise keep — modern signal is doing useful work.
            should_purge = (
                base_freq >= BASE_FLOOR
                and peer_top > 0
                and peer_top >= base_freq * PEER_RATIO_THRESHOLD
            )
            if should_purge:
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
          f"(policy: base>={BASE_FLOOR} AND peer/base>={PEER_RATIO_THRESHOLD})",
          file=sys.stderr)
    print(f"[purge-overlap] sample purged rows:", file=sys.stderr)
    for p, w, mf, bf in purged_examples:
        print(f"  {p:<14} {w:<8} mod={mf:>6} base={bf:>6}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
