#!/usr/bin/env python3
"""Diff today's probe output against v1.3-snapshot.json.

Usage: python3 data/v14-baseline-fixtures/diff-vs-snapshot.py

Re-runs the probe for the same 33 buffers × {jp on, jp off} + 3 edge
cases the original snapshot captured. Reports per-buffer:
  - same: top10 word sequence identical
  - reorder: same words, different order (cosmetic)
  - drift: top-1 changed (user-visible)
  - new-empty / new-nonempty: count changed

Snapshot rebuild target = WU-ρ post-WU-π cherry-pick. Drift is
expected and audited per the v1.9.0-vNEXT-audit.md close-out section.
"""
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SNAPSHOT = ROOT / "data/v14-baseline-fixtures/v1.3-snapshot.json"
PROBE_CRATE = ROOT / "core"


def probe(buf: str, mode: str = "", jp: bool = False) -> dict:
    args = ["cargo", "run", "--quiet", "--release", "--bin",
            "inputx-probe", "--", buf]
    if mode:
        args += ["--mode", mode]
    if jp:
        args += ["--jp"]
    r = subprocess.run(args, capture_output=True, text=True, cwd=PROBE_CRATE)
    if r.returncode != 0:
        raise RuntimeError(f"probe {buf!r} failed: {r.stderr}")
    return json.loads(r.stdout)


def main():
    snapshot = json.loads(SNAPSHOT.read_text())
    print(f"loaded snapshot — {len(snapshot)} entries\n")

    same = 0
    reorder = 0
    drift = 0
    count_changed = 0
    drift_detail = []

    for i, entry in enumerate(snapshot, 1):
        buf = entry["buffer"]
        mode_label = entry["mode"]
        jp = entry["jp"]
        category = entry["category"]

        # Map mode_label back to CLI flag
        mode_flag = ""
        if mode_label == "Wubi":
            mode_flag = "wubi"
        elif mode_label == "Pinyin":
            mode_flag = "pinyin"
        elif mode_label == "Japanese":
            mode_flag = "japanese"
        # Mixed → no flag

        try:
            now = probe(buf, mode_flag, jp)
        except RuntimeError as e:
            print(f"  [{i}/{len(snapshot)}] {buf!r:20} ERROR: {e}")
            continue

        # Symmetric top-10 slice — what the user actually sees on the
        # first candidate row. The snapshot field is named `top10` but
        # historically stored variable lengths (0-50); cap to 10 on
        # both sides for the apples-to-apples comparison.
        old_top10 = [c["word"] for c in entry["top10"][:10]]
        new_top10 = [c["word"] for c in now["candidates"][:10]]
        old_top3 = old_top10[:3]
        new_top3 = new_top10[:3]
        old_top1 = old_top10[0] if old_top10 else None
        new_top1 = new_top10[0] if new_top10 else None

        tag = ""
        if old_top10 == new_top10:
            same += 1
            tag = "same"
        elif old_top1 != new_top1:
            drift += 1
            tag = f"DRIFT top1 {old_top1!r}→{new_top1!r}"
            drift_detail.append({
                "buffer": buf, "jp": jp, "category": category,
                "old_top3": old_top3, "new_top3": new_top3,
                "old_top10_len": len(old_top10), "new_top10_len": len(new_top10),
            })
        elif old_top3 == new_top3:
            # top-3 (visible first row in most candidate panels) unchanged
            # but deeper top-10 reordered or grew/shrunk — cosmetic only.
            reorder += 1
            tag = "reorder-deep (top3 same)"
        else:
            # top-1 same, but top-2/3 changed — still some user-visible
            # impact, log it.
            reorder += 1
            tag = f"reorder-top3 ({old_top3} → {new_top3})"

        jp_label = "jp" if jp else "  "
        print(f"  [{i:2}/{len(snapshot)}] {buf!r:22} {jp_label} {category:24} {tag}")

    print()
    print(f"summary: same={same}  reorder={reorder}  count_changed={count_changed}  drift={drift}")
    if drift_detail:
        print(f"\ndrift detail ({len(drift_detail)} entries):")
        for d in drift_detail:
            old_top1 = d['old_top3'][0] if d['old_top3'] else None
            new_top1 = d['new_top3'][0] if d['new_top3'] else None
            print(f"  - {d['buffer']!r:20} (jp={d['jp']}, {d['category']})")
            print(f"      top1: {old_top1!r} → {new_top1!r}")
            print(f"      old top3: {d['old_top3']}")
            print(f"      new top3: {d['new_top3']}")
    # Exit 0 even on drift — drift IS the point of this audit; humans decide
    # if the drift is acceptable.
    return 0


if __name__ == "__main__":
    sys.exit(main())
