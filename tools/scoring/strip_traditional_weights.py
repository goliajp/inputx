#!/usr/bin/env python3
"""Strip traditional-only rows from `weights.tsv` IN PLACE.

Problem (user-reported 2026-05-24): pinyin.fst was returning traditional
variants (於 / 國 / 來 / 來 / ...) alongside simplified for the same
pinyin. Example: `yu` → both `于` (49010) and `於` (49376) at near-
equal freq, and the FST happened to surface `於` first.

Root cause: the upstream weights builder pulled from multiple sources
(Unihan readings + phrase dicts + corpus) without a unified t2s pass.
Mixed-content sources (zh.wikipedia, Unihan kMandarin which includes
trad chars) inject traditional rows.

Fix policy:
  - For each row (pinyin, word, freq), convert `word` through OpenCC
    `t2s`. If `simplified != word`, the row is dropped (word had at
    least one traditional-only char).
  - We DO NOT collapse-and-sum freq into the simplified row — the
    simplified row already exists with its own corpus-derived freq;
    merging would over-credit it.
  - 100% deterministic, idempotent.

This is the minimum-correct-fix for v1.4. The proper long-term fix is
to push t2s INSIDE build_weights at the corpus-read stage, but that
needs a Rust-side OpenCC binding (none of the popular crates are
mature). Post-processing is good enough until then.

Run: python3 tools/scoring/strip_traditional_weights.py
"""
from __future__ import annotations
import shutil
import sys
from pathlib import Path

try:
    from opencc import OpenCC
except ImportError:
    raise SystemExit(
        "pip3 install --user --break-system-packages opencc-python-reimplemented"
    )

ROOT = Path(__file__).resolve().parent.parent.parent
WEIGHTS = ROOT / "core/crates/inputx-pinyin/data/weights/weights.tsv"

def main() -> int:
    if not WEIGHTS.exists():
        print(f"missing: {WEIGHTS}", file=sys.stderr)
        return 1
    t2s = OpenCC("t2s")
    backup = WEIGHTS.with_suffix(".tsv.pretrad-backup")
    if not backup.exists():
        shutil.copy(WEIGHTS, backup)
        print(f"[strip-trad] backup: {backup}", file=sys.stderr)

    kept: list[str] = []
    dropped = 0
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
            pinyin, word = parts[0], parts[1]
            simplified = t2s.convert(word)
            if simplified != word:
                dropped += 1
                continue
            kept.append(line)

    with WEIGHTS.open("w") as f:
        for line in kept:
            f.write(line + "\n")

    print(f"[strip-trad] total={total} dropped={dropped} kept={total - dropped} "
          f"({dropped / max(total, 1):.1%})", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
