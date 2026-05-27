#!/usr/bin/env python3
"""Generate Japanese counter-word entries (TSV) for jp_jukugo merge.

Output is `tools/scoring/data/supplemental/jp_counters_v1.tsv`. Each row is
`<kanji>\t<romaji>\t<freq>` matching the format consumed by
`tools/jp/build_jukugo_rs.py`.

Covers 1-10 × the common counters, including the irregular sound changes
(ippon / sanbon / roppon, sanbiki / roppiki, ippun / sanpun, hitori / futari,
tsuitachi / futsuka / mikka / yokka / itsuka / muika / nanoka / youka / kokonoka
/ tooka) that real-world JP IME users hit constantly. Pure-yomi input path —
no digit-key hijacking.

Run from repo root:  python3 tools/jp/build_counters_tsv.py
"""

from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
OUT = ROOT / "tools/scoring/data/supplemental/jp_counters_v1.tsv"


# 数 1-10. For each digit: the bare kanji for the number (used in counter
# combinations like 1個 → 一個 — we always render with the kanji digit).
DIGIT_KANJI = {
    1: "一",
    2: "二",
    3: "三",
    4: "四",
    5: "五",
    6: "六",
    7: "七",
    8: "八",
    9: "九",
    10: "十",
}


def _regular_reading(num: int, counter_yomi: str) -> str | None:
    """Regular (no sound change) reading: <digit_yomi> + <counter_yomi>."""
    # ichi / ni / san / yon / go / roku / nana / hachi / kyuu / juu
    digit = {
        1: "ichi",
        2: "ni",
        3: "san",
        4: "yon",
        5: "go",
        6: "roku",
        7: "nana",
        8: "hachi",
        9: "kyuu",
        10: "juu",
    }.get(num)
    if digit is None:
        return None
    return digit + counter_yomi


# Per-counter table. Each entry: (counter_kanji, counter_yomi_default,
# overrides_dict {num: [readings...]}). When a number is in `overrides`, those
# readings REPLACE the regular one (yon/nana variants get added explicitly).
# When not overridden, we emit the regular `<digit>+<yomi>`.
#
# Multiple readings per number → we emit one row per reading (all sharing the
# same kanji output, e.g. 一個 has only the `ikko` reading; 三個 has `sanko`).
COUNTERS: list[tuple[str, str, dict[int, list[str]]]] = [
    # 個 — ko sound change for 1/6/8/10
    ("個", "ko", {
        1: ["ikko"],
        6: ["rokko"],
        8: ["hakko"],
        10: ["jukko", "jikko"],
    }),
    # 人 — irregular 1/2; 4 = yonin (NOT shinin)
    ("人", "nin", {
        1: ["hitori"],
        2: ["futari"],
        4: ["yonin"],
        7: ["shichinin", "nananin"],
    }),
    # 日 — wholesale yamato 1-10 (tsuitachi/futsuka/...)
    ("日", "nichi", {
        1: ["tsuitachi", "ichinichi"],
        2: ["futsuka"],
        3: ["mikka"],
        4: ["yokka"],
        5: ["itsuka"],
        6: ["muika"],
        7: ["nanoka"],
        8: ["youka"],
        9: ["kokonoka"],
        10: ["tooka"],
    }),
    # 本 — ippon / sanbon / yonhon / roppon / happon / juppon
    ("本", "hon", {
        1: ["ippon"],
        3: ["sanbon"],
        6: ["roppon"],
        8: ["happon"],
        10: ["juppon", "jippon"],
    }),
    # 枚 — regular
    ("枚", "mai", {}),
    # 匹 — ippiki / sanbiki / roppiki / happiki / juppiki
    ("匹", "hiki", {
        1: ["ippiki"],
        3: ["sanbiki"],
        6: ["roppiki"],
        8: ["happiki"],
        10: ["juppiki", "jippiki"],
    }),
    # 頭 — ittou / juttou
    ("頭", "tou", {
        1: ["ittou"],
        8: ["hattou"],
        10: ["juttou", "jittou"],
    }),
    # 羽 (birds) — ichiwa / sanba (or sanwa) / roppa / happa / juppa
    ("羽", "wa", {
        3: ["sanba", "sanwa"],
        6: ["roppa", "rokuwa"],
        8: ["happa", "hachiwa"],
        10: ["juppa", "juuwa"],
    }),
    # 回 — ikkai / rokkai / hakkai / jukkai
    ("回", "kai", {
        1: ["ikkai"],
        6: ["rokkai"],
        8: ["hakkai"],
        10: ["jukkai", "jikkai"],
    }),
    # 階 (floors) — ikkai / sangai / rokkai / hakkai / jukkai
    ("階", "kai", {
        1: ["ikkai"],
        3: ["sangai", "sankai"],
        6: ["rokkai"],
        8: ["hakkai"],
        10: ["jukkai", "jikkai"],
    }),
    # 週 — isshuu / hasshuu / jusshuu
    ("週", "shuu", {
        1: ["isshuu"],
        8: ["hasshuu"],
        10: ["jusshuu", "jisshuu"],
    }),
    # 週間
    ("週間", "shuukan", {
        1: ["isshuukan"],
        8: ["hasshuukan"],
        10: ["jusshuukan", "jisshuukan"],
    }),
    # 月 (calendar month) — gatsu, 4=shigatsu, 7=shichigatsu, 9=kugatsu
    ("月", "gatsu", {
        4: ["shigatsu"],
        7: ["shichigatsu"],
        9: ["kugatsu"],
    }),
    # ヶ月 (number of months) — kagetsu, ikkagetsu / rokkagetsu / hakkagetsu / jukkagetsu
    ("ヶ月", "kagetsu", {
        1: ["ikkagetsu"],
        6: ["rokkagetsu"],
        8: ["hakkagetsu", "hachikagetsu"],
        10: ["jukkagetsu", "jikkagetsu"],
    }),
    # 年 — nen, 4=yonen, 7=shichinen/nananen, 9=kyuunen
    ("年", "nen", {
        4: ["yonen"],
        7: ["shichinen", "nananen"],
    }),
    # 年生 (school grade)
    ("年生", "nensei", {
        4: ["yonensei"],
        7: ["shichinensei", "nananensei"],
    }),
    # 年間 (year duration)
    ("年間", "nenkan", {
        4: ["yonenkan"],
    }),
    # 時 (clock hour) — 4=yoji, 7=shichiji, 9=kuji
    ("時", "ji", {
        4: ["yoji"],
        7: ["shichiji"],
        9: ["kuji"],
    }),
    # 時間 (duration in hours)
    ("時間", "jikan", {
        4: ["yojikan"],
        7: ["shichijikan", "nanajikan"],
        9: ["kujikan"],
    }),
    # 分 (minute) — ippun / sanpun / yonpun / roppun / happun / juppun
    ("分", "fun", {
        1: ["ippun"],
        3: ["sanpun"],
        4: ["yonpun", "yonfun"],
        6: ["roppun"],
        8: ["happun", "hachifun"],
        10: ["juppun", "jippun"],
    }),
    # 分間
    ("分間", "funkan", {
        1: ["ippunkan"],
        3: ["sanpunkan"],
        6: ["roppunkan"],
        8: ["happunkan"],
        10: ["juppunkan"],
    }),
    # 秒 — regular
    ("秒", "byou", {}),
    # 秒間
    ("秒間", "byoukan", {}),
    # 号 (issue/room/route number) — regular
    ("号", "gou", {}),
    # 番 (number/ordinal) — regular
    ("番", "ban", {}),
    # 番目 (ordinal "Nth")
    ("番目", "banme", {}),
    # 件 — ikken / rokken / hakken / jukken
    ("件", "ken", {
        1: ["ikken"],
        6: ["rokken"],
        8: ["hakken"],
        10: ["jukken", "jikken"],
    }),
    # 度 (degrees / times) — regular
    ("度", "do", {}),
    # 倍 (multiplier) — regular; ichibai usually omitted but include
    ("倍", "bai", {}),
    # 杯 (cup/glass) — ippai / sanbai / roppai / happai / juppai
    ("杯", "hai", {
        1: ["ippai"],
        3: ["sanbai"],
        6: ["roppai"],
        8: ["happai"],
        10: ["juppai", "jippai"],
    }),
    # 冊 (book) — issatsu / hassatsu / jussatsu
    ("冊", "satsu", {
        1: ["issatsu"],
        8: ["hassatsu"],
        10: ["jussatsu", "jissatsu"],
    }),
    # 台 (machines/cars) — regular
    ("台", "dai", {}),
    # 軒 (houses) — ikken / rokken / hakken / jukken (same shape as 件)
    ("軒", "ken", {
        1: ["ikken"],
        6: ["rokken"],
        8: ["hakken"],
        10: ["jukken", "jikken"],
    }),
    # 部 (parts / departments / copies) — regular
    ("部", "bu", {}),
    # 組 (groups) — hitokumi / futakumi / mikumi / ... yamato-ish for 1-3
    ("組", "kumi", {
        1: ["hitokumi", "ichikumi"],
        2: ["futakumi", "nikumi"],
        3: ["mikumi", "sankumi"],
    }),
    # 種 / 種類 (kinds) — isshu / hasshu / jusshu
    ("種", "shu", {
        1: ["isshu"],
        8: ["hasshu"],
        10: ["jusshu", "jisshu"],
    }),
    ("種類", "shurui", {
        1: ["isshurui"],
        8: ["hasshurui"],
        10: ["jusshurui", "jisshurui"],
    }),
    # 章 (chapter) — isshou / hasshou / jusshou
    ("章", "shou", {
        1: ["isshou"],
        8: ["hasshou"],
        10: ["jusshou", "jisshou"],
    }),
    # 学期 (semester)
    ("学期", "gakki", {
        1: ["ichigakki"],
        2: ["nigakki"],
        3: ["sangakki"],
    }),
    # 学年 (school year)
    ("学年", "gakunen", {}),
    # 校時 (class period at school)
    ("時限目", "jigenme", {
        4: ["yojigenme"],
        7: ["shichijigenme", "nanajigenme"],
        9: ["kujigenme"],
    }),
    # 回目
    ("回目", "kaime", {
        1: ["ikkaime"],
        6: ["rokkaime"],
        8: ["hakkaime"],
        10: ["jukkaime", "jikkaime"],
    }),
    # 日目 (Nth day) — uses yamato-yomi for 1-10 like 日
    ("日目", "nichime", {
        1: ["tsuitachime", "ichinichime"],
        2: ["futsukame"],
        3: ["mikkame"],
        4: ["yokkame"],
        5: ["itsukame"],
        6: ["muikame"],
        7: ["nanokame"],
        8: ["youkame"],
        9: ["kokonokame"],
        10: ["tookame"],
    }),
    # 週目 (Nth week)
    ("週目", "shuume", {
        1: ["isshuume"],
        8: ["hasshuume"],
        10: ["jusshuume", "jisshuume"],
    }),
    # 月目 (Nth month)
    ("月目", "kagetsume", {
        1: ["ikkagetsume"],
        6: ["rokkagetsume"],
        8: ["hakkagetsume"],
        10: ["jukkagetsume", "jikkagetsume"],
    }),
    # 年目 (Nth year)
    ("年目", "nenme", {
        4: ["yonenme"],
        7: ["shichinenme", "nananenme"],
    }),
    # 名 (person, formal) — regular
    ("名", "mei", {}),
    # 円 (yen) — regular; ichien, ni-en, san-en... 4=yoen sometimes but usually yo-en spoken; freq mostly written
    ("円", "en", {
        4: ["yoen"],
    }),
    # 歳 / 才 (age) — issai / hassai / jussai
    ("歳", "sai", {
        1: ["issai"],
        8: ["hassai"],
        10: ["jussai", "jissai"],
        20: ["hatachi"],  # special: 二十歳 → はたち
    }),
    ("才", "sai", {
        1: ["issai"],
        8: ["hassai"],
        10: ["jussai", "jissai"],
    }),
    # 階段 — too composite for counter set; skip
]


# Freq band: counters are everyday vocabulary. We pick a baseline by counter
# class so the top-frequency particles (1個/1人/1日/1本) land above more
# specialized counters (1軒/1部). Freqs roughly mirror real usage.
COUNTER_FREQ_BASE = {
    "個": 88, "人": 92, "日": 90, "月": 88, "年": 88, "時": 88, "分": 85, "秒": 78,
    "本": 80, "枚": 78, "匹": 70, "頭": 60, "羽": 55, "回": 85, "階": 78, "週": 75,
    "週間": 75, "時間": 88, "分間": 70, "秒間": 60, "号": 70, "番": 80, "番目": 78,
    "件": 75, "度": 80, "倍": 72, "杯": 72, "冊": 70, "台": 78, "軒": 65, "部": 68,
    "組": 65, "種": 65, "種類": 72, "章": 60, "学期": 60, "学年": 60, "時限目": 50,
    "回目": 70, "日目": 68, "週目": 60, "月目": 60, "年目": 60, "名": 70, "円": 80,
    "ヶ月": 80, "年生": 72, "年間": 75, "歳": 80, "才": 75,
}
# Slight per-number adjustment: 1/2/3 most common, 4-7 mid, 8-10 less.
NUM_FREQ_DELTA = {1: 5, 2: 3, 3: 2, 4: 0, 5: 0, 6: -2, 7: -3, 8: -3, 9: -5, 10: -2, 20: -5}


def emit_counter(counter_kanji: str, counter_yomi: str,
                 overrides: dict[int, list[str]]) -> list[tuple[str, str, int]]:
    rows: list[tuple[str, str, int]] = []
    base = COUNTER_FREQ_BASE.get(counter_kanji, 60)
    # Iterate numbers we want to emit. Default 1-10 plus any extras present in
    # overrides (e.g. 20 for 二十歳).
    nums = sorted(set(list(range(1, 11)) + list(overrides.keys())))
    for n in nums:
        digit_k = DIGIT_KANJI.get(n) or (
            "二十" if n == 20 else None
        )
        if digit_k is None:
            continue
        kanji_word = f"{digit_k}{counter_kanji}"
        freq = base + NUM_FREQ_DELTA.get(n, -5)

        if n in overrides:
            for r in overrides[n]:
                rows.append((kanji_word, r, freq))
        else:
            r = _regular_reading(n, counter_yomi)
            if r is not None:
                rows.append((kanji_word, r, freq))
    return rows


# Yamato 1-10 つ-counter is special: hitotsu / futatsu / mittsu / yottsu /
# itsutsu / muttsu / nanatsu / yattsu / kokonotsu / too.
YAMATO_TSU = [
    ("一つ", "hitotsu", 95),
    ("二つ", "futatsu", 92),
    ("三つ", "mittsu", 90),
    ("四つ", "yottsu", 85),
    ("五つ", "itsutsu", 80),
    ("六つ", "muttsu", 70),
    ("七つ", "nanatsu", 70),
    ("八つ", "yattsu", 65),
    ("九つ", "kokonotsu", 60),
    ("十", "too", 75),  # `tou`/`too` both valid; we keep `too`
    # …目 form
    ("一つ目", "hitotsume", 78),
    ("二つ目", "futatsume", 75),
    ("三つ目", "mittsume", 70),
    ("四つ目", "yottsume", 65),
    ("五つ目", "itsutsume", 60),
]

# Ordinals — 第N 系列. Common up to 第十.
DAI_ORDINALS: list[tuple[str, str, int]] = []
for n, k in DIGIT_KANJI.items():
    yomi_digit = {
        1: "ichi", 2: "ni", 3: "san", 4: "yon", 5: "go",
        6: "roku", 7: "nana", 8: "hachi", 9: "kyuu", 10: "juu",
    }[n]
    DAI_ORDINALS.append((f"第{k}", f"dai{yomi_digit}", 75 + NUM_FREQ_DELTA.get(n, 0)))
    DAI_ORDINALS.append((f"第{k}回", f"dai{yomi_digit}kai", 60 + NUM_FREQ_DELTA.get(n, 0)))
    DAI_ORDINALS.append((f"第{k}章", f"dai{yomi_digit}shou", 55 + NUM_FREQ_DELTA.get(n, 0)))


def main() -> int:
    rows: list[tuple[str, str, int]] = []
    seen: set[tuple[str, str]] = set()

    for kanji, yomi, overrides in COUNTERS:
        for w, r, f in emit_counter(kanji, yomi, overrides):
            if (r, w) in seen:
                continue
            seen.add((r, w))
            rows.append((w, r, f))

    for w, r, f in YAMATO_TSU + DAI_ORDINALS:
        if (r, w) in seen:
            continue
        seen.add((r, w))
        rows.append((w, r, f))

    # Sort: freq desc, then reading, then word — matches build_jukugo_rs's
    # sort order so cat/append + regenerate is order-stable.
    rows.sort(key=lambda r: (-r[2], r[1], r[0]))

    OUT.parent.mkdir(parents=True, exist_ok=True)
    with OUT.open("w") as f:
        f.write("# Japanese counter-word table — generated by tools/jp/build_counters_tsv.py\n")
        f.write("# Format: kanji<TAB>romaji<TAB>freq.  Merged into jp_jukugo_v1.tsv by hand or pipeline.\n")
        for w, r, freq in rows:
            f.write(f"{w}\t{r}\t{freq}\n")

    print(f"wrote {OUT} ({len(rows)} entries)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
