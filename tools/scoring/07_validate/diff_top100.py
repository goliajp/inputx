#!/usr/bin/env python3
"""Diff the per-code candidate lists between two weights.tsv snapshots.

This is the `make diff-vs-shipped` engine (CP1 step3 of the dict-pipeline
rebuild). Given a freshly-built `weights.tsv` and the currently-shipped
one, it reports the input codes whose candidate ordering changed — the
human-reviewable signal gating any cutover (SCORING.md Phase 2:
"re-derive existing weights, validate identity").

"Candidate list" here is the **weights-level** view: for each pinyin code,
the words sorted by freq_score descending. This is NOT the full runtime
candidate list — the live IME layers wubi simple-code floors, cross-engine
JP/wubi candidates, composed fallbacks, and n-gram reranking on top (see
composite/scoring.rs). A weights-level diff catches every change that
*originates in the pinyin dictionary*, which is what this pipeline owns;
an engine-level diff needs `inputx-probe` and is a separate gate (TODO,
wire in at CP4 alongside the 4 QA gates).

Baseline defaults to the git-committed shipped weights (`git:HEAD`) so
"what's shipped" is unambiguous and reproducible. New defaults to the
pipeline output `data/merged/weights.tsv` if present, else the working-tree
shipped file — so on a clean tree with no pipeline output yet, the tool
self-compares and reports zero changes (the CP1 step3 verification).

Usage:
    python3 diff_top100.py                       # HEAD vs working tree
    python3 diff_top100.py --new data/merged/weights.tsv
    python3 diff_top100.py --baseline v1.2.0     # vs a tagged release
    python3 diff_top100.py --top 100 --depth 5

Exit code is always 0 on success (this is a report, not a gate); non-zero
only on I/O / git errors.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

# weights.tsv location relative to repo root.
SHIPPED_REL = "core/crates/inputx-pinyin/data/weights/weights.tsv"


def repo_root() -> Path:
    out = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        capture_output=True,
        text=True,
        check=True,
    )
    return Path(out.stdout.strip())


def load_from_text(text: str) -> dict[str, list[tuple[str, int]]]:
    """Parse weights.tsv text → {code: [(word, freq), ...] sorted desc}."""
    by_code: dict[str, list[tuple[str, int]]] = defaultdict(list)
    for raw in text.splitlines():
        line = raw.rstrip("\r\n")
        if not line or line.startswith("#"):
            continue
        parts = line.split("\t")
        if len(parts) < 3:
            continue
        code, word, freq_s = parts[0], parts[1], parts[2]
        try:
            freq = int(freq_s)
        except ValueError:
            continue
        by_code[code].append((word, freq))
    # Deterministic order: freq desc, then word asc as tiebreak — mirrors
    # weights.tsv's own secondary sort so equal-freq ties are stable and a
    # clean self-diff is genuinely empty (not order-noise).
    for code in by_code:
        by_code[code].sort(key=lambda wf: (-wf[1], wf[0]))
    return by_code


def load_source(spec: str, root: Path) -> dict[str, list[tuple[str, int]]]:
    """Load weights from a file path or a `git:<ref>` / bare git-ref spec."""
    ref: str | None = None
    if spec.startswith("git:"):
        ref = spec[len("git:") :]
    elif "/" not in spec and not Path(spec).exists():
        # bare token that isn't an existing path → treat as a git ref.
        ref = spec
    if ref is not None:
        out = subprocess.run(
            ["git", "show", f"{ref}:{SHIPPED_REL}"],
            capture_output=True,
            text=True,
            cwd=root,
        )
        if out.returncode != 0:
            sys.exit(
                f"error: git show {ref}:{SHIPPED_REL} failed: {out.stderr.strip()}"
            )
        return load_from_text(out.stdout)
    path = Path(spec)
    if not path.exists():
        sys.exit(f"error: weights file not found: {path}")
    return load_from_text(path.read_text())


def classify(base: list[str], new: list[str]) -> str | None:
    """Change kind for two top-depth word lists, or None if identical."""
    if base == new:
        return None
    if not base:
        return "new-code"
    if not new:
        return "dropped-code"
    if base[0] != new[0]:
        return "top1-flip"
    if set(base) != set(new):
        return "membership"
    return "reorder"


# Most-disruptive kinds first; impact (freq mass) breaks ties within a kind.
KIND_RANK = {
    "top1-flip": 0,
    "membership": 1,
    "new-code": 2,
    "dropped-code": 2,
    "reorder": 3,
}


def main() -> int:
    ap = argparse.ArgumentParser(
        description="Per-code candidate-list diff between two weights.tsv snapshots."
    )
    ap.add_argument(
        "--baseline",
        default="git:HEAD",
        help="baseline weights: file path, `git:<ref>`, or bare git ref "
        "(default: git:HEAD = currently shipped)",
    )
    ap.add_argument(
        "--new",
        default=None,
        help="new weights file (default: data/merged/weights.tsv if present, "
        "else working-tree shipped → self-diff)",
    )
    ap.add_argument(
        "--top", type=int, default=100, help="max changed codes to report (default 100)"
    )
    ap.add_argument(
        "--depth",
        type=int,
        default=5,
        help="candidate-list depth to compare per code (default 5)",
    )
    args = ap.parse_args()

    root = repo_root()

    if args.new is None:
        merged = root / "tools/scoring/data/merged/weights.tsv"
        new_spec = str(merged) if merged.exists() else str(root / SHIPPED_REL)
    else:
        new_spec = args.new

    base = load_source(args.baseline, root)
    new = load_source(new_spec, root)

    depth = args.depth
    changes: list[tuple[str, str, int, list[str], list[str]]] = []
    for code in sorted(set(base) | set(new)):
        b_words = [w for w, _ in base.get(code, [])[:depth]]
        n_words = [w for w, _ in new.get(code, [])[:depth]]
        kind = classify(b_words, n_words)
        if kind is None:
            continue
        # Impact = total freq mass on this code = how visible the change is.
        impact = sum(f for _, f in base.get(code, [])) + sum(
            f for _, f in new.get(code, [])
        )
        changes.append((code, kind, impact, b_words, n_words))

    changes.sort(key=lambda c: (KIND_RANK.get(c[1], 9), -c[2]))

    total_codes = len(set(base) | set(new))
    print(f"# diff-vs-shipped — candidate-list changes (depth {depth})")
    print(f"# baseline: {args.baseline}   new: {new_spec}")
    print(f"# codes compared: {total_codes}   changed: {len(changes)}")
    if not changes:
        print("# ✓ no candidate-list changes (identity)")
        return 0
    print(f"# showing top {min(args.top, len(changes))} by (kind, impact)")
    print("#")
    print("code\tkind\timpact\tbaseline_top\tnew_top")
    for code, kind, impact, b_words, n_words in changes[: args.top]:
        print(f"{code}\t{kind}\t{impact}\t{' '.join(b_words)}\t{' '.join(n_words)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
