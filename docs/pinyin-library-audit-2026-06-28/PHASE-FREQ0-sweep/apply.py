#!/usr/bin/env python3
"""PHASE-FREQ0-sweep apply — D1 delete every library.tsv row with freq=0.

Rationale (2026-06-28 user "freq 0 全删"):
- freq=0 = corpus 零观测;宁缺毋滥 standing 默认拒收
- 142,566 行 (35.9% of post-SIP library) 一次清理
- 多字 (2c+) f=0 行几乎全是 字字直拼 / 人名 / 半成品组合 — 纯噪音
- 单字 (1c) f=0 = BMP CJK Ext-A 罕字 / 异体字;真但极罕用,可通过
  本目录 deleted-rows.tsv 选择性恢复 (若日后某个具体字 polish 要)

Outputs:
  deleted-rows.tsv        — 全 142k 行原数据 (audit trail, in-repo)
  library.tsv (modified)
  corpus_garbage_filter_v1.tsv (appended)
"""

from __future__ import annotations
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
LIBRARY = REPO / "core/crates/inputx-pinyin/data/library.tsv"
GARBAGE = REPO / "tools/scoring/data/polish/corpus_garbage_filter_v1.tsv"
TODAY = "2026-06-28"


def main() -> int:
    lines = LIBRARY.read_text().splitlines(keepends=True)
    keep: list[str] = []
    removed: list[tuple[str, str]] = []
    for ln in lines:
        if not ln.strip() or ln.startswith("#"):
            keep.append(ln); continue
        parts = ln.rstrip("\n").split("\t")
        if len(parts) != 4:
            keep.append(ln); continue
        code, word, freq, _src = parts
        try:
            f = int(freq)
        except ValueError:
            keep.append(ln); continue
        if f == 0:
            removed.append((code, word))
        else:
            keep.append(ln)

    if not removed:
        print("[apply] no freq=0 rows — nothing to do", file=sys.stderr)
        return 0

    # 1. audit trail
    trail = HERE / "deleted-rows.tsv"
    with trail.open("w") as f:
        f.write(f"# PHASE-FREQ0-sweep deleted rows ({TODAY})\n")
        f.write("# criterion: freq == 0 (zero corpus observation)\n")
        f.write("# format: code\\tword\\tcharlen\n")
        for code, word in removed:
            f.write(f"{code}\t{word}\t{len(word)}\n")

    # 2. rewrite library
    LIBRARY.write_text("".join(keep))

    # 3. append garbage filter (single header + bulk rows)
    with GARBAGE.open("a") as f:
        f.write(f"# {TODAY} PHASE-FREQ0-sweep — bulk D1 of {len(removed)} freq=0 rows (zero corpus observation)\n")
        for code, word in removed:
            f.write(f"{code}\t{word}\t{TODAY}\t# freq=0\n")

    print(f"[apply] removed {len(removed):,} rows from library.tsv", file=sys.stderr)
    print(f"[apply] audit trail → {trail}", file=sys.stderr)
    print(f"[apply] garbage filter appended → {GARBAGE}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
