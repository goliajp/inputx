#!/usr/bin/env python3
"""Gate 1 — regression corpus: run tests/input_corpus.tsv against the engine.

CP3a of the dict-pipeline rebuild. Since CP3b's per-source log-rank leaves the
byte-identical regime, this is the engine-level quality gate that replaces the
`cmp` identity check: for each (code, expected_top) row, drive inputx-probe and
assert expected_top lands at candidate position 0.

The probe loads its pinyin dict via the INPUTX_PINYIN_DICT env var (dev/test
escape hatch added in dict.rs), so we can validate a pipeline-built dict
WITHOUT rebuilding the binary or touching the shipped data/pinyin.dict:

    pinyin-build-dict --weights tools/scoring/data/merged/weights.tsv \\
                      --out /tmp/cp.dict --no-overlay
    python3 07_validate/gate1_regression_corpus.py --dict /tmp/cp.dict

engine_hint → probe --mode:
    wubi → wubi   pinyin → pinyin   japanese-only → japanese   mixed → mixed

Exit: 0 if all rows pass (gate semantics). --baseline turns failures into a
report-only run (exit 0 regardless) for recording the pre-change pass rate.

The `maodund` pollution NEGATIVE case in input_corpus.tsv is a comment (no
expected_top), so it is skipped here; CP3d adds an explicit negative check.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

HINT_TO_MODE = {
    "wubi": "wubi",
    "pinyin": "pinyin",
    "japanese-only": "japanese",
    "mixed": "mixed",
    "llm": "pinyin",
}


def repo_root() -> Path:
    out = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        capture_output=True,
        text=True,
        check=True,
    )
    return Path(out.stdout.strip())


def find_probe(root: Path, explicit: str | None) -> Path:
    if explicit:
        p = Path(explicit)
        if not p.exists():
            sys.exit(f"error: --probe {p} not found")
        return p
    candidates = []
    if tgt := os.environ.get("CARGO_TARGET_DIR"):
        candidates.append(Path(tgt) / "release" / "inputx-probe")
    # cargo metadata resolves ~/.cargo/config.toml target-dir — this repo points
    # it at an external SSD, so core/target may not exist locally.
    try:
        meta = subprocess.run(
            ["cargo", "metadata", "--no-deps", "--format-version", "1"],
            cwd=root / "core", capture_output=True, text=True, check=True,
        )
        candidates.append(Path(json.loads(meta.stdout)["target_directory"]) / "release" / "inputx-probe")
    except Exception:
        pass
    candidates.append(root / "core" / "target" / "release" / "inputx-probe")
    for c in candidates:
        if c.exists():
            return c
    sys.exit(
        "error: inputx-probe not found — build it first:\n"
        "  (cd core && cargo build --release --bin inputx-probe)\n"
        "or pass --probe <path>. Tried: " + ", ".join(str(c) for c in candidates)
    )


def run_probe(probe: Path, dict_path: str, code: str, mode: str) -> list[str]:
    env = dict(os.environ)
    env["INPUTX_PINYIN_DICT"] = dict_path
    out = subprocess.run(
        [str(probe), code, "--mode", mode],
        capture_output=True,
        text=True,
        env=env,
    )
    if out.returncode != 0:
        raise RuntimeError(f"probe {code} --mode {mode} failed: {out.stderr.strip()}")
    doc = json.loads(out.stdout)
    return [c["word"] for c in doc.get("candidates", [])]


def main() -> int:
    ap = argparse.ArgumentParser(description="Gate 1 regression corpus runner.")
    ap.add_argument("--dict", required=True, help="pinyin dict to test (INPUTX_PINYIN_DICT)")
    ap.add_argument("--probe", default=None, help="path to inputx-probe (auto-detected if omitted)")
    ap.add_argument("--corpus", default=None, help="input_corpus.tsv (default tests/input_corpus.tsv)")
    ap.add_argument("--depth", type=int, default=1, help="position expected_top must be within (default 1 = strict pos 0)")
    ap.add_argument("--baseline", action="store_true", help="report-only: exit 0 even with failures")
    args = ap.parse_args()

    root = repo_root()
    dict_path = str(Path(args.dict).resolve())
    if not Path(dict_path).exists():
        sys.exit(f"error: --dict {dict_path} not found")
    probe = find_probe(root, args.probe)
    corpus = Path(args.corpus) if args.corpus else root / "tests" / "input_corpus.tsv"

    passed: list[str] = []
    failed: list[tuple[str, str, str, list[str]]] = []
    neg_passed: list[str] = []
    neg_failed: list[tuple[str, str, str, list[str]]] = []
    known_fail: list[tuple[str, str, str, list[str]]] = []  # ~ prefix, allowed
    promoted: list[tuple[str, str]] = []                    # ~ but now passes
    for raw in corpus.read_text().splitlines():
        line = raw.rstrip("\r\n")
        if not line or line.startswith("#"):
            continue
        parts = line.split("\t")
        if len(parts) < 3:
            continue
        code, expected, hint = parts[0], parts[1], parts[2]
        mode = HINT_TO_MODE.get(hint, "mixed")
        cands = run_probe(probe, dict_path, code, mode)
        if expected.startswith("!"):
            # CP3d negative case: forbidden word must NOT appear among candidates
            forbidden = expected[1:]
            if forbidden not in cands:
                neg_passed.append(code)
            else:
                neg_failed.append((code, forbidden, mode, cands[:5]))
            continue
        if expected.startswith("~"):
            # known-allowed fail (CP3-out-of-scope): doesn't block the gate, but
            # surface it if it starts passing so the ~ can be dropped (promoted).
            want = expected[1:]
            if want in cands[: args.depth]:
                promoted.append((code, want))
            else:
                known_fail.append((code, want, mode, cands[:5]))
            continue
        top = cands[: args.depth]
        if expected in top:
            passed.append(code)
        else:
            failed.append((code, expected, mode, cands[:5]))

    total = len(passed) + len(failed)
    neg_total = len(neg_passed) + len(neg_failed)
    print(f"# gate1 — input_corpus regression  (dict: {dict_path})")
    print(f"# probe: {probe}")
    print(f"# pass {len(passed)}/{total} strict  (depth {args.depth})")
    if neg_total:
        print(f"# negatives {len(neg_passed)}/{neg_total} (forbidden word absent)")
    if known_fail:
        print(f"# known-fail {len(known_fail)} (CP3-out-of-scope, allowed): "
              + " ".join(c for c, _, _, _ in known_fail))
    if promoted:
        print(f"# PROMOTE {len(promoted)} known-fail now PASS — drop the ~ prefix: "
              + " ".join(c for c, _ in promoted))
    if failed or neg_failed:
        print("#")
        print("code\texpected\tmode\tgot_top5")
        for code, expected, mode, got in failed:
            print(f"{code}\t{expected}\t{mode}\t{' '.join(got)}")
        for code, forbidden, mode, got in neg_failed:
            print(f"{code}\t!{forbidden} STILL PRESENT\t{mode}\t{' '.join(got)}")

    if (failed or neg_failed) and not args.baseline:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
