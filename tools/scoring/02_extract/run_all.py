#!/usr/bin/env python3
"""02_extract — per-source (word, count) tables via build_weights' Rust counter.

CP2 reuses build_weights' deterministic Aho-Corasick overlapping counter
(strategy B) so the Python pipeline never reproduces the counting + f64
accumulation itself — only the downstream arithmetic (03_normalize). This is
a thin wrapper that invokes:

    cargo run --features tools --release --bin pinyin-build-weights -- dump

which writes, WITHOUT touching the shipped weights.tsv:
  - tools/scoring/data/extracted/<src>/freq.tsv  (this step's output)
  - tools/scoring/data/baseline/weights-bare.tsv (the CP2 identity target)

See .claude/PLAN-dict-pipeline.md (CP2 hot plan) for the strategy rationale.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path


def repo_root() -> Path:
    out = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        capture_output=True,
        text=True,
        check=True,
    )
    return Path(out.stdout.strip())


def main() -> int:
    core = repo_root() / "core"
    cmd = [
        "cargo", "run", "--features", "tools", "--release",
        "--bin", "pinyin-build-weights", "--", "dump",
    ]
    print(f"[02] $ {' '.join(cmd)}  (cwd={core})")
    return subprocess.run(cmd, cwd=core).returncode


if __name__ == "__main__":
    sys.exit(main())
