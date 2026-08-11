"""
KANJIDIC2 backfill v2 — fill nihongo single-kanji readings for every
常用漢字 (grade 1-8) + 人名用漢字 (grade 9-10) NOT already present as a
single kanji in nihongo library.tsv. Follow-up to the 2026-06-08
backfill (which only covered the 217 swept Shinjitai chars) — user
2026-06-09: nihongo single-kanji coverage was only ~43% of 常用漢字
(sakai→堺 etc. unreachable).

Reading source: KANJIDIC2 (EDRDG, /tmp/kanjidic2.xml.gz). Per char:
  - ja_on  (katakana → hiragana) → romaji
  - ja_kun (hiragana, strip okurigana ".える" + "-" markers) → romaji
  - nanori (name readings) DROPPED
kana→romaji is Hepburn, round-trip-verified against the engine's own
romaji table (examples/kana_roundtrip.rs).

freq: differentiated by KANJIDIC2 <misc><freq> (news-corpus rank 1-2501,
1 = most common). Mapped to a conservative library band so common chars
(永/映/液) sit reasonably while never overpowering corpus-attested
entries; chars with no freq rank (rare / many 人名用) get the floor.
  lib_freq = 10 + round(50 * (2500 - min(rank,2500)) / 2500)   # 60..10
  no rank  → 8

Run:
  python3 docs/nihongo-kanji-backfill-2026-06-09/gen_jouyou_jinmei.py          # dry-run
  python3 docs/nihongo-kanji-backfill-2026-06-09/gen_jouyou_jinmei.py --apply
"""
import gzip
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

APPLY = "--apply" in sys.argv
KD = "/tmp/kanjidic2.xml.gz"
NLIB = Path("core/crates/inputx-nihongo/data/library.tsv")
AUDIT = Path("docs/nihongo-kanji-backfill-2026-06-09")

# ---- Hepburn kana→romaji (round-trip-correct; same table as v1) ----
DIGRAPH = {
    "きゃ": "kya", "きゅ": "kyu", "きょ": "kyo", "ぎゃ": "gya", "ぎゅ": "gyu", "ぎょ": "gyo",
    "しゃ": "sha", "しゅ": "shu", "しょ": "sho", "じゃ": "ja", "じゅ": "ju", "じょ": "jo",
    "ちゃ": "cha", "ちゅ": "chu", "ちょ": "cho", "にゃ": "nya", "にゅ": "nyu", "にょ": "nyo",
    "ひゃ": "hya", "ひゅ": "hyu", "ひょ": "hyo", "びゃ": "bya", "びゅ": "byu", "びょ": "byo",
    "ぴゃ": "pya", "ぴゅ": "pyu", "ぴょ": "pyo", "みゃ": "mya", "みゅ": "myu", "みょ": "myo",
    "りゃ": "rya", "りゅ": "ryu", "りょ": "ryo",
}
MONO = {
    "あ": "a", "い": "i", "う": "u", "え": "e", "お": "o",
    "か": "ka", "き": "ki", "く": "ku", "け": "ke", "こ": "ko",
    "が": "ga", "ぎ": "gi", "ぐ": "gu", "げ": "ge", "ご": "go",
    "さ": "sa", "し": "shi", "す": "su", "せ": "se", "そ": "so",
    "ざ": "za", "じ": "ji", "ず": "zu", "ぜ": "ze", "ぞ": "zo",
    "た": "ta", "ち": "chi", "つ": "tsu", "て": "te", "と": "to",
    "だ": "da", "ぢ": "di", "づ": "du", "で": "de", "ど": "do",
    "な": "na", "に": "ni", "ぬ": "nu", "ね": "ne", "の": "no",
    "は": "ha", "ひ": "hi", "ふ": "fu", "へ": "he", "ほ": "ho",
    "ば": "ba", "び": "bi", "ぶ": "bu", "べ": "be", "ぼ": "bo",
    "ぱ": "pa", "ぴ": "pi", "ぷ": "pu", "ぺ": "pe", "ぽ": "po",
    "ま": "ma", "み": "mi", "む": "mu", "め": "me", "も": "mo",
    "や": "ya", "ゆ": "yu", "よ": "yo",
    "ら": "ra", "り": "ri", "る": "ru", "れ": "re", "ろ": "ro",
    "わ": "wa", "を": "wo",
}
VOWELS = set("aiueo")


def kana_to_romaji(kana):
    out = []
    i, n = 0, len(kana)
    while i < n:
        if i + 1 < n and kana[i:i + 2] in DIGRAPH:
            out.append(DIGRAPH[kana[i:i + 2]]); i += 2; continue
        c = kana[i]
        if c == "っ":
            nxt = kana_to_romaji(kana[i + 1:])
            if not nxt:
                return None
            return "".join(out) + nxt[0] + nxt
        if c == "ん":
            nxt = kana_to_romaji(kana[i + 1:]) if i + 1 < n else ""
            if nxt is None:
                return None
            out.append("nn" if (nxt and nxt[0] in VOWELS | {"y"}) else "n")
            out.append(nxt); return "".join(out)
        if c in MONO:
            out.append(MONO[c]); i += 1; continue
        return None
    return "".join(out)


def kata2hira(s):
    return "".join(chr(ord(c) - 0x60) if 0x30A1 <= ord(c) <= 0x30F6 else c for c in s)


# existing single kanji (any code, type=kanji) → skip
have = set()
for l in NLIB.read_text(encoding="utf-8").splitlines():
    if l and not l.startswith("#"):
        p = l.split("\t")
        if len(p) >= 3 and len(p[1]) == 1 and p[2] == "kanji":
            have.add(p[1])

root = ET.fromstring(gzip.open(KD).read())
rows, seen, codes, code_kana = [], set(), set(), {}
skipped = []
n_chars = 0
for ch in root.findall("character"):
    lit = ch.findtext("literal")
    grade = ch.findtext("./misc/grade")
    if grade is None or int(grade) > 10:
        continue          # only 常用 (1-8) + 人名用 (9-10)
    if lit in have:
        continue
    # Conservative uniform freq: KANJIDIC2 <freq> is a WHOLE-CHAR rank,
    # not per-reading, so using it would wrongly boost minor kun readings
    # (e.g. 証/あかし inheriting 証's high しょう freq, displacing the
    # place name 明石). A flat low freq keeps every backfilled single
    # kanji reachable while always yielding #0 to corpus-attested
    # words/jukugo (明石 freq 35 > 10).
    lib_freq = 10
    kanas = []
    for rm in ch.findall("./reading_meaning/rmgroup/reading"):
        t = rm.get("r_type")
        if t == "ja_on":
            kanas.append(kata2hira(rm.text))
        elif t == "ja_kun":
            kanas.append(rm.text.lstrip("-").split(".")[0].rstrip("-"))
    added_any = False
    for k in kanas:
        if not k:
            continue
        code = kana_to_romaji(k)
        if code is None:
            skipped.append((lit, k)); continue
        key = (code, lit)
        if key in seen:
            continue
        seen.add(key)
        rows.append((code, lit, lib_freq)); codes.add(code); code_kana[code] = k
        added_any = True
    if added_any:
        n_chars += 1

rows.sort(key=lambda r: (r[1], r[0]))
AUDIT.mkdir(parents=True, exist_ok=True)
(AUDIT / "rows_added.tsv").write_text(
    "\n".join(f"{c}\t{w}\tkanji\t{f}\tpolish" for c, w, f in rows) + "\n", encoding="utf-8")
Path("/tmp/jk_roundtrip_in.txt").write_text("\n".join(sorted(codes)) + "\n", encoding="utf-8")
Path("/tmp/jk_roundtrip_pairs.tsv").write_text(
    "\n".join(f"{c}\t{code_kana[c]}" for c in sorted(codes)) + "\n", encoding="utf-8")

print(f"chars to add: {n_chars}   reading rows: {len(rows)}   unique codes: {len(codes)}")
print(f"unmappable kana skipped: {len(skipped)}  {skipped[:15]}")
import collections
fd = collections.Counter(f for _, _, f in rows)
print("freq distribution:", dict(sorted(fd.items(), reverse=True)))
print("samples:", [(c, w, f) for c, w, f in rows[:10]])

if APPLY:
    with NLIB.open("a", encoding="utf-8") as fh:
        for c, w, f in rows:
            fh.write(f"{c}\t{w}\tkanji\t{f}\tpolish\n")
    print(f"\n[applied] nihongo library += {len(rows)} rows")
else:
    print("\n[dry-run] re-run with --apply")
