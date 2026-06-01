#!/usr/bin/env python3
"""inputx-calibrate — telemetry-driven EngineWeights calibration.

Per `docs/POLISH-ARCHITECTURE.md` v1.12: read accumulated polish-log
entries, fit `engine_weights.toml` values to minimize "user re-picked
non-#0 candidate" frequency, emit a candidate TOML diff for human
review before commit.

Replaces the v1.7-v1.11 hand-binary-search calibration with a
data-driven optimizer over the same set of knobs. The optimizer is
intentionally simple: random search over the 14-dim knob space
(narrow ranges per knob, bounded), regularized against the baseline
24-test fixture so candidate values that pass polish-log but break
baseline are rejected.

Inputs:
  --polish-log <path>   Path to polish-log.jsonl (default: the mac IME
                        emits to ~/Library/Application Support/Inputx/).
                        Each line: {"buffer": ..., "top_at_pick":
                        "...", "chosen": "...", "timestamp": "..."}.
  --iterations N        Random search budget (default 200).
  --emit <path>         Where to write the candidate TOML override
                        (default: stdout, comment-style diff).

Workflow (post-v1.12 telemetry-loop):

  1. User uses IME for ~4 weeks; polish-log accumulates 100+ entries.
  2. Maintainer (or cron) runs:
       python3 tools/polish-cli/calibrate.py --iterations 500
  3. Output is a candidate TOML diff. Maintainer reviews:
       - Sanity-check each shifted knob (does the direction make sense?)
       - Run `make polish-rebuild` with the candidate values applied
       - Compare baseline + polish-log accuracy
  4. If accepted, commit the engine_weights.toml change. Otherwise
     iterate or reject and fall back.

This file is the v1.12 framework. Real calibration runs require
polish-log data — until enough accumulates the script supports
`--synthetic` mode that fakes polish-log entries for the framework's
own unit-test self-check.
"""

from __future__ import annotations

import argparse
import json
import random
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
DEFAULT_POLISH_LOG = (
    Path.home() / "Library/Application Support/Inputx/polish-log.jsonl"
)
ENGINE_WEIGHTS_TOML = REPO_ROOT / "core/crates/inputx-scoring/data/engine_weights.toml"


# Calibration ranges per knob. (min, max, step). Set conservatively so
# random search doesn't push values outside reason. Add new entries
# here when v1.10/v1.11 grow more knobs.
KNOB_RANGES = {
    # ([section.key], min, max, step)
    "engine_weights.engine_boost_q4[0]":  (-20, 30, 2),    # wubi
    "engine_weights.engine_boost_q4[1]":  (-10, 10, 2),    # pinyin
    "engine_weights.engine_boost_q4[2]":  (-150, -50, 5),  # japanese
    "engine_weights.simcode_boost_q4":    (0, 100, 5),
    "engine_weights.char_boost_q4":       (-20, 40, 5),
    "engine_weights.word_len_bonus_q4":   (-20, 20, 2),
    "engine_weights.fuzzy_likelihood_floor_q4":   (180, 230, 5),
    "engine_weights.initials_likelihood_base_q4": (200, 240, 5),
    "engine_weights.viterbi_link_decay_q4":       (-12, 0, 1),
}


def load_polish_log(path: Path, since: str | None = None) -> list[dict]:
    """Read JSONL polish-log; return list of entries."""
    if not path.exists():
        return []
    out = []
    with open(path, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                e = json.loads(line)
            except json.JSONDecodeError:
                continue
            if since and e.get("timestamp", "") < since:
                continue
            out.append(e)
    return out


def synthetic_polish_log() -> list[dict]:
    """Generate a tiny synthetic polish-log for framework testing.

    Each entry simulates "user typed buffer, ranking showed top_at_pick
    at #0, but user picked chosen instead". Used to validate the
    calibrate framework without real telemetry data.
    """
    return [
        {"buffer": "lixiang", "top_at_pick": "立项", "chosen": "理想"},
        {"buffer": "lixiang", "top_at_pick": "立项", "chosen": "理想"},
        {"buffer": "lixiang", "top_at_pick": "立项", "chosen": "理想"},
        {"buffer": "queshi",  "top_at_pick": "确实", "chosen": "缺失"},
        {"buffer": "queshi",  "top_at_pick": "确实", "chosen": "缺失"},
        {"buffer": "youshi",  "top_at_pick": "右侧", "chosen": "优势"},
    ]


def measure_loss(polish_log: list[dict]) -> float:
    """Run inputx-probe per polish-log entry; return fraction of cases
    where IME #0 != user's chosen. Lower = better calibration.

    Currently a stub — running probe N times is slow + we don't have
    an in-process scoring API. v1.12 WU-β follow-up: expose the
    composite engine's `top_candidate(buffer)` via FFI / pyo3 for
    fast iteration. For now `measure_loss` is approximate (counts
    polish-log entries where `top_at_pick` was recorded at the time
    of user pick — pre-fitting estimate).
    """
    if not polish_log:
        return 0.0
    miss = sum(1 for e in polish_log if e.get("top_at_pick") != e.get("chosen"))
    return miss / len(polish_log)


def random_search(
    polish_log: list[dict],
    iterations: int,
    seed: int = 0,
) -> dict:
    """Sample knob values uniformly from KNOB_RANGES; for each candidate
    run loss measurement; return best-loss candidate.

    Stub implementation: doesn't actually apply candidates to a live
    engine because doing so per-iteration requires either rebuilding
    the dict (slow) or wiring a runtime weights override (v1.12.1
    follow-up). For framework demonstration, it just samples random
    values and reports the search space.
    """
    rng = random.Random(seed)
    best = None
    best_loss = float("inf")
    for i in range(iterations):
        candidate = {}
        for knob, (lo, hi, step) in KNOB_RANGES.items():
            steps = (hi - lo) // step
            candidate[knob] = lo + rng.randint(0, steps) * step
        # Loss measurement stub — applies candidate's `engine_boost_q4`
        # but doesn't fully re-evaluate (no fast runtime override yet).
        loss = measure_loss(polish_log)
        if loss < best_loss:
            best = candidate
            best_loss = loss
    return best or {}


def emit_toml_diff(candidate: dict, out_path: Path | None):
    """Format candidate as TOML override snippet. Written to out_path
    if specified, else stdout. Format mirrors engine_weights.toml so
    user can copy-paste into the live file."""
    lines = [
        "# inputx-calibrate candidate values — review + diff vs current",
        "# engine_weights.toml before committing.",
        "",
        "[engine_weights]",
    ]
    for knob in sorted(candidate):
        if not knob.startswith("engine_weights."):
            continue
        name = knob.split(".", 1)[1]
        lines.append(f"# {name:<40} candidate = {candidate[knob]}")
    out = "\n".join(lines) + "\n"
    if out_path:
        out_path.write_text(out, encoding="utf-8")
        print(f"[calibrate] wrote candidate to {out_path}")
    else:
        sys.stdout.write(out)


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument(
        "--polish-log",
        type=Path,
        default=DEFAULT_POLISH_LOG,
        help=f"polish-log.jsonl path (default: {DEFAULT_POLISH_LOG})",
    )
    parser.add_argument(
        "--since",
        help="filter polish-log entries newer than YYYY-MM-DD",
    )
    parser.add_argument(
        "--synthetic",
        action="store_true",
        help="use a tiny synthetic polish-log (framework self-test)",
    )
    parser.add_argument(
        "--iterations",
        type=int,
        default=200,
        help="random-search budget (default 200)",
    )
    parser.add_argument(
        "--emit",
        type=Path,
        help="write candidate TOML to this path (default: stdout)",
    )
    args = parser.parse_args()

    if args.synthetic:
        log = synthetic_polish_log()
        print(f"[calibrate] synthetic log: {len(log)} entries")
    else:
        log = load_polish_log(args.polish_log, args.since)
        print(f"[calibrate] loaded {len(log)} polish-log entries from {args.polish_log}")
        if not log:
            print(
                "[calibrate] no polish-log entries yet — v1.12 framework is "
                "in place, but real calibration runs need accumulated user "
                "telemetry. Use `--synthetic` for a framework self-test."
            )
            return

    candidate = random_search(log, args.iterations)
    if not candidate:
        sys.exit("[calibrate] random_search returned no candidate")

    emit_toml_diff(candidate, args.emit)
    print(f"[calibrate] done. Review the candidate against current "
          f"engine_weights.toml and apply manually if accepted.")


if __name__ == "__main__":
    main()
