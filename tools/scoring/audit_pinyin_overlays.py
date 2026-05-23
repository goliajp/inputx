#!/usr/bin/env python3
"""Audit + clean the pinyin overlay TSVs before they get baked into FST.

Targets:
  - tools/scoring/data/supplemental/pinyin_modern_v1.tsv (hand-curated)
  - tools/scoring/data/polish_reports/quickfix_boost.tsv (polish-log derived)

Each row is checked against a set of rules. Bad rows are dropped (and
logged); good rows are kept as-is. The script rewrites each file in
place with only the surviving rows. Run by `refresh.sh` between
generation and FST build.

Rules (composable, fail-on-any):
  R1 — `pinyin` must be non-empty, pure-lowercase-ASCII-alpha
  R2 — `word` must contain at least one CJK character (pure-English
       like HIIT/yyds/offer rejected — they shouldn't be in a pinyin
       dict; user typing pinyin doesn't want English commits)
  R3 — `word` must not contain Chinese / Japanese / fullwidth
       punctuation marks (？！，。、…〜～♪♥★☆「」『』 etc.) — these
       are author intent, not dict entries
  R4 — `freq` must be a positive integer (no negative, no malformed)
  R5 — dedup: keep highest-freq row when (pinyin, word) repeats

Output: rewritten TSV + summary printed to stderr.

Run:  python3 tools/scoring/audit_pinyin_overlays.py
      (idempotent — re-running on already-clean files is a no-op)
"""

from __future__ import annotations
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
TARGETS = [
    ROOT / "tools/scoring/data/supplemental/pinyin_modern_v1.tsv",
    ROOT / "tools/scoring/data/polish_reports/quickfix_boost.tsv",
]

PUNCT_RE = re.compile(r"[？！?!，。、…〜～♪♥★☆「」『』·•‧]")


def has_cjk(word: str) -> bool:
    return any("一" <= c <= "鿿" for c in word)


def audit_row(pinyin: str, word: str, freq_s: str) -> tuple[bool, str]:
    """Return (ok, reason). reason is "" on ok, error message on failure."""
    if not pinyin:
        return False, "R1: empty pinyin"
    if not pinyin.isascii() or not pinyin.isalpha() or pinyin != pinyin.lower():
        return False, f"R1: bad pinyin {pinyin!r} (must be lowercase ASCII alpha)"
    if not word:
        return False, "R2: empty word"
    if not has_cjk(word):
        return False, f"R2: no CJK in {word!r} (pure non-CJK doesn't belong in pinyin dict)"
    if PUNCT_RE.search(word):
        return False, f"R3: punctuation in {word!r}"
    # freq parsing — allow trailing "# n=N" comment in same field
    freq_clean = freq_s.split("#")[0].strip()
    try:
        freq = int(freq_clean)
    except ValueError:
        return False, f"R4: bad freq {freq_s!r}"
    if freq <= 0:
        return False, f"R4: non-positive freq {freq}"
    return True, ""


def audit_file(path: Path) -> dict:
    """Audit one TSV file in-place. Returns summary dict."""
    if not path.exists():
        return {"path": str(path), "exists": False}

    header_lines: list[str] = []
    rows: list[tuple[str, str, int]] = []  # (pinyin, word, freq_int)
    dropped: list[tuple[int, str, str]] = []  # (line_no, raw, reason)

    with path.open() as f:
        for line_no, raw in enumerate(f, 1):
            line = raw.rstrip("\n").rstrip("\r")
            if line.startswith("#") or not line.strip():
                header_lines.append(raw.rstrip("\n"))
                continue
            parts = line.split("\t")
            if len(parts) < 3:
                dropped.append((line_no, line, "malformed: fewer than 3 tab fields"))
                continue
            pinyin, word, freq_s = parts[0].strip(), parts[1].strip(), parts[2].strip()
            ok, reason = audit_row(pinyin, word, freq_s)
            if not ok:
                dropped.append((line_no, line, reason))
                continue
            freq = int(freq_s.split("#")[0].strip())
            rows.append((pinyin, word, freq))

    # R5: dedup — keep highest freq for each (pinyin, word).
    best: dict[tuple[str, str], int] = {}
    for py, w, freq in rows:
        cur = best.get((py, w))
        if cur is None or freq > cur:
            best[(py, w)] = freq
    deduped_count = len(rows) - len(best)
    rows = [(py, w, freq) for (py, w), freq in best.items()]

    # Sort: freq desc, pinyin, word for deterministic output.
    rows.sort(key=lambda r: (-r[2], r[0], r[1]))

    # Rewrite file (keep original header comments).
    with path.open("w") as f:
        for h in header_lines:
            f.write(h + "\n")
        for py, w, freq in rows:
            f.write(f"{py}\t{w}\t{freq}\n")

    return {
        "path": str(path),
        "exists": True,
        "kept": len(rows),
        "dropped": len(dropped),
        "deduped": deduped_count,
        "drop_samples": dropped[:10],
    }


def main() -> int:
    total_kept = 0
    total_dropped = 0
    total_deduped = 0
    for path in TARGETS:
        summary = audit_file(path)
        if not summary["exists"]:
            print(f"[audit] SKIP {path} (not found)", file=sys.stderr)
            continue
        print(
            f"[audit] {path.name}: kept={summary['kept']} dropped={summary['dropped']} deduped={summary['deduped']}",
            file=sys.stderr,
        )
        total_kept += summary["kept"]
        total_dropped += summary["dropped"]
        total_deduped += summary["deduped"]
        for ln, raw, reason in summary["drop_samples"]:
            print(f"    L{ln}: {reason} — {raw!r}", file=sys.stderr)
        if summary["dropped"] > 10:
            print(f"    … +{summary['dropped']-10} more dropped", file=sys.stderr)
    print(
        f"[audit] TOTAL kept={total_kept} dropped={total_dropped} deduped={total_deduped}",
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
