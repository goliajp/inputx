#!/usr/bin/env python3
"""Generate the audit master HTML with all phases baked in + per-row
suggestions + chunking ≤1000 rows per sub-phase.

Output: tools/audit-ui/audit.html (self-contained, no file picker).

Each phase from PLAN.md is split into chunks of CHUNK_SIZE rows, each
becoming a separate sub-phase in the sidebar (e.g. B3-001, B3-002, …).
Empty phases are skipped entirely. Each row carries a `sug` (suggested
decision: d/s/t/f) and `signals` (list of tags driving the suggestion)
so the UI can offer one-key "accept suggestion".
"""

from __future__ import annotations
import argparse
import json
import sys
from collections import defaultdict
from pathlib import Path

HERE = Path(__file__).resolve().parent
LIBRARY = HERE.parents[1] / "core/crates/inputx-pinyin/data/library.tsv"

CHUNK_SIZE = 1000


def char_class(word: str) -> str:
    n = len(word)
    if n == 1: return "1c"
    if n == 2: return "2c"
    if n == 3: return "3c"
    if n == 4: return "4c"
    if n <= 6: return "56"
    return "7plus"


def freq_class(f: int) -> str:
    if f == 0: return "f0"
    if f < 10000: return "f1k-10k"
    if f < 50000: return "f10k-50k"
    return "f50k+"


PHASES: list[tuple[str, str, callable]] = [
    ("D",     "D · super-high (f≥50k)",       lambda lc, fc: fc == "f50k+"),
    ("7plus", "7plus · long phrases (7+c)",   lambda lc, fc: lc == "7plus"),
    ("56",    "56 · 5-6c phrases",             lambda lc, fc: lc == "56"),
    ("A1",    "A1 · 1c rare (f=0)",            lambda lc, fc: lc == "1c" and fc == "f0"),
    ("A2",    "A2 · 2c rare (f=0)",            lambda lc, fc: lc == "2c" and fc == "f0"),
    ("A4",    "A4 · 4c rare (f=0)",            lambda lc, fc: lc == "4c" and fc == "f0"),
    ("A3",    "A3 · 3c rare (f=0)",            lambda lc, fc: lc == "3c" and fc == "f0"),
    ("B4",    "B4 · 4c mid (f1k-10k)",         lambda lc, fc: lc == "4c" and fc == "f1k-10k"),
    ("B2",    "B2 · 2c mid (f1k-10k)",         lambda lc, fc: lc == "2c" and fc == "f1k-10k"),
    ("B3",    "B3 · 3c mid (f1k-10k)",         lambda lc, fc: lc == "3c" and fc == "f1k-10k"),
    ("C2",    "C2 · 2c high (f10k-50k)",       lambda lc, fc: lc == "2c" and fc == "f10k-50k"),
    ("C1",    "C1 · 1c high (f10k-50k)",       lambda lc, fc: lc == "1c" and fc == "f10k-50k"),
    ("C3",    "C3 · 3c high (f10k-50k)",       lambda lc, fc: lc == "3c" and fc == "f10k-50k"),
    ("C4",    "C4 · 4c high (f10k-50k)",       lambda lc, fc: lc == "4c" and fc == "f10k-50k"),
    ("B1",    "B1 · 1c mid (f1k-10k)",         lambda lc, fc: lc == "1c" and fc == "f1k-10k"),
]


# ── Char classification ─────────────────────────────────────────────
def is_cjk_unified(cp: int) -> bool:
    return (0x4E00 <= cp <= 0x9FFF) or (0x3400 <= cp <= 0x4DBF) \
        or (0x20000 <= cp <= 0x2FFFF) or (0x30000 <= cp <= 0x3134F)

def is_cjk_basic(cp: int) -> bool:
    """Common CJK Unified Ideographs (BMP main block) — high-confidence-real."""
    return 0x4E00 <= cp <= 0x9FFF

def is_cjk_extA(cp: int) -> bool:
    """CJK Ext-A (BMP, U+3400-4DBF) — rare but real."""
    return 0x3400 <= cp <= 0x4DBF


# ── Suggestion engine ───────────────────────────────────────────────
def compute_suggestion(code: str, word: str, freq: int,
                       byword: dict[str, list]) -> tuple[str, list[str]]:
    """Return (suggested_decision, signals[]).

    decision letter: d=delete, s=keep, t=tier-demote, f=flag-uncertain.
    signals: list of short tags shown in UI; user can override per-row.
    """
    signals: list[str] = []
    chars = list(word)
    n = len(chars)

    # signal: contains non-CJK char (mojibake / latin / digit / punct)
    if any(ord(c) >= 0x80 and not is_cjk_unified(ord(c)) for c in chars):
        signals.append("nonCJK")
    # signal: contains CJK Ext-A rare char
    if any(is_cjk_extA(ord(c)) for c in chars):
        signals.append("ExtA")
    # signal: polyphone-dup risk — same word at ≥2 codes with same freq
    peers = byword.get(word, [])
    if len(peers) >= 2:
        same_freq = [p for p in peers if p[2] == freq and (p[0], p[1]) != (code, word)]
        if same_freq:
            signals.append(f"dup×{len(same_freq)+1}")
    # signal: 1-char word (single CJK) — keep-bias unless rare
    if n == 1:
        signals.append("1c")

    # Decision tree:
    # Highest-confidence delete:
    if "nonCJK" in signals:
        return "d", signals
    # Very low freq multi-char = likely noise (post-freq0-sweep, this is
    # the 1k-3k band which has high noise density)
    if n >= 2 and freq < 3000:
        return "d", signals
    # Ext-A rare chars in multi-char compound = likely noise
    if "ExtA" in signals and n >= 2:
        return "d", signals
    # Polyphone-dup at identical freq = mechanical copy noise
    if any(s.startswith("dup×") for s in signals) and freq < 30000:
        return "d", signals
    # Single-char Ext-A = rare but real; flag for human eyeball
    if "ExtA" in signals and n == 1:
        return "f", signals
    # Mid-freq multi-char without bad signals = likely real
    if freq >= 10000:
        return "s", signals
    # Borderline (3k-10k) multi-char without bad signals = uncertain
    if n >= 2:
        return "f", signals
    # Default: keep
    return "s", signals


# ── Pipeline ────────────────────────────────────────────────────────
def load_library(path: Path) -> list[tuple[str, str, int]]:
    out: list[tuple[str, str, int]] = []
    with path.open() as f:
        for ln in f:
            if not ln.strip() or ln.startswith("#"):
                continue
            parts = ln.rstrip("\n").split("\t")
            if len(parts) != 4:
                continue
            code, word, freq_s, _src = parts
            try:
                freq = int(freq_s)
            except ValueError:
                continue
            out.append((code, word, freq))
    return out


def chunk_phase(rows: list, chunk_size: int) -> list[list]:
    """Split sorted rows into chunks of ≤chunk_size each."""
    return [rows[i:i + chunk_size] for i in range(0, len(rows), chunk_size)]


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--library", type=Path, default=LIBRARY)
    ap.add_argument("--out", type=Path, default=HERE / "audit.html")
    ap.add_argument("--chunk", type=int, default=CHUNK_SIZE)
    args = ap.parse_args()

    print(f"[gen] reading {args.library} …", file=sys.stderr)
    lib = load_library(args.library)
    print(f"[gen] library: {len(lib):,} rows", file=sys.stderr)

    # Build global cross-code index by word for polyphone-dup signal.
    byword: dict[str, list] = defaultdict(list)
    for r in lib:
        byword[r[1]].append(r)

    phases_payload = []
    for pid, label, rule in PHASES:
        rows = [(c, w, f) for c, w, f in lib if rule(char_class(w), freq_class(f))]
        if not rows:
            print(f"[gen]   {pid:6s} skipped (empty)", file=sys.stderr)
            continue
        rows.sort(key=lambda r: (-r[2], r[0]))
        chunks = chunk_phase(rows, args.chunk)
        for i, chunk in enumerate(chunks):
            sub_id = f"{pid}-{i+1:03d}" if len(chunks) > 1 else pid
            sub_label = f"{label} · part {i+1}/{len(chunks)}" if len(chunks) > 1 else label
            enriched_rows = []
            for c, w, fr in chunk:
                sug, sigs = compute_suggestion(c, w, fr, byword)
                enriched_rows.append([c, w, fr, sug, sigs])
            phases_payload.append({"id": sub_id, "label": sub_label, "rows": enriched_rows})
        if len(chunks) > 1:
            print(f"[gen]   {pid:6s} {len(rows):>7,d}  → {len(chunks)} chunks of ≤{args.chunk}", file=sys.stderr)
        else:
            print(f"[gen]   {pid:6s} {len(rows):>7,d}  (single chunk)", file=sys.stderr)

    template = (HERE / "index.html").read_text()
    if "// __INJECTED_DATA__" not in template:
        print("[gen] ERROR: index.html missing `// __INJECTED_DATA__` placeholder", file=sys.stderr)
        return 1

    payload = {"phases": phases_payload, "lib_total": len(lib), "chunk_size": args.chunk}
    inject = "window.AUDIT_DATA = " + json.dumps(payload, ensure_ascii=False, separators=(",", ":")) + ";"
    html = template.replace("// __INJECTED_DATA__", inject)
    args.out.write_text(html)

    total_candidates = sum(len(p["rows"]) for p in phases_payload)
    size_mb = args.out.stat().st_size / 1024 / 1024
    print(f"[gen] {total_candidates:,d} candidates · {len(phases_payload)} sub-phases → {args.out} ({size_mb:.1f} MB)", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
