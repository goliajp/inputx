#!/usr/bin/env python3
"""Bulk-delete Jianma2 entries that misalign with high-frequency pinyin tops.

Discovered 2026-05-24: ~47 wubi 86 Jianma2 (2-letter simcode) entries
assign a stroke-derived character that has NOTHING to do with the most
common pinyin word at the same letter sequence. Examples:
  - di → 砂 (sand)  vs pinyin top 的 (freq 65535)
  - le → 胃 (stomach) vs 了 (62402)
  - bu → 联 (associate) vs 不 (61308)
  - yi → 就 (then) vs 一 (58078)
  - da → 左 (left) vs 大 (54144)
  - mo → 嶙 (rugged) vs 默 (and 没, 模, etc.)
These cause Mixed-mode candidates to surface the misaligned wubi char
at #1 with score ~826k (LAYER_BASE), beating pinyin candidates' ~465k
(PINYIN_PHRASE_BASE 400k + freq) for every short common-pinyin input.

Per the 伙-rule (session::wubi_simcode_priority tests), the user
PROTECTS a specific set of 6 Jianma2 entries:
  wo→伙, ni→悄, ta→长, de→胡, shi→椒, you→亦
These must keep their #1 lead even under pinyin_intent.

This script deletes Jianma2 (2-letter, pinyin-shaped, single-char)
entries from jianma_simplified.txt where:
  - The code has at least one vowel (a/e/i/o/u/v) — pinyin-shaped
  - The wubi char ≠ the pinyin top at the same code
  - The pinyin top has freq ≥ FREQ_THRESHOLD (40000) — clearly common
  - The (code, char) is NOT in the protected set

Conservative: only deletes when the pinyin top is unambiguously
high-freq. Less-clear cases left for manual review (script also
emits a report).

Run: python3 tools/scoring/purge_jianma2_misaligned.py
"""
from __future__ import annotations
import shutil
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
JIANMA = ROOT / "core/crates/inputx-wubi/data/jianma_simplified.txt"
WEIGHTS = ROOT / "core/crates/inputx-pinyin/data/weights/weights.tsv"

PROTECTED: set[tuple[str, str]] = {
    # Jianma2 (2-letter) — per session::wubi_simcode_priority tests.
    ("wo", "伙"), ("ni", "悄"), ("ta", "长"), ("de", "胡"),
    # Jianma3 (3-letter).
    ("shi", "椒"), ("you", "亦"),
}

FREQ_THRESHOLD = 40000


def main() -> int:
    if not JIANMA.exists():
        print(f"missing: {JIANMA}", file=sys.stderr)
        return 1
    if not WEIGHTS.exists():
        print(f"missing: {WEIGHTS}", file=sys.stderr)
        return 1
    # Build pinyin → [(char, freq), ...] for single-char entries.
    pinyin_to_chars: dict[str, list[tuple[str, int]]] = defaultdict(list)
    with WEIGHTS.open() as f:
        for raw in f:
            line = raw.rstrip("\n")
            if not line or line.startswith("#"):
                continue
            parts = line.split("\t")
            if len(parts) < 3:
                continue
            if len(parts[1]) != 1:
                continue
            try:
                freq = int(parts[2])
            except ValueError:
                continue
            pinyin_to_chars[parts[0]].append((parts[1], freq))
    for v in pinyin_to_chars.values():
        v.sort(key=lambda x: -x[1])

    backup = JIANMA.with_suffix(".txt.prejianma2-backup")
    if not backup.exists():
        shutil.copy(JIANMA, backup)
        print(f"[purge-jianma2] backup: {backup}", file=sys.stderr)

    kept_lines: list[str] = []
    deleted: list[tuple[str, str, str, int]] = []
    with JIANMA.open() as f:
        for raw in f:
            line = raw.rstrip("\n")
            if not line or line.startswith("#"):
                kept_lines.append(line)
                continue
            parts = line.split("\t")
            if len(parts) < 2:
                kept_lines.append(line)
                continue
            code, wubi_char = parts[0], parts[1]
            # Consider Jianma2 (2-letter) and Jianma3 (3-letter)
            # single-char entries. Jianma1 (1-letter) handled separately
            # — those are the strongest user shortcuts and not audited
            # programmatically (g→一, w→伙, etc.).
            if len(code) not in (2, 3) or len(wubi_char) != 1:
                kept_lines.append(line)
                continue
            # Code must look pinyin-shaped.
            if not any(c in "aeiouv" for c in code):
                kept_lines.append(line)
                continue
            # Protected entry — keep.
            if (code, wubi_char) in PROTECTED:
                kept_lines.append(line)
                continue
            top = pinyin_to_chars.get(code)
            if not top:
                kept_lines.append(line)
                continue
            top_char, top_freq = top[0]
            if top_freq < FREQ_THRESHOLD:
                kept_lines.append(line)
                continue
            if wubi_char == top_char:
                kept_lines.append(line)
                continue
            # All conditions met → delete this Jianma2 entry.
            deleted.append((code, wubi_char, top_char, top_freq))
            # (do NOT append to kept_lines)

    if not deleted:
        print(f"[purge-jianma2] nothing to delete", file=sys.stderr)
        return 0

    with JIANMA.open("w") as f:
        for line in kept_lines:
            f.write(line + "\n")

    print(f"[purge-jianma2] deleted {len(deleted)} misaligned Jianma2 entries:",
          file=sys.stderr)
    for code, wubi_char, top_char, top_freq in deleted:
        print(f"  {code:<6} → {wubi_char}  (yielded to pinyin top {top_char} freq={top_freq})",
              file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
