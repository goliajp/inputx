#!/usr/bin/env python3
"""Harvest Chinese OpenSubtitles → word counts TSV.

Inputs: OPUS-distributed OpenSubtitles v2018 monolingual zh_cn dump
(plain text, one subtitle line per row, gzipped). Pulls fresh from
the OPUS endpoint on every run (cached locally).

OpenSubtitles is colloquial dialog text — the freq distribution
matches the *casual conversational* register that IME users actually
type, complementing Wikipedia's formal-prose bias. The v1.9.0 audit
(see `docs/v1.9.0-vNEXT-audit.md`) flagged wiki as freq-mismatched
to IME usage; OpenSubtitles is the v1.9.0 WU-π.b corpus pivot.

Outputs: `output/zh-opensubtitles/<date>.tsv`, format:
    word<TAB>count<TAB>source<TAB>fetched_at
Sorted by count descending. Header row included. Same downstream
shape as `harvest_zh_wikipedia.py` so `tools/dict-pipeline/run.sh`
can swap sources without further changes.

Reproducibility: OPUS v2018 is a frozen release; same script
version → byte-identical output (modulo fetched_at column).

Dependencies:
    - stdlib only for I/O + gzip.
    - jieba for Chinese word segmentation (same as wiki harvester).
      Install:  pip3 install jieba

License: OPUS-OpenSubtitles is CC-BY 4.0. See
`tools/corpus-harvest/LICENSES.md` for attribution requirements.

Run:
    python3 tools/corpus-harvest/harvest_zh_opensubtitles.py
    python3 tools/corpus-harvest/harvest_zh_opensubtitles.py --dry-run
"""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import re
import sys
import time
import urllib.request
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path

HERE = Path(__file__).resolve().parent
OUTPUT_DIR = HERE / "output" / "zh-opensubtitles"
CACHE_DIR = HERE / ".cache"
MANIFEST_PATH = HERE / "manifest.yaml"

SOURCE_NAME = "zh-opensubtitles"
LICENSE = "CC-BY-4.0"

# OPUS v2018 zh_cn monolingual dump. Frozen release, no `date`
# parameter needed (unlike wiki dumps). Versioned via OPUS release
# tag.
DUMP_URL = "https://object.pouta.csc.fi/OPUS-OpenSubtitles/v2018/mono/zh_cn.txt.gz"
DUMP_VERSION = "OPUS-v2018"

# Hard caps — keep output finite + reproducible memory footprint.
TOP_N_WORDS = 200_000          # write only the top N by count
MIN_COUNT = 2                  # drop hapax legomena (count=1)
DRY_RUN_LINES = 100_000        # smoke-test ceiling (vs wiki's article count)

CHINESE_CHAR_RE = re.compile(r"[一-鿿㐀-䶿]+")
# Subtitle artefacts to strip: HTML-ish tags, music/SFX annotations,
# speaker labels, timestamp leakage. OPUS preprocessed dumps are mostly
# clean but residuals exist.
SUB_TAG_RE = re.compile(r"<[^>]+>")
SUB_BRACKET_RE = re.compile(r"\[[^\]]+\]|\([^)]+\)")
SUB_DIALOG_DASH_RE = re.compile(r"^\s*-\s*", re.MULTILINE)


def require_jieba():
    """Probe jieba and exit with install hint if missing."""
    try:
        import jieba  # noqa: F401
        return jieba
    except ImportError:
        sys.stderr.write(
            "ERROR: jieba not installed. Required for Chinese word segmentation.\n"
            "Install:  pip3 install jieba\n"
            "(jieba is MIT-licensed pure-Python; doesn't ship with Inputx.)\n"
        )
        sys.exit(1)


def download(url: str, dest: Path) -> Path:
    """Download with simple progress + caching. Return path on disk."""
    CACHE_DIR.mkdir(parents=True, exist_ok=True)
    dest.parent.mkdir(parents=True, exist_ok=True)
    if dest.exists() and dest.stat().st_size > 0:
        sys.stderr.write(f"  cached: {dest.name} ({dest.stat().st_size:,} bytes)\n")
        return dest
    sys.stderr.write(f"  GET {url}\n")
    req = urllib.request.Request(
        url, headers={"User-Agent": "inputx-corpus-harvest/1.0"}
    )
    with urllib.request.urlopen(req, timeout=120) as r, open(dest, "wb") as f:
        total = int(r.headers.get("Content-Length", "0"))
        downloaded = 0
        chunk_size = 1 << 16
        last_print = time.time()
        while True:
            chunk = r.read(chunk_size)
            if not chunk:
                break
            f.write(chunk)
            downloaded += len(chunk)
            now = time.time()
            if now - last_print > 2.0:
                pct = (downloaded / total * 100) if total else 0
                sys.stderr.write(f"    {downloaded/1e6:.1f}MB  {pct:.1f}%\r")
                last_print = now
    sys.stderr.write(f"\n  downloaded: {dest.name} ({dest.stat().st_size:,} bytes)\n")
    return dest


def iter_lines(dump_path: Path, max_lines: int | None = None):
    """Stream-yield decoded subtitle lines, capped at max_lines."""
    yielded = 0
    with gzip.open(dump_path, "rt", encoding="utf-8", errors="replace") as f:
        for line in f:
            yield line
            yielded += 1
            if max_lines is not None and yielded >= max_lines:
                break


def strip_subtitle_markup(text: str) -> str:
    """Strip subtitle annotations + leading dialog dash."""
    text = SUB_TAG_RE.sub("", text)
    text = SUB_BRACKET_RE.sub("", text)
    text = SUB_DIALOG_DASH_RE.sub("", text)
    return text


def tokenize(text: str, jieba) -> list[str]:
    """Strip markup, extract Chinese runs, tokenize with jieba."""
    text = strip_subtitle_markup(text)
    words = []
    for match in CHINESE_CHAR_RE.finditer(text):
        run = match.group(0)
        # Same tokenization shape as wiki harvester for downstream
        # parity: cut_for_search yields atomic words + compounds.
        for tok in jieba.cut_for_search(run):
            if 2 <= len(tok) <= 6:
                words.append(tok)
            elif len(tok) == 1 and "一" <= tok <= "鿿":
                # keep 1-char too — single-char freq feeds dict
                words.append(tok)
    return words


def harvest(dry_run: bool, output_path: Path) -> dict:
    """Main pipeline. Returns stats dict."""
    jieba = require_jieba()

    dump_filename = DUMP_URL.rsplit("/", 1)[-1]
    dump_path = CACHE_DIR / f"opensubtitles-zh-cn-{DUMP_VERSION}.txt.gz"
    if not dump_path.exists():
        download(DUMP_URL, dump_path)
    else:
        sys.stderr.write(f"  cached: {dump_path.name} ({dump_path.stat().st_size:,} bytes)\n")

    counter: Counter[str] = Counter()
    line_count = 0
    max_lines = DRY_RUN_LINES if dry_run else None
    t_start = time.time()
    for line in iter_lines(dump_path, max_lines=max_lines):
        line_count += 1
        for word in tokenize(line, jieba):
            counter[word] += 1
        if line_count % 500_000 == 0:
            elapsed = time.time() - t_start
            sys.stderr.write(f"  parsed {line_count:,} lines  "
                             f"unique words {len(counter):,}  "
                             f"({elapsed:.0f}s)\n")

    # Filter + cap to TOP_N.
    above_min = [(w, c) for w, c in counter.items() if c >= MIN_COUNT]
    above_min.sort(key=lambda wc: (-wc[1], wc[0]))
    top = above_min[:TOP_N_WORDS]

    # Provenance.
    fetched_at = datetime.now(timezone.utc).strftime("%Y-%m-%d")
    dump_sha = hashlib.sha256(dump_path.read_bytes()[:1_000_000]).hexdigest()[:16]
    fetched_at_full = f"{fetched_at}#dump-sha:{dump_sha}"

    output_path.parent.mkdir(parents=True, exist_ok=True)
    with open(output_path, "w", encoding="utf-8") as f:
        f.write("word\tcount\tsource\tfetched_at\n")
        for word, count in top:
            f.write(f"{word}\t{count}\t{SOURCE_NAME}\t{fetched_at_full}\n")

    return {
        "lines_parsed": line_count,
        "unique_words_seen": len(counter),
        "unique_words_kept": len(top),
        "min_count_threshold": MIN_COUNT,
        "top_n_cap": TOP_N_WORDS,
        "dump_path": str(dump_path),
        "dump_sha256_prefix": dump_sha,
        "dump_version": DUMP_VERSION,
        "elapsed_sec": round(time.time() - t_start, 1),
        "output_path": str(output_path),
        "fetched_at": fetched_at_full,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help=f"Smoke test: parse only first {DRY_RUN_LINES:,} lines.",
    )
    args = parser.parse_args()

    # OPUS v2018 is a frozen release, but we still date-stamp the
    # output for cross-source parity with the wiki harvester.
    date = datetime.now(timezone.utc).strftime("%Y%m%d")
    suffix = "-dry" if args.dry_run else ""
    output_path = OUTPUT_DIR / f"{date}{suffix}.tsv"

    sys.stderr.write(f"[harvest] {SOURCE_NAME} → {output_path}\n")
    if args.dry_run:
        sys.stderr.write(f"  DRY RUN ({DRY_RUN_LINES:,}-line ceiling)\n")
    stats = harvest(args.dry_run, output_path)
    sys.stderr.write("=== stats ===\n")
    sys.stderr.write(json.dumps(stats, indent=2) + "\n")
    sys.stderr.write(
        f"wrote {stats['unique_words_kept']:,} words → {output_path}\n"
    )


if __name__ == "__main__":
    main()
