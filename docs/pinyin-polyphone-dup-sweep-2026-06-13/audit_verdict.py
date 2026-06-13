#!/usr/bin/env python3
"""
Native-speaker audit verdict over candidates.tsv (freq>=3000 SAFE groups).

Per (char) group, classify which side is the wrong-reading copy to delete:
  DELETE_WRONG : default — wrong_syl is the错读, delete the wrong_code row.
                 (prim reading is correct and stays at同 freq.)
  DELETE_PRIM  : pypinyin mis-set the char's PRIMARY; the words actually read
                 wrong_syl, so it's the prim_code row that's the错读 copy.
  MIXED        : both readings carry real words (双向多音字); needs per-word
                 handling — left out of this pass, listed for review.

EXCEPT_KEEP : per (char) a set of words whose pypinyin-mis-read side must be
              kept (e.g. 似的/似地 really read shì even though 似 is otherwise sì).
"""
import collections, os

HERE = os.path.dirname(os.path.abspath(__file__))
CAND = os.path.join(HERE, "candidates.tsv")

# A — high-confidence: wrong_syl is a rare/wrong reading; prim is correct.
# NB 着 moved to MIXED: 双向 (看着 zhe 轻声 / 睡不着 zháo / 着实 zhuó) —
# its zhao/zhuo copies aren't all错读, needs per-word.
DELETE_WRONG = set("大万会没率塞角乐传见和寻适便臂桔剖咳哦呵吁畜呐落埋抹乾")
# B — direction reversed: pypinyin's primary is wrong; the listed words read
#     the "wrong" syllable, so the prim_code row is the copy to delete.
# NB 卜 moved to MIXED: 双向 (占卜 bǔ / 萝卜·罗卜·箩卜 bo) — bo copies of
# 萝卜 variants aren't错读, pypinyin词库 doesn't protect the variant spellings.
DELETE_PRIM = set("似町聒炔芎芘嗲肋")
# words to KEEP even though their char is in DELETE_PRIM (they genuinely use prim)
EXCEPT_KEEP = {"似": {"似的", "似地"}}
# C — genuinely双向; defer to per-word pass.
MIXED = set("朝卜着长行重圈系调弹著觉参呢柏刹咽耙辟沓爪都恶血薄沈削泊彷降折差称盛壳模还宿")

rows = []
for ln in open(CAND, encoding="utf-8"):
    a = ln.rstrip("\n").split("\t")
    if a[0] == "code" or len(a) < 7:
        continue
    code, word, freq, pc, ch, ps, ws = a[0], a[1], int(a[2]), a[3], a[4], a[5], a[6]
    rows.append((code, word, freq, pc, ch, ps, ws))

del_rows = []          # (code_to_delete, word, freq, why)
mixed_rows = []
for code, word, freq, pc, ch, ps, ws in rows:
    if freq < 3000:
        continue
    if ch in DELETE_WRONG:
        del_rows.append((code, word, freq, f"{ch}:{ps}->{ws} del-wrong"))
    elif ch in DELETE_PRIM:
        if word in EXCEPT_KEEP.get(ch, set()):
            continue
        del_rows.append((pc, word, freq, f"{ch}:{ps}->{ws} REVERSED del-prim"))
    elif ch in MIXED:
        mixed_rows.append((code, word, freq, pc, ch, ps, ws))

# de-dup deletion rows (a word can appear via two wrong codes e.g. 着 zhao/zhuo)
seen = set()
uniq = []
for code, word, freq, why in del_rows:
    k = (code, word)
    if k in seen:
        continue
    seen.add(k)
    uniq.append((code, word, freq, why))

with open(os.path.join(HERE, "to_delete.tsv"), "w", encoding="utf-8") as fh:
    fh.write("delete_code\tword\tfreq\twhy\n")
    for r in sorted(uniq, key=lambda x: -x[2]):
        fh.write("\t".join(str(x) for x in r) + "\n")

mc = collections.Counter(r[4] for r in mixed_rows)
with open(os.path.join(HERE, "mixed_pending.md"), "w", encoding="utf-8") as fh:
    fh.write("# MIXED groups — per-word pass needed (双向多音字)\n\n")
    fh.write("Both readings carry real words; pypinyin保护 some but not all. "
             "Listed for the per-word pass.\n\n")
    g = collections.defaultdict(list)
    for code, word, freq, pc, ch, ps, ws in mixed_rows:
        g[(ch, ps, ws)].append((word, freq))
    for (ch, ps, ws), items in sorted(g.items(), key=lambda x: -len(x[1])):
        items.sort(key=lambda x: -x[1])
        fh.write(f"## {ch} {ps}→{ws} ({len(items)})\n\n"
                 + " ".join(w for w, _ in items[:30]) + "\n\n")

print(f"DELETE_WRONG groups: {len(DELETE_WRONG)} chars")
print(f"DELETE_PRIM (reversed) groups: {len(DELETE_PRIM)} chars")
print(f"MIXED groups deferred: {len(MIXED)} chars, {len(mixed_rows)} words")
print(f"-> to_delete.tsv: {len(uniq)} rows (high-confidence this pass)")
print(f"-> mixed_pending.md: {sum(mc.values())} words across {len(mc)} chars")
