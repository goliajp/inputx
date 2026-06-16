#!/usr/bin/env python3
"""
batch2-hard verdict — the 7 hardest MIXED chars deferred from
batch2a/b/c: 重 着 长 行 血 调 著.

Each char gets the same `(default_dir, exceptions, uncertain)` shape
as `batch2_verdict.py`. Single direction (xing→hang for 行; etc.)
because pypinyin's primary already picked one reading and all the
mechanical copies sit on the other.

This file is incremental — chars get filled in one at a time as the
native-speaker audit completes. Apply via `apply_sweep.py` after
classifying.

Currently filled:
  行 — 265 unique words, single direction xing→hang. Default del_wrong
    (xíng 走/动/进行 主流读). del_prim whitelist covers 行业/银行/
    商店/线行 (matrix row, code line) categories explicitly.
"""
import collections, os

HERE = os.path.dirname(os.path.abspath(__file__))
CAND = os.path.join(HERE, "candidates.tsv")

RULES = {
    "行": ("del_wrong",
           {
               # 银行类 (financial 行 = háng)
               "行员": "del_prim", "支行": "del_prim", "市行": "del_prim",
               "省行": "del_prim", "跨行": "del_prim", "联行": "del_prim",
               "行库": "del_prim", "行内": "del_prim", "行尊": "del_prim",
               "行外话": "del_prim",
               # 行业 / 同行 类 (industry 行 = háng)
               "各行": "del_prim",
               # 地名 / 山名 (place háng)
               "太行": "del_prim", "闵行": "del_prim", "闵行区": "del_prim",
               # 商店类 (shop 行 = háng)
               "琴行": "del_prim", "报关行": "del_prim", "律师行": "del_prim",
               "电器行": "del_prim", "茶行": "del_prim", "鞋行": "del_prim",
               "纸行": "del_prim", "药行": "del_prim", "布行": "del_prim",
               "珠宝行": "del_prim", "乐器行": "del_prim", "家具行": "del_prim",
               "建材行": "del_prim", "打字行": "del_prim",
               # 行(line / row · code / matrix / display · háng)
               "行数": "del_prim", "几行": "del_prim", "几行字": "del_prim",
               "末行": "del_prim", "首行": "del_prim", "整行": "del_prim",
               "千行": "del_prim", "行尾": "del_prim", "行频": "del_prim",
               "行向量": "del_prim", "提示行": "del_prim", "命令行": "del_prim",
               "第二行": "del_prim", "第四行": "del_prim", "好几行": "del_prim",
               "数行": "del_prim", "缩行": "del_prim",
               "漏行": "del_prim", "移行": "del_prim", "竖行": "del_prim",
               # 亚行 (Asian Development Bank = háng)
               "亚行": "del_prim",
               # N行 line-numeric (line N = háng)
               "二行": "del_prim", "四行": "del_prim", "六行": "del_prim",
               "七行": "del_prim", "八行": "del_prim", "十二行": "del_prim",
               "十四行": "del_prim", "十六行": "del_prim",
               "两行": "del_prim", "俩行": "del_prim",
               # 显示器 / 串行 / 逐行 (技术 háng)
               "串行": "del_prim", "逐行": "del_prim", "串行接口": "del_prim",
               "逐行扫描": "del_prim",
               # 行天宫 (台湾庙宇 háng-tiān-gōng)
               "行天宫": "del_prim",
               # 行内人士 (industry insiders háng)
               "行内人士": "del_prim",
               # 行规 / 行话 explicit (not always in list)
               # 行有行规 explicit
               "行有行规": "del_prim",
           },
           {
               # 双向 / 真模糊 (keep both — uncertain trumps exc per code)
               "人行",     # háng 人民银行简称 vs xíng 人行道
               "大行",     # háng 大型银行 / 大行令 vs xíng 大有所行
               "行社",     # rare · ambiguous
               "行相",     # rare · ambiguous
               "民行",     # háng 民营银行 vs xíng 民事行政
               "行约",     # rare · ambiguous
               "行检",     # 行检 háng (line check?) vs xíng (行为检察)
               "续行",     # 续行 háng (continue line) vs xíng (继续行动)
               "全行",     # háng vs xíng 全部行业 vs 全员行动
               "壮行",     # xíng but 壮行酒 has both readings
               "异行",     # rare
               "兼行",     # rare
               "并肩而行",  # xíng but be safe
               "六人行",   # xíng (six people walking) BUT 六人行 = friends sitcom · uncertain
               "行棋",     # ambiguous
               "顺行",     # xíng but uncertain
               "知行",     # xíng (陶行知) but uncertain
               "侠客行",   # xíng (李白诗 / 金庸小说) — keep both safe
               "新行",     # rare
               "始行",     # xíng but uncertain
               "末行画",   # rare
               "字行",     # ambiguous
               "末行书",   # rare
               "句行",     # rare
               "草行",     # rare 草行书 vs 草地而行 — actually uncertain
           }),
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

deletions = []
uncertain = []
kept_default = []
for code, word, freq, pc, ch, ps, ws in rows:
    default, exc, unc = RULES[ch]
    if "*" in unc or word in unc:
        uncertain.append((ch, word, freq, pc, code))
        continue
    direction = exc.get(word, default)
    if direction == "del_wrong":
        deletions.append((code, word, freq, "del_wrong"))
    elif direction == "del_prim":
        deletions.append((pc, word, freq, "del_prim"))
    else:
        uncertain.append((ch, word, freq, pc, code))
    kept_default.append((word, direction))


if __name__ == "__main__":
    # Dump the to-delete list. Apply via apply_sweep.py:
    #   python apply_sweep.py < batch2_hard_to_delete.tsv
    out = os.path.join(HERE, "batch2_hard_to_delete.tsv")
    with open(out, "w", encoding="utf-8") as f:
        f.write("delete_code\tword\tfreq\twhy\n")
        for c, w, fq, why in deletions:
            f.write(f"{c}\t{w}\t{fq}\t{why}\n")
    print(f"deletions: {len(deletions)} rows → {out}")
    print(f"uncertain: {len(uncertain)} rows kept (both readings)")
    bc = collections.Counter(d[3] for d in deletions)
    print(f"  del_wrong: {bc.get('del_wrong', 0)}  del_prim: {bc.get('del_prim', 0)}")
    if uncertain:
        print("uncertain words:")
        for ch, w, fq, pc, c in uncertain[:30]:
            print(f"  {w} (freq={fq}) [{pc} / {c}]")
