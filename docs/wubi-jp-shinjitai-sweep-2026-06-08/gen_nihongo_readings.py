"""
Generate nihongo single-kanji rows for the 153 Shinjitai chars that the
2026-06-08 wubi sweep removed from wubi/pinyin but which must stay
type-able in Japanese.

Reading source: KANJIDIC2 (EDRDG, /tmp/kanjidic2.xml.gz) — the
authoritative kanji on/kun reading database. Chosen over the mozc cache
because mozc single-char entries carry name/place noise (e.g. みにく→亜,
よつのや→乗) that would pollute those romaji inputs; KANJIDIC2 separates
on'yomi / kun'yomi / nanori cleanly and we drop nanori.

Per char:
  - ja_on  (katakana) → hiragana → romaji
  - ja_kun (hiragana, may carry okurigana ".える" or prefix "-") → take
    the stem (before "."), strip "-" markers → romaji
  - nanori (name readings) skipped

kana→romaji is Hepburn; the hard requirement is round-trip:
to_hiragana(code) == kana via the engine's own table (verified by
examples/kana_roundtrip.rs). Ambiguous kana use the round-tripping form
(ぢ→di, づ→du).

Output (idempotent, /tmp + audit only):
  - /tmp/jp_new_rows.tsv      code\tword\tkanji\tfreq\tpolish
  - /tmp/jp_roundtrip_in.txt  unique codes for the rust verifier
"""
import gzip
import xml.etree.ElementTree as ET
from pathlib import Path

DELS = {l.split("\t")[0] for l in
        Path("docs/wubi-jp-shinjitai-sweep-2026-06-08/deleted.tsv").read_text().splitlines()
        if l and not l.startswith("#")}
NLIB = Path("core/crates/inputx-nihongo/data/library.tsv")
KD = "/tmp/kanjidic2.xml.gz"
FREQ = 10  # reachable, won't crowd established single-kanji (freq 70-85)

have = set()
for l in NLIB.read_text(encoding="utf-8").splitlines():
    if l and not l.startswith("#"):
        p = l.split("\t")
        if len(p) >= 2 and p[1] in DELS and len(p[1]) == 1:
            have.add(p[1])
need = DELS - have


def kata2hira(s):
    return "".join(chr(ord(c) - 0x60) if 0x30A1 <= ord(c) <= 0x30F6 else c for c in s)


# ---- Hepburn kana→romaji (round-trip-correct) ----
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


root = ET.fromstring(gzip.open(KD).read())
rows = []          # (code, word)
seen = set()       # (code, word) dedup
codes = set()
code_kana = {}     # code -> source kana (for round-trip verification)
skipped = []       # (char, kana) unmappable
covered_chars = set()
for ch in root.findall("character"):
    lit = ch.findtext("literal")
    if lit not in need:
        continue
    covered_chars.add(lit)
    kanas = []
    for rm in ch.findall("./reading_meaning/rmgroup/reading"):
        t = rm.get("r_type")
        if t == "ja_on":
            kanas.append(kata2hira(rm.text))
        elif t == "ja_kun":
            kanas.append(rm.text.lstrip("-").split(".")[0].rstrip("-"))
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
        rows.append((code, lit)); codes.add(code); code_kana[code] = k

rows.sort(key=lambda r: (r[1], r[0]))
Path("/tmp/jp_new_rows.tsv").write_text(
    "\n".join(f"{c}\t{w}\tkanji\t{FREQ}\tpolish" for c, w in rows) + "\n", encoding="utf-8")
Path("/tmp/jp_roundtrip_in.txt").write_text("\n".join(sorted(codes)) + "\n", encoding="utf-8")
Path("/tmp/jp_roundtrip_pairs.tsv").write_text(
    "\n".join(f"{c}\t{code_kana[c]}" for c in sorted(codes)) + "\n", encoding="utf-8")

print(f"need={len(need)}  KANJIDIC2 covered={len(covered_chars)}  missing={len(need - covered_chars)}")
print(f"reading rows generated: {len(rows)}  unique codes: {len(codes)}")
print(f"unmappable kana skipped: {len(skipped)}  {skipped[:20]}")
print("samples:")
for c, w in rows[:14]:
    print(f"   {c}\t{w}")
