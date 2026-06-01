#!/usr/bin/env python3
"""Fit per-source log-prior shifts so pinyin / wubi / nihongo
candidates land in a comparable log-prior space.

Reads each engine's weights.tsv (or equivalent freq source), computes
the natural-log freq distribution per source, finds the median, and
emits the Q4-encoded shift that would move that source's median to
the cross-source median.

Output is **suggestion only** — not applied automatically. Update
`core/crates/inputx-scoring/src/lib.rs` `LOG_PRIOR_SHIFT_Q4` to the
emitted values when ready to cut over (per [[PLAN-v1.7]] D21, the
cutover is its own commit since it changes ranking).

Run:
    python3 tools/dict-pipeline/fit_log_prior_shifts.py
"""

from __future__ import annotations

import math
import sys
from pathlib import Path

Q4 = 16  # mirror inputx_scoring::Q4

ENGINES = {
    "Wubi":     ("core/crates/inputx-wubi/data/weights/weights.tsv",   "wubi"),
    "Pinyin":   ("core/crates/inputx-pinyin/data/weights/weights.tsv", "pinyin"),
    # Japanese reads from `mozc.csv` upstream + processed jukugo/kanji
    # idf builders. Its raw_freq lives inside the build_idf paths
    # (inputx-core/src/bin/idf_from_nihongo_*.rs); placeholder until
    # we expose those freqs to TSV in v1.7.3.
    # "Japanese": ("???", "nihongo"),
}

WEIGHTS_SCHEMA = {
    # 0-indexed column for the freq number
    "pinyin": {"freq_col": 2, "comment": "#"},
    "wubi":   {"freq_col": 3, "comment": "#"},
}


def load_freqs(path: Path, schema_name: str) -> list[int]:
    schema = WEIGHTS_SCHEMA[schema_name]
    freqs = []
    with open(path, encoding="utf-8") as f:
        for line in f:
            if line.startswith(schema["comment"]):
                continue
            parts = line.rstrip("\n").split("\t")
            if len(parts) <= schema["freq_col"]:
                continue
            try:
                freqs.append(int(parts[schema["freq_col"]]))
            except ValueError:
                continue
    return freqs


def stats(freqs: list[int]) -> dict:
    """Compute log-freq distribution stats (median + IQR in log space)."""
    log_freqs = [math.log(1 + f) for f in freqs]
    log_freqs.sort()
    n = len(log_freqs)
    median = log_freqs[n // 2]
    q1 = log_freqs[n // 4]
    q3 = log_freqs[3 * n // 4]
    return {
        "n": n,
        "log_median": median,
        "log_q1": q1,
        "log_q3": q3,
        "log_iqr": q3 - q1,
    }


def main():
    repo_root = Path(__file__).resolve().parents[2]
    per_source = {}
    for name, (rel_path, schema) in ENGINES.items():
        path = repo_root / rel_path
        if not path.exists():
            print(f"WARN: {name} weights missing at {path}; skipping", file=sys.stderr)
            continue
        freqs = load_freqs(path, schema)
        per_source[name] = stats(freqs)
        print(f"{name:10s} n={per_source[name]['n']:>8,d}  "
              f"log_median={per_source[name]['log_median']:>6.2f}  "
              f"log_iqr={per_source[name]['log_iqr']:>6.2f}")

    if not per_source:
        sys.exit("ERROR: no engines loaded")

    # Cross-source median = unweighted mean of per-source log medians.
    # (Weighted by n would let pinyin dominate; unweighted balances.)
    cross_median = sum(s["log_median"] for s in per_source.values()) / len(per_source)
    print(f"\ncross_source_log_median = {cross_median:.2f}")

    # shift_q4[source] = Q4 · (cross_median - source_median)
    # Adding this shift to a source's log_prior recenters its
    # distribution on the cross-source median.
    print(f"\n=== suggested LOG_PRIOR_SHIFT_Q4 (apply by editing inputx-scoring/src/lib.rs) ===\n")
    print("const LOG_PRIOR_SHIFT_Q4: [i32; 3] = [")
    # Same order as the Rust enum: Wubi=0, Pinyin=1, Japanese=2
    enum_order = ["Wubi", "Pinyin", "Japanese"]
    for name in enum_order:
        if name in per_source:
            shift_log = cross_median - per_source[name]["log_median"]
            shift_q4 = round(shift_log * Q4)
            linear_factor = math.exp(shift_log)
            print(f"    {shift_q4:>4d}, // Source::{name} (= {enum_order.index(name)})  "
                  f"shift {shift_log:+.2f} log → ×{linear_factor:.2f} linear")
        else:
            print(f"    0, // Source::{name} (= {enum_order.index(name)})  not fit (missing data)")
    print("];")

    print(f"\nReminder: applying these shifts WILL move ranking. Run "
          f"`cargo test -p inputx-core --lib baseline` after the cutover "
          f"and update test fixtures if intended.")


if __name__ == "__main__":
    main()
