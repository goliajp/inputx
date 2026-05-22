#!/usr/bin/env python3
"""Aggregate polish-log.jsonl into actionable scoring patches.

Reads `~/Library/Containers/jp.golia.inputmethod.wubi/Data/Library/Application
Support/Inputx/polish-log.jsonl` (the mac IME telemetry — every time the
user picked a candidate that wasn't #0, one line is appended) and surfaces
the repeat patterns that should drive the next data drop.

Output: one TSV report per category, plus a quickfix `.tsv` ready for
piping into the supplemental-phrases pipeline.

Usage:
    python3 aggregate_polish_log.py                    # default mac path
    python3 aggregate_polish_log.py --log path/to.jsonl
    python3 aggregate_polish_log.py --since 2026-05-20 # filter by date

Categories surfaced:
    1. **repeat-miss**: same (input, picked-word) ≥ 3 times → strong
       signal that picked-word should be #0 for that input. Outputs a
       freq-boost suggestion.
    2. **near-miss-bigger-rank**: picked-word was at rank ≥ 4 in the
       candidate list → ranking is far off, not just a 1↔2 flip.
    3. **buffer-no-match**: picked-word didn't appear in the candidates
       at all (user typed something, panel showed nothing useful,
       picked from somewhere else?) — currently can't happen unless
       PolishLog spec changes; reserved for future.

The pipeline's `06_llm_annotate` step optionally consumes the
`repeat-miss` output as a soft prior; the human reviewer pass folds
the suggestions into `data/supplemental/`.
"""

from __future__ import annotations

import argparse
import collections
import datetime as dt
import json
import sys
from pathlib import Path

DEFAULT_LOG = Path.home() / (
    "Library/Containers/jp.golia.inputmethod.wubi/Data/Library/"
    "Application Support/Inputx/polish-log.jsonl"
)

REPEAT_THRESHOLD = 3  # ≥ 3 picks of the same (input, word) = strong signal
NEAR_MISS_RANK = 4    # picked rank ≥ this = far-off ranking


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--log", type=Path, default=DEFAULT_LOG,
                   help=f"path to polish-log.jsonl (default: {DEFAULT_LOG})")
    p.add_argument("--since", type=str, default=None,
                   help="ISO date (YYYY-MM-DD); only consider entries on/after")
    p.add_argument("--out-dir", type=Path,
                   default=Path(__file__).resolve().parent.parent / "data" / "polish_reports",
                   help="where to write aggregate reports")
    return p.parse_args()


def load(log_path: Path, since: dt.datetime | None):
    if not log_path.exists():
        print(f"  no polish-log at {log_path} — nothing to aggregate")
        return []
    entries = []
    with log_path.open() as f:
        for ln, raw in enumerate(f, 1):
            raw = raw.strip()
            if not raw: continue
            try:
                e = json.loads(raw)
            except json.JSONDecodeError as err:
                print(f"  line {ln}: bad json, skipping ({err})")
                continue
            if since:
                try:
                    ts = dt.datetime.fromisoformat(e["ts"].rstrip("Z"))
                    if ts < since: continue
                except (KeyError, ValueError):
                    pass
            entries.append(e)
    return entries


def repeat_misses(entries):
    """Same (buffer, pickedWord) picked ≥ THRESHOLD times."""
    by_pair = collections.Counter()
    by_pair_ctx = collections.defaultdict(list)
    for e in entries:
        key = (e.get("buffer", ""), e.get("pickedWord", ""))
        by_pair[key] += 1
        by_pair_ctx[key].append(e)
    rows = []
    for (buf, word), n in by_pair.most_common():
        if n < REPEAT_THRESHOLD: continue
        # Compute average pickedIdx across hits — useful signal too.
        ctx = by_pair_ctx[(buf, word)]
        avg_idx = sum(c.get("pickedIdx", 0) for c in ctx) / max(1, len(ctx))
        rows.append({
            "buffer": buf,
            "preferred": word,
            "count": n,
            "avg_picked_idx": round(avg_idx, 1),
        })
    return rows


def near_miss_bigger_rank(entries):
    """Picks where pickedIdx ≥ NEAR_MISS_RANK — ranking far off."""
    rows = []
    for e in entries:
        idx = e.get("pickedIdx", 0)
        if idx >= NEAR_MISS_RANK:
            rows.append({
                "buffer": e.get("buffer", ""),
                "picked": e.get("pickedWord", ""),
                "picked_idx": idx,
                "top3": e.get("candidates", [])[:3],
            })
    return rows


def write_report(rows, out_path: Path, header_cols: list[str]):
    out_path.parent.mkdir(parents=True, exist_ok=True)
    with out_path.open("w") as f:
        f.write("\t".join(header_cols) + "\n")
        for r in rows:
            f.write("\t".join(str(r[k]) for k in header_cols) + "\n")


def write_quickfix_tsv(repeat_rows, out_path: Path):
    """Emit a TSV that can be appended (after pypinyin validation +
    dedup) to a supplemental phrase file: `pinyin<TAB>word<TAB>freq`.
    Freq starts at 30000 — moderate boost, reviewer can tune up.
    """
    out_path.parent.mkdir(parents=True, exist_ok=True)
    try:
        from pypinyin import pinyin, Style
        compute_pinyin = lambda w: "".join(
            p[0] for p in pinyin(w, style=Style.NORMAL, heteronym=False)
        )
    except ImportError:
        # Without pypinyin the operator has to fill the pinyin column
        # by hand. Mark with `???`.
        compute_pinyin = lambda w: "???"

    with out_path.open("w") as f:
        f.write("# repeat-miss-driven quickfix (review before applying)\n")
        f.write("# format: pinyin\\tword\\tfreq\\t(original_count)\n")
        for r in repeat_rows:
            buf = r["buffer"]
            word = r["preferred"]
            n = r["count"]
            # Use the buffer string verbatim as the "pinyin code" if it
            # looks pinyin-shaped; otherwise compute from word.
            # User picks via candidate panel are stored with the actual
            # typed buffer in the log — that IS the code we want.
            py = buf if buf.isascii() and buf.islower() else compute_pinyin(word)
            # Boost amount scales with repeat count: 25k base + 5k per repeat.
            freq = 25000 + 5000 * n
            f.write(f"{py}\t{word}\t{freq}\t# n={n}\n")


def main() -> int:
    a = parse_args()
    since = None
    if a.since:
        since = dt.datetime.fromisoformat(a.since)
    entries = load(a.log, since)
    print(f"[aggregate] loaded {len(entries)} entries from {a.log}")
    if not entries:
        return 0

    r_repeat = repeat_misses(entries)
    r_near = near_miss_bigger_rank(entries)

    a.out_dir.mkdir(parents=True, exist_ok=True)
    write_report(r_repeat, a.out_dir / "repeat_misses.tsv",
                 ["buffer", "preferred", "count", "avg_picked_idx"])
    write_report(r_near, a.out_dir / "near_miss_bigger_rank.tsv",
                 ["buffer", "picked", "picked_idx", "top3"])
    write_quickfix_tsv(r_repeat, a.out_dir / "quickfix_boost.tsv")

    print(f"[aggregate] wrote {len(r_repeat)} repeat-miss rows")
    print(f"[aggregate] wrote {len(r_near)} near-miss-bigger-rank rows")
    print(f"[aggregate] quickfix TSV at {a.out_dir / 'quickfix_boost.tsv'}")
    if r_repeat:
        print("\nTop repeat-miss patterns (review-then-merge):")
        for r in r_repeat[:10]:
            print(f"  {r['buffer']:>12} → {r['preferred']:<8} × {r['count']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
