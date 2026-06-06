"""
Sweep wubi library.tsv for traditional-form entries.
Rule (conservative pass 1):
  delete (code, trad_word) IF
    - opencc t2s(trad_word) != trad_word  (entry contains TRAD char)
    - AND (code, simplified_peer) ALSO exists in library
         where simplified_peer == opencc t2s(trad_word)

Orphan TRAD entries (no same-code simp peer) are FLAGGED but NOT deleted
in pass 1 — would silently break wubi lookup for those chars.

Output:
  - audit list (code, trad_word, simp_peer, layer, freq)
  - rewritten library.tsv with deletions applied
"""
import subprocess
from collections import defaultdict

LIB = "core/crates/inputx-wubi/data/library.tsv"

entries = []
with open(LIB, encoding="utf-8") as f:
    for line in f:
        raw = line.rstrip("\n")
        if raw.startswith("#") or not raw.strip():
            entries.append((raw, None)); continue
        parts = raw.split("\t")
        if len(parts) < 5:
            entries.append((raw, None)); continue
        entries.append((raw, parts))

# Get distinct words
words = list({e[1][1] for e in entries if e[1]})
print(f"distinct words: {len(words)}")

# Batch through opencc t2s
proc = subprocess.run(
    ["opencc", "-c", "t2s.json"],
    input="\n".join(words),
    capture_output=True, text=True
)
simp_out = proc.stdout.rstrip("\n").split("\n")
simp_map = dict(zip(words, simp_out))

# Per-code → set of words at that code
by_code = defaultdict(set)
for e in entries:
    if e[1]:
        code = e[1][0]; word = e[1][1]
        by_code[code].add(word)

deletes = []      # (code, trad_word, simp_peer, layer, freq)
orphans = []      # (code, trad_word, simp_form_at_different_code_or_nowhere, layer, freq)
for e in entries:
    if not e[1]: continue
    code, word, layer, freq, source = e[1][:5]
    s = simp_map.get(word, word)
    if s == word: continue       # not trad
    if s in by_code[code]:
        deletes.append((code, word, s, layer, freq))
    else:
        orphans.append((code, word, s, layer, freq))

print(f"  delete-eligible (simp peer at same code): {len(deletes)}")
print(f"  orphan TRAD (no same-code simp peer)    : {len(orphans)}")

# Apply deletions
to_delete_keys = {(d[0], d[1]) for d in deletes}
kept = []
deleted_lines = []
for e in entries:
    if e[1] and (e[1][0], e[1][1]) in to_delete_keys:
        deleted_lines.append(e[0])
    else:
        kept.append(e[0])

with open(LIB, "w", encoding="utf-8") as f:
    for line in kept:
        f.write(line + "\n")

# Dump audit reports
with open("/tmp/wubi_trad_deleted.tsv", "w", encoding="utf-8") as f:
    f.write("# code\ttrad\tsimp_peer\tlayer\tfreq\n")
    for d in deletes:
        f.write("\t".join(d) + "\n")
with open("/tmp/wubi_trad_orphans.tsv", "w", encoding="utf-8") as f:
    f.write("# code\ttrad\tsimp_form\tlayer\tfreq\n")
    for o in orphans:
        f.write("\t".join(o) + "\n")

print(f"library rows: {len(entries)} → {len(kept)} (-{len(deletes)})")
print(f"audit: /tmp/wubi_trad_deleted.tsv  /tmp/wubi_trad_orphans.tsv")
