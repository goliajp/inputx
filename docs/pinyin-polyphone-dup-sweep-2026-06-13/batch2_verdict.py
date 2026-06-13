#!/usr/bin/env python3
"""
MIXED 双向多音字 per-word verdict — batch2a (small/medium chars).

For each MIXED char a default direction + per-word exceptions encode the
native-speaker reading call:
  del_wrong : word reads prim_syl, so the wrong_code copy is deleted.
  del_prim  : word reads wrong_syl (pypinyin mis-primaried), so the
              prim_code copy is deleted.
  uncertain : can't decide / 口语 boundary → keep BOTH, log to ask user.

RULES[char] = (default_dir, {word: dir_override}, {uncertain_words})
A '*' in the uncertain set means the whole char is uncertain (keep all).
"""
import collections, os

HERE = os.path.dirname(os.path.abspath(__file__))
CAND = os.path.join(HERE, "candidates.tsv")

RULES = {
    # whole-direction chars
    "彷": ("del_prim", {}, set()),    # 彷如 = 仿如 fǎng
    "蕃": ("del_wrong", {}, set()),   # 蕃薯/蕃人 = 番 fān
    "耙": ("del_prim", {}, set()),    # 犁耙/耙田 = pá
    "削": ("del_prim", {}, set()),    # 削苹果/削骨 = xiāo (动作)
    "堤": ("del_wrong", {}, set()),   # 河堤/堤坝 = dī (ti 已废)
    # NB 朝 deferred to batch2b: 双向 (朝代 cháo / 朝晨 zhāo) — 朝花夕拾/
    # 朝九晚五 read zhāo but sit in the chao→zhao group; default del-wrong
    # would误删 them. Needs per-group (cháo vs zhāo) split.
    # mixed: default + exceptions
    "沓": ("del_wrong", {"纷沓": "del_prim", "沓杂": "del_prim"}, set()),  # 一沓 dá / 纷沓·沓杂 tà
    "辟": ("del_wrong", {}, {"征辟", "便辟"}),  # 另辟 pì / 征辟·便辟 bì(生僻)
    "咽": ("del_wrong", {"凄咽": "del_prim"}, set()),  # 咽下/鼻咽 yàn·yān / 凄咽 yè
    "柏": ("del_wrong", {"柏拉": "del_prim", "柏辽兹": "del_prim", "库柏": "del_prim",
                        "阿古柏": "del_prim", "潘玮柏": "del_prim", "柏忌": "del_prim"}, set()),
    "刹": ("del_wrong", {"一刹": "del_prim", "刹海": "del_prim", "什刹海": "del_prim",
                        "塔刹": "del_prim", "刹帝利": "del_prim", "黎刹公园": "del_prim"}, set()),
    "呢": ("del_prim", {"多着呢": "del_wrong", "养呢": "del_wrong", "在哪玩呢": "del_wrong",
                       "蒸呢": "del_wrong", "吃法呢": "del_wrong"}, set()),  # 毛呢 ní / 语气 ne
    "泊": ("del_prim", {"镜泊": "del_wrong", "镜泊湖": "del_wrong", "泊湖": "del_wrong",
                       "泊中": "del_wrong"}, set()),  # 停泊 bó / 镜泊湖 pō
    "参": ("del_wrong", {"手参": "del_prim", "洋参": "del_prim", "辽参": "del_prim",
                        "本参": "del_prim", "兵参": "del_prim", "冰参": "del_prim"}, set()),
    "卜": ("del_prim", {"罗卜": "del_wrong", "箩卜": "del_wrong"}, {"卜卜"}),  # 占卜 bǔ / 萝卜 bo
    "沈": ("del_wrong", {"沈浸": "del_prim", "沈溺": "del_prim", "深沈": "del_prim",
                        "沈淀": "del_prim", "沈寂": "del_prim", "低沈": "del_prim",
                        "沈沦": "del_prim", "沈甸甸": "del_prim"}, set()),  # 沈阳 shěn / 沈=沉 chén
    "觉": ("del_wrong", {"睡觉觉": "del_prim", "回笼觉": "del_prim", "睡大觉": "del_prim",
                        "睡着觉": "del_prim", "中觉": "del_prim"}, set()),  # 感觉 jué / 睡觉 jiào
    # whole-char uncertain
    "嗯": ("uncertain", {}, {"*"}),
    "爪": ("uncertain", {}, {"*"}),
}

rows = []
for ln in open(CAND, encoding="utf-8"):
    a = ln.rstrip("\n").split("\t")
    if a[0] == "code" or len(a) < 7:
        continue
    code, word, freq, pc, ch, ps, ws = a[0], a[1], int(a[2]), a[3], a[4], a[5], a[6]
    if freq < 3000 or ch not in RULES:
        continue
    rows.append((code, word, freq, pc, ch, ps, ws))

deletions = []      # (delete_code, word, freq, why)
uncertain = []      # (ch, word, freq, prim_code, wrong_code)
for code, word, freq, pc, ch, ps, ws in rows:
    default, exc, unc = RULES[ch]
    if "*" in unc or word in unc:
        uncertain.append((ch, word, freq, pc, code))
        continue
    d = exc.get(word, default)
    if d == "uncertain":
        uncertain.append((ch, word, freq, pc, code))
    elif d == "del_wrong":
        deletions.append((code, word, freq, f"{ch}:{ps}/{ws} del-wrong b2"))
    elif d == "del_prim":
        deletions.append((pc, word, freq, f"{ch}:{ps}/{ws} del-prim b2"))

# de-dup
seen, uniq = set(), []
for code, word, freq, why in deletions:
    if (code, word) in seen:
        continue
    seen.add((code, word))
    uniq.append((code, word, freq, why))

with open(os.path.join(HERE, "to_delete_batch2a.tsv"), "w", encoding="utf-8") as fh:
    fh.write("delete_code\tword\tfreq\twhy\n")
    for r in sorted(uniq, key=lambda x: -x[2]):
        fh.write("\t".join(str(x) for x in r) + "\n")

with open(os.path.join(HERE, "uncertain_batch2.tsv"), "w", encoding="utf-8") as fh:
    fh.write("char\tword\tfreq\tprim_code\twrong_code\n")
    for r in sorted(uncertain, key=lambda x: (x[0], -x[2])):
        fh.write("\t".join(str(x) for x in r) + "\n")

bc = collections.Counter(r[3].split(":")[0] for r in uniq)
print(f"batch2a deletions: {len(uniq)} rows across {len(bc)} chars")
print(f"uncertain (ask user): {len(uncertain)} words")
# show deletions grouped for review
g = collections.defaultdict(lambda: {"w": [], "p": []})
for code, word, freq, why in uniq:
    ch = why.split(":")[0]
    (g[ch]["w"] if "del-wrong" in why else g[ch]["p"]).append(word)
for ch in sorted(g):
    w, p = g[ch]["w"], g[ch]["p"]
    print(f"  {ch}: del-wrong[{len(w)}]={' '.join(w[:8])}  | del-prim[{len(p)}]={' '.join(p[:8])}")
