#!/usr/bin/env python3
"""
batch3 RISKY 助词 sub-word noise classifier.

Input: risky_detail.md (1845 unique words含助词地/得/的/了/谁/那).
Output: batch3_helper_classify.tsv with each row labeled.

Categories:
  - SPURIOUS  : V/Adv/Adj/Phrase + 助词 伪词 (e.g. 哀求地 / 静静地 / 高兴得 / 看了 / 干了)
                → 词不该存在 (无论 reading), 双向都该删
  - READING_WRONG : 词合法 + pypinyin primary 读音方向反 (e.g. 各地 di→de:
                pypinyin 给 di primary 但 de 是对的助词?实际"各地"=各+地 noun 还是 di,这种实际是合法词,不该删 — 应 UNCERTAIN 归)
                Actually for 助词 case: reading-wrong 跟 sub-word noise 高度重叠 —
                如"静静地"读 de (轻声助词) vs di (地球地理): 静静地 既是 sub-word
                noise 又是 reading-wrong (de copy 删除既清理 noise 也修正方向)
  - LEGITIMATE : 合法 compound 含助词字 (e.g. 目的 / 得意 / 罢了 / 极了 /
                了不起 / 大地 / 极地 / 各地 / 接地)
  - UNCERTAIN  : 助词在中/前 (谁/那), 或 prefix 不明 → 留 user audit

Heuristic:
  1. 助词在末位 (地/得/的/了):
     a. word == 助词 single char → LEGITIMATE (单字助词合法 entry)
     b. prefix == 1 char →
        - 高频常见 N+助词 (各/大/极/某/此/接/营/坐 etc) → LEGITIMATE
        - 其他 → UNCERTAIN
     c. prefix == 2 chars 重叠 (XX) →
        - 重叠副词 + 助词 (静静/默默/悄悄/深深/慢慢 etc) → SPURIOUS
     d. prefix == 2 chars 非重叠 →
        - prefix 看着像 V/Adj (高兴/快乐/认真) → SPURIOUS (V+助词)
        - prefix 看着像 N (中心/边界) → LIKELY_LEGITIMATE 但 lean UNCERTAIN
     e. prefix >= 3 chars → 几乎肯定 SPURIOUS (long V phrase + 助词)
  2. 助词在非末位 (谁/那):
     - 默认 UNCERTAIN (需 user audit)

注:这是 best-effort heuristic, 最终 ground truth 仍需 native-speaker audit.
"""
import re, sys
from collections import Counter

HERE = "."

# Read risky_detail.md and extract all rows
rows = []  # (char_source, wrong_code, word, freq, primary)
current_char = None
with open(f"{HERE}/risky_detail.md", encoding='utf-8') as f:
    for ln in f:
        m = re.match(r'^## (\S+) ', ln)
        if m:
            current_char = m.group(1)
            continue
        if current_char and ln.startswith('| ') and not ln.startswith('| wrong_code') and not ln.startswith('|---'):
            parts = [p.strip() for p in ln.strip().strip('|').split('|')]
            if len(parts) >= 4:
                wrong_code, word, freq_str, primary = parts[0], parts[1], parts[2], parts[3]
                try:
                    freq = int(freq_str)
                except ValueError:
                    continue
                rows.append((current_char, wrong_code, word, freq, primary))

print(f"loaded {len(rows)} rows across {len(set(r[0] for r in rows))} chars", file=sys.stderr)

# Common N + 助词 high-confidence LEGITIMATE list (manual whitelist)
N_HELPER_LEGITIMATE = {
    # 单字 N + 地 = noun (地理含义)
    "各地", "大地", "极地", "某地", "此地", "该地", "本地", "外地",
    "土地", "实地", "原地", "异地", "圣地", "目的地", "境地", "在地",
    "接地", "营地", "工地", "陵地", "草地", "腹地", "盆地", "沼地",
    "胜地", "园地", "佳地", "古地", "属地", "高地", "低地", "前地",
    "后地", "南地", "北地", "东地", "西地", "中地", "新地", "旧地",
    # 单字 N + 的 noun (目的/标的)
    "目的", "标的", "众矢之的",
    # 单字 + 得 verb 合法 (得意/得力/得罪/得逞/获得/取得 etc)
    "得意", "得罪", "得力", "得逞", "得到", "得失", "得手", "得宜",
    "得当", "得了", "得分", "得过", "得人心", "得空", "得票", "得益",
    "获得", "取得", "求得", "记得", "懂得", "认得", "晓得", "舍得",
    "值得", "使得", "免得", "怪不得", "舍不得", "对得起", "对不起",
    "巴不得", "见不得", "经得起", "比不得", "怪不得", "划得来",
    # 了 在末位作动词或 idiom (了不起/不得了/不了了之/罢了)
    "了不起", "了得", "了断", "了结", "了解", "了了",
    "罢了", "极了", "对了", "好了", "完了", "够了", "成了", "败了",
    "没了", "算了", "干了", "完事了",
    # 谁/那 高频合法
    "谁知", "谁也", "谁说", "谁让", "谁的", "谁敢", "谁还", "无谁",
    "那个", "那些", "那么", "那里", "那边", "那时", "那阵", "那样",
}

# 高置信 V/Adj/Adv 列表 (人工补 — 给 prefix == 2 chars 用)
V_ADJ_HELPERS = {
    # V (动词) — pure V 不该跟助词构成 word
    "哀求", "祈求", "请求", "要求", "央求", "请教", "答应", "解释",
    "回答", "说明", "证明", "强调", "表示", "宣布", "通知", "告知",
    "等待", "期待", "盼望", "希望", "祝愿", "想到", "看到", "听到",
    "拿到", "得到", "找到", "捡到", "感到", "想着", "想着的", "考虑到",
    "答应到", "决定", "决意", "立志", "立誓",
    # Adj — 形容词
    "高兴", "快乐", "认真", "仔细", "聪明", "勤奋", "活泼", "温柔",
    "和蔼", "可爱", "可亲", "可怜", "可怕", "可恶", "美丽", "漂亮",
    "丑陋", "胆小", "胆大", "勇敢", "懦弱", "坚强", "脆弱", "坚定",
    # Adv — 副词
    "格外", "特别", "尤其", "非常", "十分", "万分", "相当", "颇为",
    "相对", "略微", "稍微", "稍稍",
}

def classify(char_src, wrong_code, word, freq, primary):
    """Return (verdict, confidence_note)."""
    if len(word) == 0:
        return ("UNCERTAIN", "empty")
    last = word[-1]

    # 助词在末位
    if last in "地得的了":
        if len(word) == 1:
            return ("LEGITIMATE", f"single-char 助词 '{last}'")
        prefix = word[:-1]

        # Manual whitelist check first
        if word in N_HELPER_LEGITIMATE:
            return ("LEGITIMATE", "whitelist N+助词")

        # 重叠 prefix
        if len(prefix) >= 2 and prefix[0] == prefix[1]:
            return ("SPURIOUS", f"重叠副词 '{prefix[0]}{prefix[0]}' + 助词")
        if len(prefix) == 4 and prefix[0] == prefix[1] and prefix[2] == prefix[3]:
            return ("SPURIOUS", "AABB 重叠副词 + 助词")
        if len(prefix) == 4 and prefix[0] == prefix[2] and prefix[1] == prefix[3]:
            return ("SPURIOUS", "ABAB 重叠副词 + 助词")

        # prefix in V/Adj/Adv whitelist
        if prefix in V_ADJ_HELPERS:
            return ("SPURIOUS", f"V/Adj '{prefix}' + 助词")

        # length-based:
        if len(prefix) == 1:
            return ("UNCERTAIN", f"single-char prefix '{prefix}' + 助词 (likely legitimate N+助词,但 ambiguous)")
        if len(prefix) == 2:
            return ("UNCERTAIN", f"2-char prefix '{prefix}' + 助词 (V+助词 or 2-char N+助词)")
        if len(prefix) >= 3:
            return ("LIKELY_SPURIOUS", f">=3-char prefix '{prefix}' + 助词 (long phrase + 助词)")

    # 助词在非末位 (谁/那)
    if char_src in ("谁", "那"):
        if word in N_HELPER_LEGITIMATE:
            return ("LEGITIMATE", "whitelist")
        return ("UNCERTAIN", "谁/那 in non-末位")

    return ("UNCERTAIN", "default")

# Run + tally
results = []
tally = Counter()
for char_src, wrong_code, word, freq, primary in rows:
    verdict, note = classify(char_src, wrong_code, word, freq, primary)
    results.append((char_src, wrong_code, word, freq, primary, verdict, note))
    tally[verdict] += 1

# Print summary to stderr
print(f"\n=== verdict tally ({len(rows)} rows) ===", file=sys.stderr)
for v, n in tally.most_common():
    pct = 100 * n / len(rows)
    print(f"  {v:18s}: {n:5d}  ({pct:5.1f}%)", file=sys.stderr)

# Tally by char source
print(f"\n=== verdict by source char ===", file=sys.stderr)
by_char = {}
for char_src, _, _, _, _, verdict, _ in results:
    by_char.setdefault(char_src, Counter())[verdict] += 1
for ch in "地得的了谁那":
    if ch in by_char:
        print(f"  {ch}: {dict(by_char[ch])}", file=sys.stderr)

# Output TSV
print("char\twrong_code\tword\tfreq\tprimary\tverdict\tnote")
for r in results:
    print('\t'.join(str(x) for x in r))
