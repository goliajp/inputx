"""
Second half of the 日本新字体 sweep: make the 217 swept chars Japanese-only.

  1. nihongo library.tsv  ← APPEND single-kanji rows (from /tmp/jp_new_rows.tsv,
     KANJIDIC2-sourced, round-trip-verified) for the 153 chars not already
     present as single kanji. Keeps the chars type-able in Japanese.
  2. pinyin library.tsv   → DELETE the 217 single-char noise rows (these
     Japanese chars sat in the pinyin dict under Chinese readings, e.g.
     ba→抜) + log each to corpus_garbage_filter_v1.tsv so a future
     corpus-digest can't re-admit them.

Run:
  python3 docs/wubi-jp-shinjitai-sweep-2026-06-08/apply_nihongo_pinyin.py          # dry-run
  python3 docs/wubi-jp-shinjitai-sweep-2026-06-08/apply_nihongo_pinyin.py --apply
"""
import sys
from pathlib import Path

APPLY = "--apply" in sys.argv
DATE = "2026-06-08"

DELS = {l.split("\t")[0] for l in
        Path("docs/wubi-jp-shinjitai-sweep-2026-06-08/deleted.tsv").read_text().splitlines()
        if l and not l.startswith("#")}
NLIB = Path("core/crates/inputx-nihongo/data/library.tsv")
PYLIB = Path("core/crates/inputx-pinyin/data/library.tsv")
GARBAGE = Path("tools/scoring/data/polish/corpus_garbage_filter_v1.tsv")
NEW_ROWS = Path("/tmp/jp_new_rows.tsv")

new_rows = [l for l in NEW_ROWS.read_text(encoding="utf-8").splitlines() if l]
print(f"nihongo rows to append: {len(new_rows)}  ({len({r.split(chr(9))[1] for r in new_rows})} chars)")

# pinyin rows to delete
py_lines = PYLIB.read_text(encoding="utf-8").splitlines()
py_kept, py_dropped = [], []
for line in py_lines:
    if not line or line.startswith("#"):
        py_kept.append(line); continue
    p = line.split("\t")
    if len(p) >= 2 and p[1] in DELS and len(p[1]) == 1:
        py_dropped.append((p[0], p[1]))   # (code, word)
    else:
        py_kept.append(line)
print(f"pinyin rows to delete: {len(py_dropped)}")

if not APPLY:
    print("\n[dry-run] no files written. re-run with --apply.")
    sys.exit(0)

# 1. append nihongo rows
with NLIB.open("a", encoding="utf-8") as f:
    f.write("\n".join(new_rows) + "\n")
print(f"nihongo library.tsv += {len(new_rows)} rows")

# 2. delete pinyin rows
PYLIB.write_text("\n".join(py_kept) + "\n", encoding="utf-8")
print(f"pinyin library.tsv: {len(py_lines)} → {len(py_kept)} rows (-{len(py_dropped)})")

# 3. log to garbage filter
with GARBAGE.open("a", encoding="utf-8") as f:
    for code, word in py_dropped:
        f.write(f"{code}\t{word}\t{DATE}\n")
print(f"corpus_garbage_filter_v1.tsv += {len(py_dropped)} rows")
print("\n[applied]")
