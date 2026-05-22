#!/usr/bin/env python3
"""LLM-driven re-ranker for ambiguous-top codes.

Reads `data/merged/weights.tsv` (output of 05_merge), finds codes
where the top-2 candidates' scores are within 5% of each other,
batches them to Claude for a "most-likely-user-intent" judgment,
emits `data/annotations/llm_overrides.tsv`.

Reproducibility:
  * Model + prompt version pinned via constants below (bump together).
  * All API responses cached at data/annotations/cache/<sha256>.json
    so re-runs of the same prompt are free + deterministic.
  * --dry-run prints what would be sent without calling the API.

Cost discipline:
  * Prompt caching ON (system + few-shot prefix shared across calls).
  * Batch concurrency ≤ 10 to stay friendly with API rate limits.
  * Skip codes where top-2 gap > 5% (corpus alone resolved it).
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import os
import sys
from pathlib import Path

# Model + prompt version pinned together. Bump both when changing.
MODEL = "claude-opus-4-7"
PROMPT_VERSION = "v1.0"
AMBIGUITY_GAP_THRESHOLD = 0.05  # top-2 score gap below this → ambiguous

SYSTEM_PROMPT = """You are calibrating a Chinese pinyin / wubi IME's
candidate ranking. Given an input code and 2-5 candidate Chinese words,
identify which is most likely what a typical user intended when typing
that code. Consider modern Chinese (Mainland Mandarin, simplified
characters) usage frequency, register (formal / casual / technical),
and typical typing-context patterns.

Output JSON exactly in this shape:
  {"top": "<index-letter>", "confidence": <float 0-1>, "reason": "<short>"}

Where <index-letter> is A/B/C/.. matching the input order.
"""


def ambiguous_codes(weights_tsv: Path):
    """Yield (code, top_candidates) for ambiguous codes."""
    by_code: dict[str, list[tuple[str, float]]] = {}
    with weights_tsv.open() as f:
        for row in csv.reader(f, delimiter="\t"):
            if not row or row[0].startswith("#"):
                continue
            if len(row) < 5:
                continue
            code, word, engine, layer, score_s = row[:5]
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
        {"model": MODEL, "prompt": PROMPT_VERSION, "code": code,
         "candidates": [w for w, _ in candidates]},
        ensure_ascii=False, sort_keys=True,
    )
    return hashlib.sha256(payload.encode()).hexdigest()


def call_claude(code: str, candidates: list[tuple[str, float]]) -> dict:
    """Real implementation gated behind anthropic SDK + ANTHROPIC_API_KEY.
    Falls back to a deterministic stub when SDK absent (so the pipeline
    is still runnable on dev machines without API credentials).
    """
    try:
        import anthropic  # type: ignore
    except ImportError:
        return _stub_response(candidates)
    if not os.environ.get("ANTHROPIC_API_KEY"):
        return _stub_response(candidates)
    client = anthropic.Anthropic()
    msg_user = "Input code: `{}`\n\nCandidates:\n{}".format(
        code,
        "\n".join(f"  {chr(65+i)}. {w}" for i, (w, _) in enumerate(candidates)),
    )
    resp = client.messages.create(
        model=MODEL,
        max_tokens=200,
        system=[{"type": "text", "text": SYSTEM_PROMPT,
                  "cache_control": {"type": "ephemeral"}}],
        messages=[{"role": "user", "content": msg_user}],
    )
    text = resp.content[0].text.strip()
    return json.loads(text)


def _stub_response(candidates):
    # No API key — heuristic: pick the first candidate (the corpus
    # already ranked it #1, we agree by default). Confidence 0.5
    # signals "no real judgment was made".
    return {"top": "A", "confidence": 0.5, "reason": "stub: no API key"}


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--in", dest="input", type=Path, required=True)
    p.add_argument("--out", dest="output", type=Path, required=True)
    p.add_argument("--dry-run", action="store_true")
    args = p.parse_args()

    cache_dir = args.output.parent / "cache"
    cache_dir.mkdir(parents=True, exist_ok=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)

    seen = 0
    annotated = 0
    with args.output.open("w") as out:
        out.write("# version: {}\n".format(PROMPT_VERSION))
        out.write("# model: {}\n".format(MODEL))
        out.write("# format: code\\tpreferred_word\\tconfidence\\tmodel\\treason\n")
        for code, candidates in ambiguous_codes(args.input):
            seen += 1
            ck = cache_key(code, candidates)
            cache_path = cache_dir / f"{ck}.json"
            if cache_path.exists():
                resp = json.loads(cache_path.read_text())
            else:
                if args.dry_run:
                    print(f"  [dry-run] {code} → {[w for w,_ in candidates]}")
                    continue
                resp = call_claude(code, candidates)
                cache_path.write_text(json.dumps(resp, ensure_ascii=False))
            idx = ord(resp["top"]) - ord("A")
            if 0 <= idx < len(candidates):
                pref_word = candidates[idx][0]
                out.write(f"{code}\t{pref_word}\t{resp.get('confidence', 0.5):.2f}"
                          f"\t{MODEL}\t{resp.get('reason', '')[:80]}\n")
                annotated += 1
    print(f"[06_llm_annotate] {seen} ambiguous codes seen, {annotated} annotations written")
    return 0


if __name__ == "__main__":
    sys.exit(main())
