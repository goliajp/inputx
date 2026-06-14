#!/usr/bin/env python3
"""Merge LLM-audit verdicts into gold-1000 set (CP-0.4).

Reads:
- tools/eval/gold_1000_pending.tsv (1000 unverified rows + header)
- /tmp/inputx_gold_audit/verdict_{0..9}.tsv (100 rows each, idx 1..1000)

Writes:
- tools/eval/gold_1000.tsv  same shape, with:
    - quality_verified set to true (correct + corrected) or false (ambiguous)
    - pinyin column overwritten on corrected rows
    - audit_note records LLM rationale

This is a one-shot merge. The verdict files are ephemeral (in /tmp);
the result tsv is the committed artifact.

Usage:
  tools/eval/.venv/bin/python tools/eval/merge_gold_verdict.py
"""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
PENDING = ROOT / "tools" / "eval" / "gold_1000_pending.tsv"
OUT = ROOT / "tools" / "eval" / "gold_1000.tsv"
VERDICT_DIR = Path("/tmp/inputx_gold_audit")


def main() -> int:
    if not PENDING.exists():
        print(f"ERROR: missing {PENDING}", file=sys.stderr)
        return 1

    # Read pending — list-of-list, 1000 rows, schema:
    # [pinyin, hanzi, flags, quality_verified, audit_note]
    with PENDING.open("r", encoding="utf-8") as f:
        header = f.readline().rstrip("\n").split("\t")
        rows = [line.rstrip("\n").split("\t") for line in f]
    assert len(rows) == 1000, f"expected 1000 rows, got {len(rows)}"

    # Read verdicts — 10 files × 100 rows. Match by row order, not the
    # idx column: 7/10 agents re-numbered 1-100 within their chunk (prompt
    # was ambiguous), so the idx column is unreliable. Row order is
    # preserved by all agents, so verdict[N] line K corresponds to
    # chunk[N] line K, i.e. global idx = N*100 + K (1-based).
    verdicts: dict = {}
    for chunk in range(10):
        path = VERDICT_DIR / f"verdict_{chunk}.tsv"
        if not path.exists():
            print(f"ERROR: missing {path}", file=sys.stderr)
            return 1
        with path.open("r", encoding="utf-8") as f:
            lines = [line.rstrip("\n") for line in f if line.rstrip("\n")]
        if len(lines) != 100:
            print(f"WARN: verdict_{chunk} has {len(lines)} lines, expected 100",
                  file=sys.stderr)
        for k, raw in enumerate(lines):
            parts = raw.split("\t")
            if len(parts) < 2:
                continue
            # NF may be 2 (idx, verdict) or 4 (idx, verdict, correct_pinyin, note)
            # idx column is ignored — k+1 + chunk*100 is the global idx
            global_idx = chunk * 100 + k + 1
            verdict = parts[1]
            correct_pinyin = parts[2] if len(parts) > 2 else ""
            note = parts[3] if len(parts) > 3 else ""
            verdicts[global_idx] = (verdict, correct_pinyin, note)

    assert len(verdicts) == 1000, f"expected 1000 verdicts, got {len(verdicts)}"

    # Merge
    counts = {"correct": 0, "corrected": 0, "ambiguous": 0, "unknown": 0}
    for i, row in enumerate(rows):
        idx = i + 1  # 1-based
        verdict, correct_py, note = verdicts.get(idx, ("unknown", "", ""))
        if verdict == "correct":
            row[3] = "true"
            row[4] = ""
            counts["correct"] += 1
        elif verdict == "corrected":
            # pypinyin was wrong → overwrite pinyin
            old_py = row[0]
            row[0] = correct_py if correct_py else old_py
            row[3] = "true"
            row[4] = f"LLM-corrected: {note}" if note else "LLM-corrected"
            counts["corrected"] += 1
        elif verdict == "ambiguous":
            row[3] = "false"
            row[4] = f"LLM-ambiguous: {note}" if note else "LLM-ambiguous"
            counts["ambiguous"] += 1
        else:
            row[3] = "false"
            row[4] = f"unknown verdict: {verdict}"
            counts["unknown"] += 1

    # Write
    with OUT.open("w", encoding="utf-8") as f:
        f.write("\t".join(header) + "\n")
        for row in rows:
            f.write("\t".join(row) + "\n")

    print(f"wrote {len(rows):,} rows -> {OUT}")
    for k, v in counts.items():
        print(f"  {k}: {v:,}")
    verified = counts["correct"] + counts["corrected"]
    print(f"  quality_verified=true: {verified:,} ({100*verified/len(rows):.1f}%)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
