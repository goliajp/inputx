#!/usr/bin/env python3
"""Recapture a set of (code, word) pairs that were swept in a prior
batch but should be kept. Reverses the apply_batch.py effect:
  1. Inserts `source=polish` row(s) into library.tsv with the user-
     supplied freq.
  2. Removes matching `code\tword\t<date>` rows from corpus_garbage_
     filter so future regens don't re-block.

Input file schema:
    code\tword\tfreq\t# note

Usage:
    python3 docs/pinyin-AA-redup-sweep-2026-06-21/recapture.py recapture_b1b.tsv
"""
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
LIB = REPO / "core/crates/inputx-pinyin/data/library.tsv"
GARBAGE = REPO / "tools/scoring/data/polish/corpus_garbage_filter_v1.tsv"


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: recapture.py <recapture.tsv>")
        return 2
    path = Path(sys.argv[1])
    if not path.is_absolute():
        path = Path(__file__).parent / path

    targets = []
    target_pairs: set[tuple[str, str]] = set()
    with path.open() as f:
        for line in f:
            line = line.rstrip("\n")
            if not line or line.startswith("#"):
                continue
            parts = line.split("\t")
            if len(parts) < 3:
                continue
            code, word, freq = parts[0], parts[1], parts[2]
            targets.append((code, word, freq))
            target_pairs.add((code, word))

    # Append polish rows to library.tsv (the file is unsorted within-code,
    # build chain re-sorts).
    with LIB.open("a") as f:
        for code, word, freq in targets:
            f.write(f"{code}\t{word}\t{freq}\tpolish\n")
    print(f"appended {len(targets)} polish rows to library.tsv")

    # Filter garbage_filter, drop matching code+word lines.
    keep_lines = []
    dropped = 0
    with GARBAGE.open() as f:
        for line in f:
            stripped = line.rstrip("\n")
            if stripped.startswith("#") or not stripped:
                keep_lines.append(line)
                continue
            parts = stripped.split("\t")
            if len(parts) >= 2 and (parts[0], parts[1]) in target_pairs:
                dropped += 1
                continue
            keep_lines.append(line)
    GARBAGE.write_text("".join(keep_lines))
    print(f"removed {dropped} rows from corpus_garbage_filter")

    if dropped != len(targets):
        print(f"WARNING: dropped {dropped} but recapture asked for "
              f"{len(targets)} — some weren't in garbage_filter")
    return 0


if __name__ == "__main__":
    sys.exit(main())
