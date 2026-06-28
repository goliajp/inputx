#!/usr/bin/env python3
"""Generate the audit master HTML with all phases baked in.

Reads `core/crates/inputx-pinyin/data/library.tsv`, slices it into the
14 phases defined in `docs/pinyin-library-audit-2026-06-28/PLAN.md`, and
injects ALL phases inline into `tools/audit-ui/index.html` → writes
`tools/audit-ui/audit.html`. Open that file in a browser — phase menu
is on the left, no file picker, no per-phase HTML files.

Usage:
    python3 tools/audit-ui/gen-candidates.py          # build audit.html
    python3 tools/audit-ui/gen-candidates.py --phase D   # one phase only
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
LIBRARY = HERE.parents[1] / "core/crates/inputx-pinyin/data/library.tsv"


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


# Phase id → (label, rule(lc, fc))
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


def slice_phase(lib: list[tuple[str, str, int]], rule) -> list[tuple[str, str, int]]:
    rows = [(c, w, f) for c, w, f in lib if rule(char_class(w), freq_class(f))]
    rows.sort(key=lambda r: (-r[2], r[0]))
    return rows


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--phase", help="only one phase (still embedded in audit.html, but no others)")
    ap.add_argument("--library", type=Path, default=LIBRARY)
    ap.add_argument("--out", type=Path, default=HERE / "audit.html")
    args = ap.parse_args()

    print(f"[gen] reading {args.library} …", file=sys.stderr)
    lib = load_library(args.library)
    print(f"[gen] library: {len(lib)} rows", file=sys.stderr)

    phases_filtered = [p for p in PHASES if (args.phase is None or p[0] == args.phase)]

    phases_payload = []
    for pid, label, rule in phases_filtered:
        rows = slice_phase(lib, rule)
        phases_payload.append({"id": pid, "label": label, "rows": [[c, w, f] for c, w, f in rows]})
        print(f"[gen]   {pid:6s} {len(rows):>7,d}  {label}", file=sys.stderr)

    template = (HERE / "index.html").read_text()
    if "// __INJECTED_DATA__" not in template:
        print("[gen] ERROR: index.html missing `// __INJECTED_DATA__` placeholder", file=sys.stderr)
        return 1

    payload = {"phases": phases_payload, "lib_total": len(lib)}
    inject = "window.AUDIT_DATA = " + json.dumps(payload, ensure_ascii=False, separators=(",", ":")) + ";"
    html = template.replace("// __INJECTED_DATA__", inject)
    args.out.write_text(html)

    total_candidates = sum(len(p["rows"]) for p in phases_payload)
    size_mb = args.out.stat().st_size / 1024 / 1024
    print(f"[gen] {total_candidates:,d} candidates across {len(phases_payload)} phases → {args.out} ({size_mb:.1f} MB)", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
