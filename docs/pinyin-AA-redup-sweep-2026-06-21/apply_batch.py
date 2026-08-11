#!/usr/bin/env python3
"""Apply a batch deletion list against library.tsv + garbage_filter.

Reads a `<batchN>_to_delete.tsv` file with schema:
    code\tword\treason

For each (code, word):
  1. Remove ALL matching `(code, word)` rows from library.tsv,
     regardless of source tag (digested OR polish — caller must
     have audited).
  2. Append `(code, word, date)\t# reason` to corpus_garbage_filter.

Usage:
    python3 docs/pinyin-AA-redup-sweep-2026-06-21/apply_batch.py \
        batch1b_to_delete.tsv 2026-06-22
"""
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
LIB = REPO / "core/crates/inputx-pinyin/data/library.tsv"
GARBAGE = REPO / "tools/scoring/data/polish/corpus_garbage_filter_v1.tsv"


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: apply_batch.py <batch_to_delete.tsv> <YYYY-MM-DD>")
        return 2
    batch_path = Path(sys.argv[1])
    if not batch_path.is_absolute():
        batch_path = Path(__file__).parent / batch_path
    date = sys.argv[2]

    # Load deletion targets — set of (code, word).
    targets: set[tuple[str, str]] = set()
    reasons: dict[tuple[str, str], str] = {}
    with batch_path.open() as f:
        for line in f:
            line = line.rstrip("\n")
            if not line or line.startswith("#"):
                continue
            parts = line.split("\t")
            if len(parts) < 3:
                continue
            code, word, reason = parts[0], parts[1], parts[2]
            targets.add((code, word))
            reasons[(code, word)] = reason

    # Filter library.tsv.
    keep_lines = []
    removed_rows = []
    with LIB.open() as f:
        for line in f:
            parts = line.rstrip("\n").split("\t")
            if len(parts) >= 2 and (parts[0], parts[1]) in targets:
                removed_rows.append(parts)
                continue
            keep_lines.append(line)

    if not removed_rows:
        print(f"no matching rows in library.tsv for {len(targets)} targets — nothing to do")
        return 1

    LIB.write_text("".join(keep_lines))
    print(f"removed {len(removed_rows)} rows from library.tsv "
          f"(out of {len(targets)} targets in batch)")

    # Append to garbage_filter (one block per batch).
    block = [
        "",
        f"# {date} Batch {batch_path.stem.replace('_to_delete', '')} —"
        f" AA-redup sweep, {len(removed_rows)} rows deleted from library.tsv.",
        f"# Audit doc: docs/pinyin-AA-redup-sweep-2026-06-21/.",
    ]
    for code, word, *_ in removed_rows:
        reason = reasons.get((code, word), "noise")
        block.append(f"{code}\t{word}\t{date}\t# {reason}")
    with GARBAGE.open("a") as f:
        f.write("\n".join(block) + "\n")
    print(f"appended {len(removed_rows)} rows to "
          f"{GARBAGE.relative_to(REPO)}")

    # Report unmatched targets (in batch file but not in library — already
    # deleted manually, or typo).
    matched_set = {(p[0], p[1]) for p in removed_rows}
    unmatched = targets - matched_set
    if unmatched:
        print(f"\n{len(unmatched)} unmatched targets (already gone or typo):")
        for code, word in sorted(unmatched):
            print(f"  {code}\t{word}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
