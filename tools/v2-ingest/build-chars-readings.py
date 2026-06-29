#!/usr/bin/env python3
"""Phase 1 of pinyin v2 rewrite: build chars.tsv + readings.tsv from
authoritative open data sources.

Sources (in sources/):
- level-1.txt / level-2.txt / level-3.txt  通用规范汉字表 2013
  一/二/三级 (3500 / 3000 / 1605 = 8105 chars total). Defines char
  tier (一级 → t1, 二级 → t2, 三级 → t3).
- kMandarin_8105.txt   mozillazg/pinyin-data — 通用规范汉字表 char
  → 最常用读音 (primary canonical reading).
- kHanyuPinyin.txt     汉语大字典 char → all readings (polyphone full).
- kXHC1983.txt         新华字典 1983 char → readings (cross-check).

Outputs (to core/crates/inputx-pinyin-v2/data/):
- chars.tsv     <char>\\t<codepoint>\\t<tier>\\t<canonical_reading>
- readings.tsv  <char>\\t<reading>\\t<rank>\\t<tier_offset>\\t<source>
  rank: primary / secondary / archaic
  tier_offset: 0 / +1 / +3 (added to char tier when ranking words
  that use this reading)
"""

from __future__ import annotations
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
SRC = HERE / "sources"
OUT = HERE.parents[1] / "core/crates/inputx-pinyin-v2/data"


def read_level(path: Path) -> list[str]:
    return [ln.strip() for ln in path.read_text().splitlines() if ln.strip()]


def parse_unihan_line(ln: str) -> tuple[str, list[str]] | None:
    """Parse 'U+4E00: yī  # 一' → (char, ['yī'])."""
    if not ln.strip() or ln.startswith("#"):
        return None
    # split on first colon
    head, sep, rest = ln.partition(":")
    if not sep:
        return None
    head = head.strip()  # 'U+4E00'
    if not head.startswith("U+"):
        return None
    try:
        cp = int(head[2:], 16)
    except ValueError:
        return None
    ch = chr(cp)
    # strip trailing ' # 一' comment
    payload = rest.split("#", 1)[0].strip()
    if not payload:
        return None
    # readings are comma-separated; preserve order (file's "most common first")
    readings = [r.strip() for r in payload.split(",") if r.strip()]
    return (ch, readings)


def parse_unihan(path: Path) -> dict[str, list[str]]:
    out: dict[str, list[str]] = {}
    with path.open() as f:
        for ln in f:
            parsed = parse_unihan_line(ln)
            if parsed is None:
                continue
            ch, rs = parsed
            out[ch] = rs
    return out


def main() -> int:
    if not SRC.exists():
        print(f"[ingest] missing sources dir: {SRC}", file=sys.stderr)
        return 1

    # 1. char tier from 通用规范汉字表 一/二/三级.
    chars_t1 = set(read_level(SRC / "level-1.txt"))
    chars_t2 = set(read_level(SRC / "level-2.txt"))
    chars_t3 = set(read_level(SRC / "level-3.txt"))
    print(f"[ingest] tiers: t1={len(chars_t1)} t2={len(chars_t2)} t3={len(chars_t3)} total={len(chars_t1)+len(chars_t2)+len(chars_t3)}", file=sys.stderr)

    char_tier: dict[str, int] = {}
    for c in chars_t1: char_tier[c] = 1
    for c in chars_t2: char_tier[c] = 2
    for c in chars_t3: char_tier[c] = 3

    # 2. primary readings — kMandarin_8105 (one per char, official 通用规范).
    k_mandarin = parse_unihan(SRC / "kMandarin_8105.txt")
    print(f"[ingest] kMandarin entries: {len(k_mandarin)}", file=sys.stderr)

    # 3. full polyphone readings — kHanyuPinyin (汉语大字典).
    k_hanyu = parse_unihan(SRC / "kHanyuPinyin.txt")
    print(f"[ingest] kHanyuPinyin entries: {len(k_hanyu)}", file=sys.stderr)

    # 4. supplementary readings — kXHC1983 (新华字典 1983).
    k_xhc = parse_unihan(SRC / "kXHC1983.txt")
    print(f"[ingest] kXHC1983 entries: {len(k_xhc)}", file=sys.stderr)

    OUT.mkdir(parents=True, exist_ok=True)

    # === chars.tsv ===
    chars_path = OUT / "chars.tsv"
    n_chars = 0
    n_missing_reading = 0
    with chars_path.open("w") as f:
        f.write("# char\\tcodepoint\\ttier\\tcanonical_reading\n")
        f.write("# Source: 通用规范汉字表 2013 (3500/3000/1605) + mozillazg/pinyin-data kMandarin_8105\n")
        # Order: t1 (sorted by codepoint), t2, t3
        for tier_n, level_set in [(1, chars_t1), (2, chars_t2), (3, chars_t3)]:
            for ch in sorted(level_set, key=ord):
                cp = ord(ch)
                readings = k_mandarin.get(ch, [])
                if not readings:
                    n_missing_reading += 1
                    canonical = ""
                else:
                    canonical = readings[0]
                f.write(f"{ch}\t{cp:04X}\t{tier_n}\t{canonical}\n")
                n_chars += 1
    print(f"[ingest] wrote {chars_path} — {n_chars} chars ({n_missing_reading} without kMandarin reading)", file=sys.stderr)

    # === readings.tsv ===
    # For each char in chars 一/二/三级 set, emit each reading as a row.
    # Rank derivation:
    #   - if reading == kMandarin reading → primary
    #   - else if reading in kHanyuPinyin (汉语大字典) → secondary
    #   - else if only in kXHC1983 → supplementary
    readings_path = OUT / "readings.tsv"
    n_readings = 0
    n_chars_with_polyphone = 0
    with readings_path.open("w") as f:
        f.write("# char\\treading\\trank\\ttier_offset\\tsource\n")
        f.write("# rank: primary / secondary / supplementary\n")
        f.write("# tier_offset: 0 / +1 / +1 (added to char's tier when used in word ranking)\n")
        for tier_n, level_set in [(1, chars_t1), (2, chars_t2), (3, chars_t3)]:
            for ch in sorted(level_set, key=ord):
                # union all sources, preserving order
                seen: set[str] = set()
                rows: list[tuple[str, str, int, str]] = []  # (reading, rank, offset, src)
                primary = (k_mandarin.get(ch) or [""])[0]
                if primary:
                    rows.append((primary, "primary", 0, "kMandarin"))
                    seen.add(primary)
                for r in k_hanyu.get(ch, []):
                    if r in seen: continue
                    rows.append((r, "secondary", 1, "kHanyuPinyin"))
                    seen.add(r)
                for r in k_xhc.get(ch, []):
                    if r in seen: continue
                    rows.append((r, "supplementary", 1, "kXHC1983"))
                    seen.add(r)
                if len(rows) > 1:
                    n_chars_with_polyphone += 1
                for reading, rank, offset, src in rows:
                    f.write(f"{ch}\t{reading}\t{rank}\t{offset}\t{src}\n")
                    n_readings += 1
    print(f"[ingest] wrote {readings_path} — {n_readings} reading rows ({n_chars_with_polyphone} chars with polyphone)", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
