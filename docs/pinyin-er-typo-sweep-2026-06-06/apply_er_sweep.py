"""Delete the 165 (code, word) typo rows from pinyin library.tsv and
log them to corpus_garbage_filter_v1.tsv."""
LIB = "core/crates/inputx-pinyin/data/library.tsv"
GARBAGE = "tools/scoring/data/polish/corpus_garbage_filter_v1.tsv"

# Load suspects
suspects = set()
with open("/tmp/er_typos.tsv", encoding="utf-8") as f:
    for line in f:
        if line.startswith("#"): continue
        parts = line.rstrip("\n").split("\t")
        if len(parts) >= 3:
            suspects.add((parts[0], parts[2]))  # (typo_code, word)

# Rewrite library.tsv
kept = []
removed = []
with open(LIB, encoding="utf-8") as f:
    for raw in f:
        line = raw.rstrip("\n")
        if line.startswith("#") or not line.strip():
            kept.append(line); continue
        parts = line.split("\t")
        if len(parts) >= 2:
            key = (parts[0], parts[1])
            if key in suspects:
                removed.append(line)
                continue
        kept.append(line)

with open(LIB, "w", encoding="utf-8") as f:
    for line in kept:
        f.write(line + "\n")
print(f"library.tsv rows: removed {len(removed)} (expected {len(suspects)})")

# Append to corpus_garbage_filter
HEADER = """
# 2026-06-06 systemic pinyin er→r typo sweep — user "zhonghuarnv 为什么会
# 出现 中华儿女呢，不应该是 zhonghuaernv 吗".  Corpus encoding dropped the
# 'e' from 'er' (儿) in mid-word position, leaving rows like (zhonghuarnv,
# 中华儿女) alongside the correct (zhonghuaernv, 中华儿女).  Detected by
# script: word at code `<X>r<Y>` AND the SAME word at `<X>er<Y>` →
# typo row.
"""
with open(GARBAGE, "a", encoding="utf-8") as f:
    f.write(HEADER)
    for code, word in sorted(suspects):
        f.write(f"{code}\t{word}\t2026-06-06\n")
print(f"corpus_garbage_filter_v1.tsv: appended {len(suspects)} rows")
