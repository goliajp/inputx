#!/usr/bin/env python3
"""build_article_data.py — for one article, probe v2 for each segment and
emit a structured JSON capturing current state.

Usage:
    build_article_data.py <article_id> <segments.tsv> [--before-snap path] [--out path]

segments.tsv format:
    seg_idx\\texpected\\tpinyin\\treason

If --before-snap is given, that path is read and the script labels current
output as "after" while keeping the snapshot as "before". Otherwise this
is a "before" snapshot.
"""

from __future__ import annotations

import json
import subprocess
import sys
import argparse
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PROBE = ROOT / "core/target/release/inputx-probe"
CORPUS = ROOT / "docs/pinyin-dogfood-2026-06-30/scratchpad/corpus_news/articles"


def probe_v2(buffer: str) -> list[str]:
    """Call inputx-probe in pinyin mode, return top-10 words."""
    res = subprocess.run(
        [str(PROBE), buffer, "--mode", "pinyin", "--pinyin", "v2"],
        capture_output=True, text=True, timeout=10,
    )
    if res.returncode != 0:
        return []
    try:
        data = json.loads(res.stdout)
        return [c["word"] for c in data.get("candidates", [])[:10]]
    except Exception:
        return []


def find_article(article_id: str) -> tuple[str, str]:
    """Return (title, raw_text) for article."""
    candidates = list(CORPUS.glob(f"{article_id}_*.txt"))
    if not candidates:
        return article_id, ""
    f = candidates[0]
    text = f.read_text(encoding="utf-8")
    title = text.split("\n", 1)[0].strip() if text else article_id
    return title, text


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("article_id")
    ap.add_argument("segments_tsv")
    ap.add_argument("--out", default=None)
    args = ap.parse_args()

    article_id = args.article_id
    seg_path = Path(args.segments_tsv)
    out_path = Path(args.out) if args.out else (
        ROOT / "docs/pinyin-dogfood-2026-06-30/strict/data" / f"{article_id}.json"
    )

    title, raw_text = find_article(article_id)
    cjk_count = sum(1 for c in raw_text if '一' <= c <= '鿿')

    segments = []
    with seg_path.open(encoding="utf-8") as f:
        for ln in f:
            ln = ln.rstrip("\n")
            if not ln.strip() or ln.startswith("#"):
                continue
            parts = ln.split("\t")
            if len(parts) < 3:
                continue
            idx, expected, pinyin = parts[0], parts[1], parts[2]
            reason = parts[3] if len(parts) > 3 else ""
            top10 = probe_v2(pinyin)
            rank = top10.index(expected) if expected in top10 else 99
            verdict = "PASS" if rank == 0 else ("SOFT" if rank < 99 else "HARD")
            segments.append({
                "idx": idx,
                "expected": expected,
                "pinyin": pinyin,
                "reason": reason,
                "top10": top10,
                "rank": rank,
                "verdict": verdict,
            })

    stats = {
        "total": len(segments),
        "pass": sum(1 for s in segments if s["verdict"] == "PASS"),
        "soft": sum(1 for s in segments if s["verdict"] == "SOFT"),
        "hard": sum(1 for s in segments if s["verdict"] == "HARD"),
    }

    out = {
        "id": article_id,
        "title": title,
        "raw_text": raw_text,
        "cjk_char_count": cjk_count,
        "segments": segments,
        "stats": stats,
    }
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(out, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"wrote {out_path} ({stats['pass']}/{stats['total']} PASS)")


if __name__ == "__main__":
    main()
