#!/usr/bin/env python3
"""LLM-driven re-ranker for ambiguous-top codes.

Reads `data/merged/weights.tsv` (output of 05_merge), finds codes
where the top-2 candidates' scores are within 5% of each other,
batches them to the `claude` CLI for a "most-likely-user-intent"
judgment, emits `data/annotations/llm_overrides.tsv`.

Why `claude` CLI (not the anthropic SDK with an API key):
  * Picks up the operator's local Claude Code auth — no API key
    juggling, no per-CI secret.
  * Same reproducibility story as SDK: response caching by prompt
    hash means re-runs are deterministic.
  * No Python deps beyond stdlib.

Reproducibility:
  * Prompt version pinned via constants below (bump when changing).
  * All responses cached at data/annotations/cache/<sha256>.json so
    re-runs of the same prompt are free + deterministic.
  * --dry-run prints what would be sent without invoking claude.

Cost discipline:
  * Skip codes where top-2 gap > AMBIGUITY_GAP_THRESHOLD (corpus
    alone resolved it).
  * Concurrency = 1 by default (CLI is process-heavy, not API-style).
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path

PROMPT_VERSION = "v1.0"
AMBIGUITY_GAP_THRESHOLD = 0.05  # top-2 score gap below this → ambiguous
CLI = os.environ.get("INPUTX_CLAUDE_BIN", "claude")

SYSTEM_PROMPT = """\
You are calibrating a Chinese pinyin / wubi IME's candidate ranking.
Given an input code and 2-5 candidate Chinese words, identify which is
most likely what a typical user intended when typing that code. Consider
modern Chinese (Mainland Mandarin, simplified characters) usage
frequency, register (formal / casual / technical), and typical
typing-context patterns.

Output a single line of strict JSON exactly in this shape, no surrounding
prose, no code fences:
  {"top": "<letter>", "confidence": <0-1 float>, "reason": "<≤30字>"}

<letter> is A/B/C/.. matching the input order.\
"""


def ambiguous_codes(weights_tsv: Path):
    """Yield (code, top_candidates) tuples for codes whose top-2 score
    gap is below AMBIGUITY_GAP_THRESHOLD."""
    by_code: dict[str, list[tuple[str, float]]] = {}
    with weights_tsv.open() as f:
        for row in csv.reader(f, delimiter="\t"):
            if not row or row[0].startswith("#"):
                continue
            if len(row) < 5:
                continue
            code, word, _engine, _layer, score_s = row[:5]
            try:
                score = float(score_s)
            except ValueError:
                continue
            by_code.setdefault(code, []).append((word, score))
    for code, items in by_code.items():
        items.sort(key=lambda t: -t[1])
        if len(items) < 2:
            continue
        top, second = items[0][1], items[1][1]
        if top <= 0:
            continue
        gap = (top - second) / top
        if gap < AMBIGUITY_GAP_THRESHOLD:
            yield code, items[:5]


def cache_key(code: str, candidates: list[tuple[str, float]]) -> str:
    payload = json.dumps(
        {"prompt": PROMPT_VERSION, "code": code,
         "candidates": [w for w, _ in candidates]},
        ensure_ascii=False, sort_keys=True,
    )
    return hashlib.sha256(payload.encode()).hexdigest()


def build_prompt(code: str, candidates: list[tuple[str, float]]) -> str:
    lines = [
        SYSTEM_PROMPT,
        "",
        f"Input code: `{code}`",
        "",
        "Candidates:",
    ]
    for i, (word, _score) in enumerate(candidates):
        lines.append(f"  {chr(65 + i)}. {word}")
    return "\n".join(lines)


def invoke_claude_cli(prompt: str) -> str:
    """Run `claude -p <prompt>` and return the assistant's plain-text
    reply. Uses `--output-format json` so we get a stable wrapper
    around the actual result string.
    """
    # `--output-format json` returns: {"type":"result", "result":"<text>",...}
    # We parse it and pull out the inner result, which is the model's
    # plain reply to our prompt (our system asks for one-line JSON).
    proc = subprocess.run(
        [CLI, "-p", "--output-format", "json", prompt],
        capture_output=True,
        text=True,
        timeout=60,
    )
    if proc.returncode != 0:
        raise RuntimeError(
            f"claude CLI exited {proc.returncode}: {proc.stderr.strip()}"
        )
    wrapper = json.loads(proc.stdout)
    if wrapper.get("is_error"):
        raise RuntimeError(
            f"claude CLI error: {wrapper.get('result', '<no message>')}"
        )
    return wrapper.get("result", "").strip()


def parse_claude_reply(raw: str) -> dict:
    """Pull the {"top":..., "confidence":..., "reason":...} JSON out of
    the model's reply. Tolerates surrounding whitespace / single-line
    fenced wrappers in case the model deviates from the strict format.
    """
    raw = raw.strip()
    # Tolerate ``` code fences if the model adds them.
    if raw.startswith("```"):
        lines = raw.splitlines()
        # drop the fence open + fence close
        raw = "\n".join(lines[1:-1]) if len(lines) >= 2 else raw
        raw = raw.strip()
    return json.loads(raw)


def _heuristic_stub(candidates):
    """When the CLI is unavailable, fall back to "trust the corpus" —
    pick the first candidate. Confidence 0.5 marks this as a stub.
    """
    return {"top": "A", "confidence": 0.5, "reason": "stub: cli unavailable"}


def annotate_one(code, candidates, *, dry_run, cli_ok):
    if not cli_ok:
        return _heuristic_stub(candidates)
    if dry_run:
        return {"top": "?", "confidence": 0.0, "reason": "dry-run"}
    prompt = build_prompt(code, candidates)
    raw = invoke_claude_cli(prompt)
    try:
        return parse_claude_reply(raw)
    except json.JSONDecodeError:
        # Model deviated from the spec. Don't crash the pipeline; fall
        # through to the heuristic stub and log the bad reply for
        # post-mortem.
        print(f"  [{code}] WARN: non-JSON reply, falling back to stub. raw={raw!r}")
        return _heuristic_stub(candidates)


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--in", dest="input", type=Path, required=True)
    p.add_argument("--out", dest="output", type=Path, required=True)
    p.add_argument("--dry-run", action="store_true")
    p.add_argument("--max", type=int, default=None,
                   help="stop after this many calls (for smoke tests)")
    args = p.parse_args()

    cache_dir = args.output.parent / "cache"
    cache_dir.mkdir(parents=True, exist_ok=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)

    # Probe the CLI once up front so we fail fast if it's not reachable.
    cli_ok = True
    if not args.dry_run:
        try:
            subprocess.run([CLI, "--version"], check=True,
                           capture_output=True, timeout=10)
        except (FileNotFoundError, subprocess.CalledProcessError,
                subprocess.TimeoutExpired) as e:
            print(f"  WARN: claude CLI unreachable ({e}); using stub for all calls")
            cli_ok = False

    seen = 0
    annotated = 0
    cached = 0
    with args.output.open("w") as out:
        out.write(f"# prompt-version: {PROMPT_VERSION}\n")
        out.write(f"# cli: {CLI}\n")
        out.write("# format: code\\tpreferred_word\\tconfidence\\treason\n")
        for code, candidates in ambiguous_codes(args.input):
            if args.max is not None and seen >= args.max:
                break
            seen += 1
            ck = cache_key(code, candidates)
            cache_path = cache_dir / f"{ck}.json"
            if cache_path.exists():
                resp = json.loads(cache_path.read_text())
                cached += 1
            else:
                resp = annotate_one(code, candidates,
                                    dry_run=args.dry_run, cli_ok=cli_ok)
                cache_path.write_text(json.dumps(resp, ensure_ascii=False))
            if args.dry_run:
                print(f"  [dry-run] {code} → {[w for w,_ in candidates]}")
                continue
            top_letter = resp.get("top", "")
            idx = ord(top_letter) - ord("A") if len(top_letter) == 1 else -1
            if 0 <= idx < len(candidates):
                pref_word = candidates[idx][0]
                out.write(
                    f"{code}\t{pref_word}\t{resp.get('confidence', 0.5):.2f}"
                    f"\t{resp.get('reason', '')[:80]}\n"
                )
                annotated += 1

    print(f"[06_llm_annotate] {seen} ambiguous codes, "
          f"{annotated} annotations written, {cached} from cache, "
          f"cli={'ok' if cli_ok else 'stub'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
