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
  长 — 283 unique words, single direction zhang→chang per pypinyin
    prim choice. Default del_prim (cháng = "long" 长度/时间/place names —
    most usage; pypinyin's zhang→ row is the wrong-syl copy). Exception
    list = zhǎng-reading words ("grow" 长胖/长肉/长智 + 官名 营长/
    族长/委员长/教务长 + 长 prefix as senior — 长嫂/长媳).
  重 — 220 unique words, single direction zhong→chong per pypinyin
    prim. Default del_wrong (zhòng = heavy/important/serious 重要/
    重病/重视 主流). Exception del_prim = chóng-reading words (重 =
    again/re-: 重来/重塑/重新做/重整旗鼓/重见光明 + 重庆-abbreviations).
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
    "长": ("del_prim",
           {
               # zhǎng (grow) — del_wrong (delete chang-copy, keep zhang)
               "长胖": "del_wrong", "长肉": "del_wrong", "长痘": "del_wrong",
               "长痘痘": "del_wrong", "长肥": "del_wrong", "长智": "del_wrong",
               "长见识": "del_wrong", "长一智": "del_wrong",
               "长见识了": "del_wrong", "长得帅": "del_wrong",
               "越长越": "del_wrong", "长胡子": "del_wrong",
               "长皱纹": "del_wrong", "长皮": "del_wrong", "长斑": "del_wrong",
               "长痣": "del_wrong", "渐长": "del_wrong",
               "改长": "del_wrong",  # (let it 长 grow longer? actually uncertain — but lean zhǎng)
               "吃一堑长一智": "del_wrong",
               "不长一智": "del_wrong",
               "此消彼长": "del_wrong",
               "春生夏长": "del_wrong",
               "争长": "del_wrong",   # 争长论短 zhǎng
               "一家之长": "del_wrong",  # zhǎng (家长)
               "愈长": "del_wrong",
               # zhǎng (senior official / 官名) — del_wrong
               "探长": "del_wrong", "营长": "del_wrong", "族长": "del_wrong",
               "大队长": "del_wrong", "室长": "del_wrong", "舰长": "del_wrong",
               "厅长": "del_wrong", "园长": "del_wrong", "车长": "del_wrong",
               "区长": "del_wrong", "秘书长": "del_wrong",
               "检察长": "del_wrong", "委员长": "del_wrong",
               "中队长": "del_wrong", "旅长": "del_wrong", "级长": "del_wrong",
               "署长": "del_wrong", "理事长": "del_wrong",
               "场长": "del_wrong", "处处长": "del_wrong",
               "总参谋长": "del_wrong", "副委员长": "del_wrong",
               "副科长": "del_wrong", "副大队长": "del_wrong",
               "副厅长": "del_wrong", "副区长": "del_wrong",
               "副理事长": "del_wrong", "副检察长": "del_wrong",
               "教务长": "del_wrong", "教育长": "del_wrong",
               "教育厅长": "del_wrong", "教长": "del_wrong",
               "段长": "del_wrong",
               "拉拉队长": "del_wrong",
               "幕僚长": "del_wrong",
               "事务长": "del_wrong",
               "书记长": "del_wrong",
               "艇长": "del_wrong",
               "炊事班长": "del_wrong",
               "调度长": "del_wrong",
               "执行长": "del_wrong",  # 台 = CEO
               "片儿长": "del_wrong",
               "块长": "del_wrong",
               "舍长": "del_wrong",
               "分队长": "del_wrong",
               "分局长": "del_wrong",
               "岗长": "del_wrong",
               "军士长": "del_wrong",
               "郡长": "del_wrong",
               "信长": "del_wrong",  # 织田信长
               "小学校长": "del_wrong",
               "中学校长": "del_wrong",
               "织田信长": "del_wrong",
               "典狱长": "del_wrong",
               "监狱长": "del_wrong",
               "狱长": "del_wrong",
               "廷长": "del_wrong",
               "参议长": "del_wrong",
               "女家长": "del_wrong",
               "新大长": "del_wrong",  # 新大头娘大 — zhǎng (rare)
               "片长": "del_wrong",  # 制片长 (film team head)
               "钱长": "del_wrong",  # rare zhǎng
               "尊长": "del_wrong",  # 尊敬长辈 zhǎng-bèi
               # zhǎng (senior) family role
               "长媳": "del_wrong",
               "长嫂": "del_wrong",
               "长嫂如母": "del_wrong",
           },
           {
               # 双向 / 真模糊 (keep both)
               "长长",      # cháng-cháng vs zhǎng-zhǎng
               "长头",      # 长头(发) cháng vs zhǎng (grow head)
               "长大",      # cháng-dà (big and long?) vs zhǎng-dà (grow up)
               "子长",      # place name zǐ-zhǎng (Shaanxi 县) vs name
               "长黑",      # zhǎng (grow black hair) vs cháng (long-black)
               "长尾巴",    # zhǎng (grow tail) vs cháng (long tail)
               "长嘴",      # cháng (long beak) vs zhǎng (grow mouth)
               "长耳朵",    # cháng (long ear) vs zhǎng (grow ears)
               "长明",      # 长明灯 cháng — but uncertain in isolation
               "长留",      # cháng-liú vs zhǎng (let it grow more)
               "长鼻",      # cháng (long nose) — but zhǎng (grow nose) rare
               "长嚎",      # cháng-háo (long howl) — but zhǎng-háo (grow loud) rare
               "长忧",      # rare · ambiguous
               "长情",      # cháng-qíng — but uncertain spelled this way alone
               "长针",      # cháng (long needle) — but zhǎng-zhēn-yǎn rare
               "渔长",      # rare
               "扁长",      # cháng (oblong) — uncertain
               "码长",      # cháng (码 长 = numbering length) — uncertain
               "幽长",      # rare
               "音长",      # cháng (audio length) — uncertain (zhǎng meaningless)
               "字长",      # cháng (word length, bytes) — uncertain
               "颈长",      # uncertain
               "桥长",      # cháng (bridge length) — uncertain (zhǎng meaningless)
               "纵长",      # uncertain
               "横长",      # uncertain
               "条长",      # uncertain
               "线长",      # cháng (line length) but uncertain
               "弦长",      # cháng (chord length) — uncertain
               "细长",      # cháng (slender) — but spelled this often zhǎng-cháng rare
               "细细长长",  # cháng (slender repeat) — usually cháng
               "瘦瘦长长",  # cháng — usually cháng
               "副长",      # rare zhǎng senior
               "主长",      # rare zhǎng
               "群长",      # rare zhǎng (group leader)
               "渔船长",    # zhǎng — but rare
               "辉长岩",    # 辉长岩 = huī-cháng-yán (geology rock) — uncertain
               "渊远流长",  # cháng (variant of 源远流长)
               "可长可短",  # cháng — but pattern hard
               "前短后长",  # cháng — usually cháng
               "忽长忽短",  # cháng — usually cháng
               "有长有短",  # cháng — usually cháng
               "昼短夜长",  # cháng — clear cháng but pattern hard
               "不长不短",  # cháng — pattern
               "情深谊长",  # cháng (long affection) — clear
               "长盛不衰",  # cháng-shèng — clear cháng
               "又臭又长",  # cháng — clear
               "风物长宜放眼量",  # cháng — clear
               "新大长",    # 新大头娘大 — rare/uncertain
               "渔长",      # rare
               "挂长",      # uncertain
               "托长",      # uncertain
               "摆长",      # uncertain
               "炮长",      # rare zhǎng
               "副长",      # rare zhǎng
           }),
    "重": ("del_wrong",
           {
               # chóng (again / re-) — del_prim (delete zhong-copy, keep chong)
               "重来": "del_prim", "重回": "del_prim", "重塑": "del_prim",
               "重蹈": "del_prim", "重振": "del_prim", "重整": "del_prim",
               "重制": "del_prim", "重考": "del_prim", "重拾": "del_prim",
               "重开": "del_prim", "重游": "del_prim", "重发": "del_prim",
               "重见": "del_prim", "重燃": "del_prim", "重刷": "del_prim",
               "重放": "del_prim", "重画": "del_prim", "重抄": "del_prim",
               "重入": "del_prim", "重描": "del_prim", "重号": "del_prim",
               "重涂": "del_prim", "重爬": "del_prim", "重接": "del_prim",
               "重计": "del_prim", "重访": "del_prim", "重考生": "del_prim",
               "重码": "del_prim", "重组": "del_prim", "重归": "del_prim",
               "重归于好": "del_prim", "重见光明": "del_prim",
               "重做": "del_prim", "重定向": "del_prim", "重划": "del_prim",
               "重学": "del_prim", "重晚": "del_prim",
               # chóng phrases
               "推倒重来": "del_prim", "故地重游": "del_prim",
               "故技重施": "del_prim", "重施故技": "del_prim",
               "重整旗鼓": "del_prim", "重整齐鼓": "del_prim",
               "催化重整": "del_prim", "重轰炸机": "del_prim",
               "中联重科": "del_wrong",  # NB 中联重科 = 重 zhòng (heavy industry)
               # 重庆 abbrev (重 reads chóng in 重庆)
               "重邮": "del_prim",
           },
           {
               # 双向 / 真模糊 (keep both)
               "重为",       # zhòng (重视 do as 主) vs chóng (do again)
               "重作",       # chóng-zuò (redo) vs zhòng-zuò (do importantly)
               "重排",       # zhòng-pái (heavy rank) vs chóng-pái (rearrange)
               "重挫",       # zhòng (heavy blow) vs chóng (re-defeat)
               "重传",       # zhòng (heavy biography) vs chóng (retransmit)
               "重报",       # zhòng (heavy reward) vs chóng (re-report)
               "重信",       # zhòng (重信用) vs chóng (re-letter)
               "重摔",       # zhòng (heavy fall) vs chóng (re-throw)
               "重跌",       # zhòng (heavy fall) vs chóng (再跌)
               "重耳",       # 春秋人名 — debated reading
               "重雪",       # rare
               "重当",       # rare
               "重百",       # rare
               "重氮",       # zhòng (chemistry double-N) — uncertain
               "重难点",     # zhòng-nán-diǎn — but could be chong-nan-dian rare
               "重特大",     # zhòng-tè-dà (heavy + very big) — uncertain
               "重瓣",       # uncertain
               "重疾",       # zhòng-jí (serious illness) — uncertain
               "重剑",       # zhòng (heavy sword) — uncertain
               "重旱",       # uncertain
               "重场",       # uncertain
               "重宝",       # zhòng (heavy treasure) — uncertain
               "重图",       # uncertain
               "重谢",       # zhòng-xiè (heavy thanks) — uncertain
               "重压",       # zhòng (heavy press) — uncertain
               "重者",       # 重的 = zhòng, 重者 = those who emphasize — uncertain
               "重描",       # NB del_prim above (重新描) — but in case ambiguous
               # 数 + 重 (layer count chóng)
               "一重", "两重", "三重", "四重", "五重",
               "六重", "七重", "八重", "十重", "几重",
               "多重", "几重", "多次重",
               "第一重", "第四重", "千重",
               "两重性", "多重性", "多重人格",
               "重叠",   # chóng (overlap) — pretty clear chong but mark uncertain to be safe
               "重排",   # already above
               "孰轻孰重",  # zhòng — clear (pattern hard)
               "千钧之重",  # zhòng — clear
               "稳稳重重",  # zhòng — duplicate
               "前重后轻",  # zhòng — clear
               "不轻不重",  # zhòng — clear
               "一石激起千重浪",  # chóng (layer of waves)
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
