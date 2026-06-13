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
    # batch2b — large双向 chars with clear default + small exception set
    "系": ("del_wrong",  # 体系/院系/各X系 = xì
           {"系上": "del_prim"},  # 系上/系鞋带 = jì
           {"系牢", "系留", "系链", "系民", "系甘", "系吾", "系噶",
            "系由", "系心", "系怀", "系花", "系指"}),  # jì/xì or 粤语 unsure
    "圈": ("del_wrong",  # 朋友圈/一圈/眼圈 = quān
           {"猪圈": "del_prim", "羊圈": "del_prim", "鸡圈": "del_prim",
            "兽圈": "del_prim", "圈舍": "del_prim", "圈养": "del_prim",
            "圈牢": "del_prim", "圈肥": "del_prim", "圈羊": "del_prim",
            "圈马": "del_prim"},  # 猪圈/圈养 = juàn
           {"圈占", "圈住", "圈起", "圈起来", "圈进", "圈拢",
            "圈围", "圈定", "圈选", "圈闭", "圈点"}),  # quān/juān(关住) unsure
    # batch2b-2 — large双向 chars: default 主体 reading + minority exceptions
    "朝": ("del_wrong",  # 清朝/朝鲜/朝南 = cháo; 朝阳* = zhāo (both → drop错读)
           {"朝花夕拾": "del_prim", "朝九晚五": "del_prim", "朝歌": "del_prim",
            "终朝": "del_prim"},  # 真·zhāo words inside the chao→zhao group
           {"朝盛", "朝悦", "朝逐", "朝仍", "朝洪", "朝征"}),  # 生僻/专名 unsure
    "降": ("del_wrong",  # 下降 jiàng
           {"降妖": "del_prim", "降魔": "del_prim", "降妖除魔": "del_prim",
            "伏虎降": "del_prim", "降兵": "del_prim", "降曹": "del_prim",
            "降唐": "del_prim", "降秦": "del_prim", "死不降": "del_prim",
            "宁死不降": "del_prim", "誓死不降": "del_prim", "逼降": "del_prim"},  # 投降 xiáng
           {"降福", "降旨", "降附", "乘降", "降清"}),
    "盛": ("del_wrong",  # 茂盛/盛大 shèng
           {"盛酒": "del_prim", "盛汤": "del_prim", "盛碗": "del_prim",
            "碗盛": "del_prim", "盛到": "del_prim", "内盛": "del_prim",
            "盛过": "del_prim"},  # 盛饭/盛汤 chéng (装)
           {"女体盛", "男体盛"}),
    "模": ("del_wrong",  # 模型/模特 mó
           {"塑料模": "del_prim", "土模": "del_prim", "铸模": "del_prim",
            "钢模": "del_prim", "胎模": "del_prim", "金属模": "del_prim",
            "硬模": "del_prim", "指模": "del_prim", "模铸": "del_prim",
            "塑模": "del_prim", "人模人样": "del_prim", "狗模狗样": "del_prim"},  # 模具/模样 mú
           {"型模", "制模", "造模", "工模", "版模", "波模"}),
    "还": ("del_wrong",  # 还是/还有 hái (副词)
           {"有借有还": "del_prim", "还人情": "del_prim", "还付": "del_prim",
            "退耕还林": "del_prim", "以血还血": "del_prim", "血债血还": "del_prim",
            "还施彼身": "del_prim", "还治其人之身": "del_prim", "借用还": "del_prim",
            "还让出": "del_prim", "还得起": "del_prim", "素还真": "del_prim"},  # 归还 huán
           {"欲说还休", "欲语还休", "还着", "还调", "还吞", "还织",
            "还载", "统还", "还纳", "还于"}),
    "差": ("del_wrong",  # 差别/太差 chà·chā
           {"差人": "del_prim", "差旅": "del_prim", "差派": "del_prim",
            "打差": "del_prim", "趟差": "del_prim", "派差": "del_prim",
            "搞差": "del_prim", "差路": "del_prim", "差任": "del_prim",
            "县差": "del_prim", "差转": "del_prim", "差转台": "del_prim",
            "夫差": "del_prim", "神差": "del_prim", "小差": "del_prim"},  # 出差/差遣 chāi
           {"调差", "仲差", "差劣", "纳差", "差下"}),
    # batch2b-3
    "壳": ("del_wrong",  # 蛋壳/外壳 ké (口语 kept)
           {"介壳": "del_prim"},  # 介壳虫 qiào
           {"壳质", "闭壳龟"}),
    "恶": ("del_wrong",  # 恶劣/凶恶 è
           {"深恶": "del_prim", "嫉恶": "del_prim", "疾恶": "del_prim",
            "喜恶": "del_prim", "所恶": "del_prim"},  # 厌恶 wù
           {"美恶", "恶恶"}),
    "折": ("del_wrong",  # 打折/折叠 zhé
           {"掰折": "del_prim", "拧折": "del_prim", "腿折": "del_prim"},  # 折断 shé
           {"断折", "折不断", "折肉", "莫折念", "折颜", "全部折"}),
    "称": ("del_wrong",  # 称呼/职称 chēng·chèng
           {"不对称性": "del_prim", "宇称": "del_prim", "均称": "del_prim"},  # 相称 chèn
           set()),
    "薄": ("del_wrong",  # 很薄/纸薄 báo (口语 kept)
           {"绵薄": "del_prim", "绵薄之力": "del_prim", "势单力薄": "del_prim",
            "命比纸薄": "del_prim", "薄壁组织": "del_prim", "薄惩": "del_prim",
            "缘薄": "del_prim"},  # 单薄/绵薄 bó
           {"薄冰", "主薄", "薄姑", "电话薄", "留言薄", "薄壳"}),
    "都": ("del_prim",  # 首都/都市/地名 dū (majority here)
           {"都行": "del_wrong", "都还没": "del_wrong", "一切都是": "del_wrong",
            "永远都是": "del_wrong", "全部都是": "del_wrong", "都还不": "del_wrong",
            "连话都": "del_wrong", "想都别想": "del_wrong", "向来都是": "del_wrong",
            "还都不": "del_wrong", "都悔青": "del_wrong", "早晚都是": "del_wrong",
            "都说会": "del_wrong", "想都不想": "del_wrong", "满嘴都是": "del_wrong",
            "一切都在": "del_wrong", "都挑明": "del_wrong", "都受了": "del_wrong"},  # 全都 dōu
           set()),
    # batch2c — 弹 (dan→tan): default 名词 dàn (子弹/弹片), 动词 tán exceptions
    "弹": ("del_wrong",  # 子弹/弹片/弹道 = dàn (名词);拷贝到 tan 是错
           {"会弹": "del_prim", "弹跳": "del_prim", "弹回": "del_prim",
            "弹唱": "del_prim", "弹起": "del_prim", "弹指": "del_prim",
            "弹开": "del_prim", "轻弹": "del_prim", "弹棉花": "del_prim",
            "指弹": "del_prim", "弹起来": "del_prim", "弹回来": "del_prim",
            "自弹": "del_prim", "弹回去": "del_prim", "自弹自唱": "del_prim",
            "手弹": "del_prim", "弹击": "del_prim", "弹牙": "del_prim",
            "弹完": "del_prim", "弹不出": "del_prim", "弹弹琴": "del_prim",
            "练弹": "del_prim", "弹动": "del_prim", "弹舌": "del_prim",
            "弹掉": "del_prim", "弹拨": "del_prim", "弹压": "del_prim",
            "弹不来": "del_prim", "弹过去": "del_prim", "别弹": "del_prim",
            "弹下来": "del_prim", "弹塑性": "del_prim", "弹上去": "del_prim",
            "弹上来": "del_prim", "弹筝": "del_prim", "一弹指": "del_prim",
            "弹过来": "del_prim", "弹拨乐": "del_prim", "弹拨乐器": "del_prim",
            "弹奏乐器": "del_prim", "弹奏着": "del_prim", "弹奏出": "del_prim",
            "弹来弹去": "del_prim", "古调独弹": "del_prim", "弹拔乐器": "del_prim",
            "弹下去": "del_prim", "弹指神功": "del_prim", "弹指如飞": "del_prim",
            "弹指光阴": "del_prim", "弹跳板": "del_prim", "弹升": "del_prim",
            "弹花机": "del_prim", "弹指一挥间": "del_prim", "纠弹": "del_prim",
            "四面弹": "del_prim", "手弹式": "del_prim", "弹涂鱼": "del_prim",
            # tech popup-tán
            "弹出": "del_prim", "弹出来": "del_prim", "弹出去": "del_prim",
            "弹出式": "del_prim", "弹窗": "del_prim"},
           {"弹弹", "弹屏", "弹下", "弹法", "接弹", "弹挡", "几弹",
            "弹炮", "弹炸", "合弹", "敢弹", "程弹", "盖弹", "险弹",
            "乌珠弹", "拒弹", "蔡惟弹", "布依弹", "别力弹", "枪爻弹雨",
            "弹引", "弹入", "弹妥", "弹合", "借弹"}),
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

with open(os.path.join(HERE, "to_delete_batch2.tsv"), "w", encoding="utf-8") as fh:
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
