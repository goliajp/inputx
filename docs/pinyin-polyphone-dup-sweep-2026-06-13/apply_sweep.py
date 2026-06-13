#!/usr/bin/env python3
"""
Apply the polyphone-dup sweep: delete the (code, word) rows listed in
to_delete.tsv from the pinyin library.tsv, and log each to the corpus
garbage filter so a future corpus regen can't re-admit them.

Idempotent: re-running after the rows are gone is a no-op (reports 0 deleted).
"""
import os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", ".."))
LIB = os.path.join(REPO, "core/crates/inputx-pinyin/data/library.tsv")
GARBAGE = os.path.join(REPO, "tools/scoring/data/polish/corpus_garbage_filter_v1.tsv")
TODEL = os.path.join(HERE, sys.argv[1] if len(sys.argv) > 1 else "to_delete.tsv")
DATE = "2026-06-13"

targets = set()
for ln in open(TODEL, encoding="utf-8"):
    a = ln.rstrip("\n").split("\t")
    if a[0] == "delete_code" or len(a) < 2:
        continue
    targets.add((a[0], a[1]))
print(f"targets to delete: {len(targets)}")

kept, deleted = [], []
for ln in open(LIB, encoding="utf-8"):
    a = ln.rstrip("\n").split("\t")
    if len(a) >= 2 and (a[0], a[1]) in targets:
        deleted.append((a[0], a[1]))
        continue
    kept.append(ln.rstrip("\n"))

if not deleted:
    print("nothing matched — already applied?")
    sys.exit(0)

with open(LIB, "w", encoding="utf-8") as fh:
    fh.write("\n".join(kept) + "\n")

# append to garbage filter (dedup against existing rows)
existing = set()
for ln in open(GARBAGE, encoding="utf-8"):
    a = ln.rstrip("\n").split("\t")
    if len(a) >= 2 and not a[0].startswith("#"):
        existing.add((a[0], a[1]))
with open(GARBAGE, "a", encoding="utf-8") as fh:
    fh.write(f"# polyphone-dup sweep batch1 {DATE} — wrong-reading corpus copies "
             f"(audit: docs/pinyin-polyphone-dup-sweep-2026-06-13/)\n")
    n = 0
    for code, word in sorted(deleted):
        if (code, word) in existing:
            continue
        fh.write(f"{code}\t{word}\t{DATE}\n")
        n += 1
print(f"deleted {len(deleted)} rows from library.tsv")
print(f"logged {n} new rows to corpus_garbage_filter_v1.tsv")
miss = targets - set(deleted)
if miss:
    print(f"WARNING: {len(miss)} targets not found in library (sample): {list(miss)[:5]}")
