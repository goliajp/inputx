#!/usr/bin/env python3
"""Phase 2 of pinyin v2 rewrite: build words.tsv from CC-CEDICT +
HSK 2.0 tier overlay.

Pipeline:
1. Load chars.tsv (8105 标准字) + readings.tsv (12149 reading rows)
   built by build-chars-readings.py.
2. Load HSK 2.0 exclusive lists (level 1-6) → word → hsk_level.
3. Parse CC-CEDICT. For each entry:
   a. simplified word ≥ 2 chars
   b. all chars must be in chars.tsv (= 通用规范汉字表 8105)
   c. all chars must be all-lowercase Han (no proper-noun caps in pinyin)
   d. resolve reading_path: each char + its pinyin syllable matched
      against readings.tsv. Reject if any char's reading isn't in our
      readings table (= 字字直拼 noise / wrong reading / out-of-scope).
4. Assign tier from HSK level OR fallback by word length.
5. Output words.tsv.

Output schema:
    code\\tword\\treading_path\\ttier\\tsource
    nihao\\t你好\\t[你|nǐ][好|hǎo]\\t1\\tcedict+hsk1
"""

from __future__ import annotations
import json
import re
import sys
from collections import defaultdict
from pathlib import Path

HERE = Path(__file__).resolve().parent
SRC = HERE / "sources"
OUT = HERE.parents[1] / "core/crates/inputx-pinyin-v2/data"


# ── Pinyin tone3 → tone-mark + bare-code conversion ──────────────
# CC-CEDICT format: "ni3 hao3" / "nu:3" (where : marks ü).
TONE_MARKS = {
    'a': "āáǎà", 'e': "ēéěè", 'i': "īíǐì",
    'o': "ōóǒò", 'u': "ūúǔù", 'ü': "ǖǘǚǜ",
}


def syllable_tone3_to_mark(syl: str) -> str:
    """Convert 'huan2' → 'huán', 'nu:3' → 'nǚ'."""
    s = syl.lower().replace("u:", "ü")
    # extract tone digit
    tone = 0
    if s and s[-1] in "1234":
        tone = int(s[-1])
        s = s[:-1]
    elif s and s[-1] == "5":
        s = s[:-1]
    if tone == 0:
        return s
    # mark priority: a > o > e > (last of iu/ui) > i > u > ü
    chars = list(s)
    targets = []
    for i, c in enumerate(chars):
        if c in "aeiouü":
            targets.append((i, c))
    if not targets:
        return s
    if any(c == 'a' for _, c in targets):
        idx = next(i for i, c in targets if c == 'a')
    elif any(c == 'o' for _, c in targets):
        idx = next(i for i, c in targets if c == 'o')
    elif any(c == 'e' for _, c in targets):
        idx = next(i for i, c in targets if c == 'e')
    else:
        # for iu / ui, mark the LAST vowel
        idx = targets[-1][0]
    ch = chars[idx]
    chars[idx] = TONE_MARKS[ch][tone - 1]
    return "".join(chars)


def syllable_to_code_letter(syl: str) -> str:
    """Reduce 'huán' (tone-mark) to 'huan' (bare ASCII code letters).
    ü → v (IME convention)."""
    s = syl.replace("ü", "v")
    for vowel, marks in TONE_MARKS.items():
        for m in marks:
            s = s.replace(m, "v" if vowel == 'ü' else vowel)
    return s


# ── Source loaders ──────────────────────────────────────────────
def load_chars_set() -> set[str]:
    chars: set[str] = set()
    with (OUT / "chars.tsv").open() as f:
        for ln in f:
            if not ln.strip() or ln.startswith("#"):
                continue
            ch = ln.split("\t", 1)[0]
            if ch:
                chars.add(ch)
    return chars


def load_readings_index() -> dict[str, dict[str, str]]:
    """Return {char → {bare_letter_form: tone_mark_form}}.
    Bare form drops tone marks (jiě → jie) for matching CC-CEDICT
    neutral-tone syllables (jie5 / jie5 with no tone mark) against
    the tone-marked readings.tsv. Tone-mark form is preserved for
    output reading_path."""
    idx: dict[str, dict[str, str]] = defaultdict(dict)
    with (OUT / "readings.tsv").open() as f:
        for ln in f:
            if not ln.strip() or ln.startswith("#"):
                continue
            parts = ln.rstrip("\n").split("\t")
            if len(parts) < 2:
                continue
            ch, reading = parts[0], parts[1]
            bare = syllable_to_code_letter(reading).replace("v", "ü")
            idx[ch].setdefault(bare, reading)
    return idx


def load_hsk() -> dict[str, int]:
    """Return {simplified_word → hsk_level (1-6)}."""
    out: dict[str, int] = {}
    for level in range(1, 7):
        p = SRC / f"{level}.json"
        if not p.exists():
            continue
        data = json.loads(p.read_text())
        for entry in data:
            w = entry.get("simplified")
            if w and w not in out:
                out[w] = level
    return out


# ── CC-CEDICT parsing ────────────────────────────────────────────
CEDICT_LINE = re.compile(r"^(\S+)\s+(\S+)\s+\[([^\]]+)\]\s+/(.+)/\s*$")


def parse_cedict(path: Path):
    """Yield (simplified, pinyin_syllables, gloss) for each entry."""
    with path.open() as f:
        for ln in f:
            ln = ln.rstrip("\n")
            if not ln or ln.startswith("#"):
                continue
            m = CEDICT_LINE.match(ln)
            if not m:
                continue
            _trad, simp, pinyin, gloss = m.groups()
            syls = pinyin.split()
            yield simp, syls, gloss


# ── reading_path resolver ────────────────────────────────────────
def resolve_reading_path(word: str, syls: list[str],
                          chars_set: set[str],
                          readings_idx: dict[str, dict[str, str]]) -> list[tuple[str, str]] | None:
    """For each (char, pinyin_syllable), validate char ∈ chars_set AND
    pinyin matches one of char's readings (tone-mark or bare form for
    neutral-tone tolerance). Returns list of (char, tone-mark reading)
    or None on failure."""
    chars = list(word)
    if len(chars) != len(syls):
        return None
    out: list[tuple[str, str]] = []
    for ch, syl in zip(chars, syls):
        if ch not in chars_set:
            return None
        # Try tone-mark match first, then bare-letter fallback.
        marked = syllable_tone3_to_mark(syl)
        char_readings = readings_idx.get(ch, {})
        # bare-letter form of input syllable for lookup
        bare = marked
        for vowels, marks in TONE_MARKS.items():
            for m in marks:
                bare = bare.replace(m, vowels)
        bare = bare.replace("ü", "ü")  # keep ü as ü for index match
        if bare in char_readings:
            out.append((ch, char_readings[bare]))
            continue
        return None
    return out


def derive_code(reading_path: list[tuple[str, str]]) -> str:
    return "".join(syllable_to_code_letter(r) for _, r in reading_path)


def reading_path_str(reading_path: list[tuple[str, str]]) -> str:
    return "".join(f"[{ch}|{r}]" for ch, r in reading_path)


# ── Tier assignment ──────────────────────────────────────────────
def assign_tier(word: str, hsk_level: int | None) -> int:
    """HSK 1-2 → t1, HSK 3-4 → t2, HSK 5-6 → t3,
       non-HSK by length: 2c → t4, 3-4c → t5, 5+c → t6."""
    if hsk_level is not None:
        if hsk_level <= 2: return 1
        if hsk_level <= 4: return 2
        return 3  # hsk 5/6
    n = len(word)
    if n == 2: return 4
    if n <= 4: return 5
    return 6


# ── Reject filters (CC-CEDICT noise classes) ─────────────────────
def should_reject_pre_path(simp: str, syls: list[str], gloss: str) -> str | None:
    """Cheap rejects before the (expensive) reading_path resolver.
    Returns reject reason or None to keep."""
    if len(simp) < 2:
        return "single-char"
    # CC-CEDICT entries marked as variant — keep only the canonical
    if gloss.startswith("variant of "):
        return "variant"
    if gloss.startswith("old variant of "):
        return "old-variant"
    if gloss.startswith("see "):
        return "alias"
    # Proper-noun pinyin (capitalized) — sound names, place names
    if any(re.search(r"[A-Z]", s) for s in syls):
        return "proper-noun"
    # No Han chars (e.g., "3D打印")
    if not all('一' <= c <= '鿿' or '㐀' <= c <= '䶿' for c in simp):
        return "non-han"
    return None


# ── Main ─────────────────────────────────────────────────────────
def main() -> int:
    print(f"[ingest] loading chars + readings …", file=sys.stderr)
    chars_set = load_chars_set()
    readings_idx = load_readings_index()
    print(f"[ingest]   chars={len(chars_set)}, readings_idx={len(readings_idx)}", file=sys.stderr)

    print(f"[ingest] loading HSK 2.0 lists …", file=sys.stderr)
    hsk = load_hsk()
    print(f"[ingest]   HSK total={len(hsk)} words", file=sys.stderr)

    print(f"[ingest] parsing CC-CEDICT …", file=sys.stderr)
    accepted: list[tuple[str, str, str, int, str]] = []
    reject_reasons: dict[str, int] = defaultdict(int)
    total = 0
    for simp, syls, gloss in parse_cedict(SRC / "cc-cedict.txt"):
        total += 1
        reason = should_reject_pre_path(simp, syls, gloss)
        if reason:
            reject_reasons[reason] += 1
            continue
        rp = resolve_reading_path(simp, syls, chars_set, readings_idx)
        if rp is None:
            reject_reasons["reading_path_fail"] += 1
            continue
        code = derive_code(rp)
        tier = assign_tier(simp, hsk.get(simp))
        source = "cedict"
        if simp in hsk:
            source = f"cedict+hsk{hsk[simp]}"
        accepted.append((code, simp, reading_path_str(rp), tier, source))

    print(f"[ingest] CC-CEDICT total={total}", file=sys.stderr)
    print(f"[ingest]   accepted={len(accepted)}", file=sys.stderr)
    print(f"[ingest]   rejected:", file=sys.stderr)
    for r, n in sorted(reject_reasons.items(), key=lambda x: -x[1]):
        print(f"     {r:30s} {n:>7,d}", file=sys.stderr)

    # Sort: tier asc, then code asc, then word asc (deterministic)
    accepted.sort(key=lambda r: (r[3], r[0], r[1]))

    out_path = OUT / "words.tsv"
    with out_path.open("w") as f:
        f.write("# words.tsv — Phase 2 ingest from CC-CEDICT + HSK 2.0\n")
        f.write("# Schema: code\\tword\\treading_path\\ttier\\tsource\n")
        f.write("# reading_path: [char|reading][char|reading]... showing\n")
        f.write("#   exactly which reading of each char is used.\n")
        f.write("# tier: 1-3 HSK / 4-6 cedict by word length.\n")
        for row in accepted:
            f.write("\t".join(str(x) for x in row) + "\n")

    print(f"[ingest] wrote {out_path} — {len(accepted)} words", file=sys.stderr)

    # Tier distribution
    by_tier = defaultdict(int)
    for _, _, _, t, _ in accepted:
        by_tier[t] += 1
    print(f"[ingest] tier distribution:", file=sys.stderr)
    for t in sorted(by_tier):
        print(f"     tier {t}: {by_tier[t]:>7,d}", file=sys.stderr)

    # HSK coverage
    hsk_hit = sum(1 for _, w, _, _, _ in accepted if w in hsk)
    hsk_miss = len(hsk) - hsk_hit
    print(f"[ingest] HSK coverage: {hsk_hit}/{len(hsk)} matched, {hsk_miss} HSK words missed by CC-CEDICT path resolve", file=sys.stderr)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
