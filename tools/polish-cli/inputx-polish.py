#!/usr/bin/env python3
"""inputx-polish — single-entry CLI for the v1.11 polish workflow.

Per `docs/POLISH-ARCHITECTURE.md`: takes a user-reported polish action
(rank fix or missing-word add), computes the minimum boost magnitude
needed, writes to the right overlay TSV, runs `make polish-rebuild`,
and reports the baseline gate result.

Subcommands:
  pick <buffer> <chosen>             User picked `<chosen>` at typed
                                     `<buffer>` and it wasn't #0. Auto-
                                     compute the boost to flip ranking
                                     and write to quickfix_boost.tsv.
  add-phrase <word> --engine wubi    Add a wubi phrase entry (computes
                                     wubi-86 code automatically).
                                     `--engine pinyin` writes to
                                     pinyin_modern_v1.tsv instead.
  show <buffer>                      Display the current top-10 for a
                                     buffer (Mixed mode). Diagnostic.

Examples:
  inputx-polish pick lixiang 理想 --reason "user-report-2026-06-15"
  inputx-polish add-phrase 不是 --engine wubi
  inputx-polish show lixiang
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
QUICKFIX_PATH = REPO_ROOT / "tools/scoring/data/polish_reports/quickfix_boost.tsv"
MODERN_PATH = REPO_ROOT / "tools/scoring/data/polish/modern_vocab_v1.tsv"
WUBI_PHRASES_PATH = REPO_ROOT / "core/crates/inputx-wubi/data/phrases.txt"
PINYIN_WEIGHTS_PATH = REPO_ROOT / "core/crates/inputx-pinyin/data/weights/weights.tsv"


def run_cmd(cmd: list[str], cwd: Path | None = None, capture: bool = False) -> str:
    """Run shell command, return stdout. Echo to stderr."""
    sys.stderr.write(f"$ {' '.join(cmd)}\n")
    if capture:
        result = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, check=True)
        return result.stdout
    subprocess.run(cmd, cwd=cwd, check=True)
    return ""


def lookup_top10(buffer: str, mode: str = "Mixed") -> list[tuple[str, str, float]]:
    """Run inputx-probe to get top-10 for buffer. Returns list of
    (word, source, score) tuples."""
    out = run_cmd(
        [
            "cargo", "run", "--quiet", "--release", "--bin", "inputx-probe",
            "--", buffer, "--mode", mode.lower(),
        ],
        cwd=REPO_ROOT / "core",
        capture=True,
    )
    data = json.loads(out)
    return [
        (c["word"], c["source"], float(c.get("score", 0.0)))
        for c in data.get("candidates", [])[:10]
    ]


def weights_freq(buffer: str, word: str) -> int:
    """Look up the current pinyin weights.tsv freq for (buffer, word).
    Returns 0 if not present."""
    if not PINYIN_WEIGHTS_PATH.exists():
        return 0
    with open(PINYIN_WEIGHTS_PATH, encoding="utf-8") as f:
        for line in f:
            if line.startswith("#") or "\t" not in line:
                continue
            parts = line.rstrip("\n").split("\t")
            if len(parts) >= 3 and parts[0] == buffer and parts[1] == word:
                try:
                    return int(parts[2])
                except ValueError:
                    return 0
    return 0


def append_quickfix_row(buffer: str, word: str, freq: int, comment: str):
    """Append a row to quickfix_boost.tsv. MAX overlay semantics — the
    eventual dict freq = max(base, this)."""
    QUICKFIX_PATH.parent.mkdir(parents=True, exist_ok=True)
    with open(QUICKFIX_PATH, "a", encoding="utf-8") as f:
        f.write(f"{buffer}\t{word}\t{freq}\t# {comment}\n")
    print(f"[polish] wrote {QUICKFIX_PATH}: {buffer}\\t{word}\\t{freq}")


def cmd_pick(args):
    """`inputx-polish pick <buffer> <chosen>` — write a quickfix_boost row
    that flips `<chosen>` to rank #0 at `<buffer>`. Magnitude = current
    top entry's freq + 10% margin."""
    buffer = args.buffer
    chosen = args.chosen
    reason = args.reason or "user pick"

    print(f"[polish] looking up current top10 for `{buffer}` (Mixed mode)...")
    top10 = lookup_top10(buffer, "Mixed")
    if not top10:
        sys.exit(f"[polish] error: no candidates for `{buffer}`")
    current_top = top10[0]
    if current_top[0] == chosen:
        print(f"[polish] `{chosen}` already #0 for `{buffer}` — nothing to do.")
        return

    # Find the chosen word in top10 (must be present — if absent, this is
    # an `add-phrase` not a `pick`).
    chosen_idx = next((i for i, (w, _, _) in enumerate(top10) if w == chosen), None)
    if chosen_idx is None:
        sys.exit(
            f"[polish] error: `{chosen}` not in top10 for `{buffer}` "
            f"(top10={[w for w,_,_ in top10]}). Use `add-phrase` if missing."
        )

    chosen_base = weights_freq(buffer, chosen)
    top_base = weights_freq(buffer, current_top[0])
    boost = top_base + max(int(top_base * 0.1), 1000)
    print(
        f"[polish] current #0 = {current_top[0]} (base {top_base}); "
        f"chosen = {chosen} (base {chosen_base}, currently #{chosen_idx+1}). "
        f"computing boost = top_base + 10% margin = {boost}"
    )
    append_quickfix_row(buffer, chosen, boost, f"{reason} | top={current_top[0]} base={chosen_base}")
    run_make_polish_rebuild()
    verify_top0(buffer, chosen)


def wubi_encode_phrase(word: str) -> str:
    """Compute the wubi-86 phrase code for `word` using the existing
    facade encoder. Returns the 4-letter code string.

    Calls a small Rust wrapper that invokes `inputx_wubi::encode_phrase`
    (or equivalent) to avoid duplicating the encoding rules in Python.
    """
    # The wubi facade exposes an `encode_phrase` helper through the
    # `wubi-encode` binary (added v1.11 WU-β). Fall back to a stub if
    # the binary doesn't exist yet — user must add manually.
    try:
        out = run_cmd(
            ["cargo", "run", "--quiet", "--features", "tools", "--release",
             "--bin", "wubi-encode", "--", word],
            cwd=REPO_ROOT / "core",
            capture=True,
        )
        return out.strip()
    except (subprocess.CalledProcessError, FileNotFoundError):
        sys.exit(
            f"[polish] error: wubi-encode binary not found. v1.11 WU-β "
            f"hasn't shipped the encoder helper yet. Compute the code "
            f"manually per wubi-86 rules and add the row directly to "
            f"{WUBI_PHRASES_PATH}:\n"
            f"  <code>\\t{word}\n"
        )


def cmd_add_phrase(args):
    word = args.word
    engine = args.engine
    if engine == "wubi":
        code = wubi_encode_phrase(word)
        print(f"[polish] wubi-86 encoder: {word} → {code}")
        with open(WUBI_PHRASES_PATH, "a", encoding="utf-8") as f:
            f.write(f"{code}\t{word}\n")
        print(f"[polish] appended to {WUBI_PHRASES_PATH}")
        run_make_polish_rebuild()
    elif engine == "pinyin":
        sys.exit(
            f"[polish] pinyin add-phrase: implement v1.11 WU-β.2 — needs "
            f"jieba tokenization check first to avoid bigram-fragment "
            f"pollution per docs/POLISH-ARCHITECTURE.md option (ii) "
            f"discussion."
        )
    else:
        sys.exit(f"[polish] unknown engine: {engine}")


def cmd_show(args):
    buffer = args.buffer
    top10 = lookup_top10(buffer, "Mixed")
    print(f"[polish] top10 for `{buffer}` (Mixed mode):")
    for i, (w, src, s) in enumerate(top10):
        print(f"  #{i:<2} {w:<10} ({src})  score={s:.1f}")


def run_make_polish_rebuild():
    """Run `make polish-rebuild` from repo root. Fails out on baseline regression."""
    print("[polish] running `make polish-rebuild` ...")
    try:
        run_cmd(["make", "polish-rebuild"], cwd=REPO_ROOT)
    except subprocess.CalledProcessError:
        sys.exit("[polish] rebuild/baseline failed — fix manually or revert the overlay row")


def verify_top0(buffer: str, expected: str):
    print(f"[polish] verifying `{buffer}` now leads with `{expected}` ...")
    top10 = lookup_top10(buffer, "Mixed")
    if not top10:
        sys.exit(f"[polish] verify: no candidates for {buffer}")
    actual = top10[0][0]
    if actual == expected:
        print(f"[polish] ✓ `{buffer}` → `{expected}` confirmed.")
    else:
        sys.exit(f"[polish] ✗ expected `{expected}` got `{actual}` — boost was insufficient.")


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = parser.add_subparsers(dest="cmd", required=True)

    p_pick = sub.add_parser("pick", help="Re-rank a candidate to #0 at a buffer.")
    p_pick.add_argument("buffer")
    p_pick.add_argument("chosen")
    p_pick.add_argument("--reason", help="Audit-log reason string.")
    p_pick.set_defaults(func=cmd_pick)

    p_add = sub.add_parser("add-phrase", help="Add a missing phrase to dict.")
    p_add.add_argument("word")
    p_add.add_argument("--engine", choices=["wubi", "pinyin"], required=True)
    p_add.set_defaults(func=cmd_add_phrase)

    p_show = sub.add_parser("show", help="Show top-10 for a buffer.")
    p_show.add_argument("buffer")
    p_show.set_defaults(func=cmd_show)

    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
