#!/usr/bin/env python3
"""Diff a fresh corpus-harvest TSV against the shipped dict weights.

Two TSV inputs:
1. **harvest** (from tools/corpus-harvest/output/<source>/<date>.tsv):
   `word<TAB>count<TAB>source<TAB>fetched_at` (header row + sorted data)
2. **weights** (from core/crates/inputx-pinyin/data/weights/weights.tsv
   or inputx-wubi/data/weights/weights.tsv): comment lines + tab-
   separated rows, schema differs per engine. Caller specifies which
   columns are key + value.

Output (in --out dir, one TSV each):
- `new_entries.tsv` — words in harvest but absent from weights.
  Top candidates for adding to dict (with adequate freq).
- `dropped_candidates.tsv` — words in weights but absent from harvest.
  *INFORMATIONAL ONLY* — we do NOT drop these. Many are valid words
  rare on Wikipedia but common elsewhere (slang, brand names, ...).
  This report flags entries to potentially re-verify.
- `freq_changes.tsv` — words in both; lists where the relative
  ranking differs by more than a threshold. Useful for spotting
  frequency drift between bundled dict and fresh corpus.

Determinism: pure Python stdlib, sorted output, no hash randomization
issues (uses Counter + sorted tuples). Same inputs → identical outputs.

Run:
    python3 tools/dict-pipeline/diff_corpus.py \\
        --harvest tools/corpus-harvest/output/zh-wikipedia/20260501.tsv \\
        --weights core/crates/inputx-pinyin/data/weights/weights.tsv \\
        --weights-schema pinyin \\
        --out tools/dict-pipeline/output/zh-wiki-vs-pinyin/
"""

from __future__ import annotations

import argparse
import sys
from collections import defaultdict
from pathlib import Path

# Per-engine weights.tsv schema. Tells us which column is the word and
# which is the frequency. The "key index" + "value index" are 0-based
# column positions after splitting by tab.
WEIGHTS_SCHEMA = {
    # pinyin weights: # pinyin\tword\tfreq_score
    "pinyin": {"key_col": 1, "value_col": 2, "comment_prefix": "#"},
    # wubi weights: # code\tword\tlayer\tfreq_score
    "wubi": {"key_col": 1, "value_col": 3, "comment_prefix": "#"},
}


def load_harvest(path: Path) -> dict[str, int]:
    """Returns word → count (aggregated; harvest is already
    word-level, but defensive sum in case input has dupes)."""
    counts: dict[str, int] = defaultdict(int)
    with open(path, encoding="utf-8") as f:
        header = next(f, None)  # skip header row
        if header and not header.startswith("word\tcount"):
            sys.stderr.write(
                f"WARNING: harvest first line doesn't look like header: {header!r}\n"
            )
        for line in f:
            parts = line.rstrip("\n").split("\t")
            if len(parts) < 2:
                continue
            try:
                counts[parts[0]] += int(parts[1])
            except ValueError:
                continue
    return dict(counts)


def load_weights(path: Path, schema_name: str) -> dict[str, int]:
    """Returns word → aggregated freq_score across all (pinyin, word)
    or (code, word, layer) entries. Multiple readings of the same word
    sum their freq."""
    schema = WEIGHTS_SCHEMA[schema_name]
    counts: dict[str, int] = defaultdict(int)
    with open(path, encoding="utf-8") as f:
        for line in f:
            if line.startswith(schema["comment_prefix"]):
                continue
            parts = line.rstrip("\n").split("\t")
            if len(parts) <= max(schema["key_col"], schema["value_col"]):
                continue
            word = parts[schema["key_col"]]
            try:
                freq = int(parts[schema["value_col"]])
            except ValueError:
                continue
            counts[word] += freq
    return dict(counts)


def rank_map(counts: dict[str, int]) -> dict[str, int]:
    """Returns word → 1-based rank in descending-count order. Ties
    broken by word string for determinism."""
    sorted_items = sorted(counts.items(), key=lambda wc: (-wc[1], wc[0]))
    return {word: i + 1 for i, (word, _) in enumerate(sorted_items)}


def write_tsv(path: Path, rows: list[tuple], header: list[str]):
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        f.write("\t".join(header) + "\n")
        for row in rows:
            f.write("\t".join(str(c) for c in row) + "\n")


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--harvest", type=Path, required=True,
                    help="Path to corpus-harvest TSV (word, count, source, fetched_at)")
    ap.add_argument("--weights", type=Path, required=True,
                    help="Path to dict weights.tsv (engine-specific)")
    ap.add_argument("--weights-schema", choices=list(WEIGHTS_SCHEMA), required=True,
                    help="Which engine's weights.tsv schema: pinyin or wubi")
    ap.add_argument("--out", type=Path, required=True,
                    help="Output directory for the 3 diff reports")
    ap.add_argument("--rank-diff-threshold", type=int, default=1000,
                    help="In freq_changes.tsv, only list words whose rank "
                         "differs by at least this much between harvest and "
                         "weights. Default 1000 (so noise from low-freq "
                         "ranks is suppressed).")
    args = ap.parse_args()

    sys.stderr.write(f"Loading harvest:  {args.harvest}\n")
    harvest = load_harvest(args.harvest)
    sys.stderr.write(f"  {len(harvest):,} words\n")

    sys.stderr.write(f"Loading weights:  {args.weights} (schema={args.weights_schema})\n")
    weights = load_weights(args.weights, args.weights_schema)
    sys.stderr.write(f"  {len(weights):,} words (aggregated across readings)\n")

    sys.stderr.write(f"Computing rank maps...\n")
    h_rank = rank_map(harvest)
    w_rank = rank_map(weights)

    # 1. new_entries: in harvest, not in weights. Sorted by harvest count desc.
    new_words = set(harvest) - set(weights)
    new_rows = [
        (w, harvest[w], h_rank[w])
        for w in sorted(new_words, key=lambda w: (-harvest[w], w))
    ]

    # 2. dropped_candidates: in weights, not in harvest. Sorted by weights freq desc.
    dropped = set(weights) - set(harvest)
    dropped_rows = [
        (w, weights[w], w_rank[w])
        for w in sorted(dropped, key=lambda w: (-weights[w], w))
    ]

    # 3. freq_changes: in both, rank diff above threshold. Sorted by abs diff desc.
    both = set(harvest) & set(weights)
    changes = []
    for w in both:
        diff = h_rank[w] - w_rank[w]
        if abs(diff) >= args.rank_diff_threshold:
            changes.append((w, w_rank[w], h_rank[w], diff, weights[w], harvest[w]))
    changes.sort(key=lambda r: (-abs(r[3]), r[0]))
    change_rows = changes

    # Write outputs
    write_tsv(args.out / "new_entries.tsv", new_rows,
              ["word", "harvest_count", "harvest_rank"])
    write_tsv(args.out / "dropped_candidates.tsv", dropped_rows,
              ["word", "weights_freq", "weights_rank"])
    write_tsv(args.out / "freq_changes.tsv", change_rows,
              ["word", "weights_rank", "harvest_rank", "rank_diff",
               "weights_freq", "harvest_count"])

    # Summary
    sys.stderr.write(f"\n=== summary ===\n")
    sys.stderr.write(f"  new_entries:        {len(new_rows):,}\n")
    sys.stderr.write(f"  dropped_candidates: {len(dropped_rows):,}  (informational; not actually dropped)\n")
    sys.stderr.write(f"  freq_changes:       {len(change_rows):,}  (rank diff ≥ {args.rank_diff_threshold:,})\n")
    sys.stderr.write(f"\nwrote: {args.out}/{{new_entries,dropped_candidates,freq_changes}}.tsv\n")


if __name__ == "__main__":
    main()
