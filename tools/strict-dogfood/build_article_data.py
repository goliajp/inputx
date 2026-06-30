#!/usr/bin/env python3
"""build_article_data.py — for one article, batch-probe v2 via inputx-dogfood
(one process load) and emit structured JSON.

Usage:
    build_article_data.py <article_id> <segments.tsv>

segments.tsv format:
    idx\\tword\\tpinyin\\treason
"""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import argparse
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DOGFOOD = ROOT / "core/target/release/inputx-dogfood"
CORPUS = ROOT / "docs/pinyin-dogfood-2026-06-30/scratchpad/corpus_news/articles"


def find_article(article_id: str) -> tuple[str, str]:
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

    # Build TSV input for inputx-dogfood: article_id\tseg_idx\tword\tpinyin\tnotes
    in_rows = []
    seg_meta = []  # parallel list for reason
    with seg_path.open(encoding="utf-8") as f:
        for ln in f:
            ln = ln.rstrip("\n")
            if not ln.strip() or ln.startswith("#"):
                continue
            parts = ln.split("\t")
            if len(parts) < 3:
                continue
            idx, word, pinyin = parts[0], parts[1], parts[2]
            reason = parts[3] if len(parts) > 3 else ""
            in_rows.append(f"{article_id}\t{idx}\t{word}\t{pinyin}\t")
            seg_meta.append({"idx": idx, "expected": word, "pinyin": pinyin, "reason": reason})

    # Write input TSV + call inputx-dogfood with --all
    with tempfile.NamedTemporaryFile("w", suffix=".tsv", delete=False, encoding="utf-8") as tf:
        tf.write("\n".join(in_rows) + "\n")
        input_path = tf.name
    with tempfile.NamedTemporaryFile("w", suffix=".tsv", delete=False, encoding="utf-8") as tf:
        output_path = tf.name

    res = subprocess.run(
        [str(DOGFOOD), "--input", input_path, "--output", output_path, "--all"],
        capture_output=True, text=True
    )
    # Parse output
    by_idx: dict[str, dict] = {}
    for ln in Path(output_path).read_text(encoding="utf-8").splitlines():
        if not ln.strip() or ln.startswith("#"):
            continue
        cols = ln.split("\t")
        if len(cols) < 7:
            continue
        idx = cols[1]
        top10 = cols[6].split(",") if cols[6] else []
        rank = int(cols[5])
        by_idx[idx] = {"top10": top10, "rank": rank, "verdict": cols[4]}

    # Stitch
    segments = []
    for m in seg_meta:
        info = by_idx.get(m["idx"], {"top10": [], "rank": 99, "verdict": "HARD"})
        segments.append({**m, **info})

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
