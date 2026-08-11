#!/usr/bin/env python3
"""Scan core/crates/inputx-pinyin/data/library.tsv for AA-reduplication
entries (2-char words where char[0] == char[1] and both are CJK).

Emit `candidates.tsv` sorted by freq desc, with auto-classification hint
(name / onomatopoeia / verb-redup / noun-redup / unknown) — heuristic
only, NOT authoritative.

Usage:
    python3 docs/pinyin-AA-redup-sweep-2026-06-21/find_aa_redups.py
"""
import sys
from pathlib import Path
from collections import Counter

REPO = Path(__file__).resolve().parents[2]
LIB = REPO / "core/crates/inputx-pinyin/data/library.tsv"
OUT = Path(__file__).parent / "candidates.tsv"

# Single-character names commonly used as reduplicated nicknames in
# Chinese (汉语昵称 reduplication). Heuristic — high precision, low
# recall (we miss less common ones; that's fine, batch reviewer will
# catch them).
COMMON_NAME_CHARS = set(
    "薇娟莉莎丽颖珂琪婷婕媛雯萱琳楠瑶玉璇瑛芳兰菲妍娜雪琴" +
    "凡帆范帅亮强伟豪聪杰浩刚刘梁波勇杨明涛康林春鹏阳鑫" +
    "嘟豆兜囡囡乖萌咪猫狗虎牛宝贝贝美" +
    "翰翔哲玮赫熙昊昕辰晨"
)

# Verb roots whose AA form is real verb-reduplication ("看看", "想想",
# "听听" — try-X / a-bit-of-X / V-V softening pattern). High precision.
COMMON_VERB_ROOTS = set(
    "看想听说读写做走跑跳坐睡吃喝喊叫笑哭问答尝试摸碰打骂等" +
    "聊谈讲念猜算找拿放送拉推抓拍翻找数瞧瞅瞧" +
    "缓缓慢慢动动转转摇摇晃晃摆摆挪挪挤挤搞搞试试改改" +
    "走走停停跑跑歇歇玩玩"
)

# Common onomatopoeia (拟声) — AA reduplication legitimate.
ONOMATOPOEIA = set("呼呵哈嘿嘻嘎咯啧叽咕咚咔嗒嗒哒铛喵汪喔嗨呜唔嗯哎")

# Adjective roots whose AA form is real (慢慢/快快/好好/深深/淡淡/...)
COMMON_ADJ_ROOTS = set(
    "慢快好深浅淡浓厚薄软硬高低长短大小多少远近热冷暖凉" +
    "细粗轻重柔猛清浊静闹稀密暗亮明暗" +
    "缓急甜苦酸辣咸鲜香臭脏净" +
    "圆扁方尖弯直老嫩生熟糟糟"
)


def classify(word: str) -> str:
    """Return a hint string. Heuristic — batch reviewer adjudicates."""
    if len(word) != 2 or word[0] != word[1]:
        return "not_aa"
    c = word[0]
    if c in COMMON_NAME_CHARS:
        return "name"
    if c in COMMON_VERB_ROOTS:
        return "verb_redup"
    if c in COMMON_ADJ_ROOTS:
        return "adj_redup"
    if c in ONOMATOPOEIA:
        return "onomatopoeia"
    return "unknown"


def main() -> int:
    entries = []
    with LIB.open() as f:
        for line in f:
            parts = line.rstrip("\n").split("\t")
            if len(parts) < 4:
                continue
            code, word, freq, src = parts[0], parts[1], parts[2], parts[3]
            if len(word) == 2 and word[0] == word[1] and "一" <= word[0] <= "鿿":
                entries.append((code, word, int(freq), src))

    entries.sort(key=lambda e: -e[2])

    with OUT.open("w") as f:
        f.write("# code\tword\tfreq\tsource\tclass_hint\n")
        for code, word, freq, src in entries:
            f.write(f"{code}\t{word}\t{freq}\t{src}\t{classify(word)}\n")

    # Summary
    cls_counter = Counter(classify(e[1]) for e in entries)
    bucket_counter = Counter()
    for _, _, freq, _ in entries:
        if freq >= 30000:
            bucket_counter["A_safe_(>=30k)"] += 1
        elif freq >= 25000:
            bucket_counter["B_likely_safe_(25-30k)"] += 1
        elif freq >= 20000:
            bucket_counter["C_mixed_(20-25k)"] += 1
        elif freq >= 15000:
            bucket_counter["D_mixed_(15-20k)"] += 1
        elif freq >= 10000:
            bucket_counter["E_noisy_(10-15k)"] += 1
        else:
            bucket_counter["F_very_noisy_(<10k)"] += 1

    print(f"wrote {OUT.relative_to(REPO)} ({len(entries)} rows)")
    print("\n=== freq bucket distribution ===")
    for k in sorted(bucket_counter):
        print(f"  {k:30s} {bucket_counter[k]:>5}")
    print("\n=== class_hint distribution (heuristic only) ===")
    for k, v in cls_counter.most_common():
        print(f"  {k:20s} {v:>5}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
