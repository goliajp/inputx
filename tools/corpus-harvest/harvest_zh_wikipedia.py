#!/usr/bin/env python3
"""Harvest Chinese Wikipedia → word counts TSV.

Inputs: Wikipedia 中文 XML dump (.bz2 / .gz). Pulls fresh from
https://dumps.wikimedia.org/zhwiki/ on every run (cached locally).

Outputs: `output/zh-wikipedia/<date>.tsv`, format:
    word<TAB>count<TAB>source<TAB>fetched_at
Sorted by count descending. Header row included.

Reproducibility: same `--date` + same script version → byte-identical
output (modulo fetched_at column, which carries the run's date for
provenance — doesn't affect downstream dict comparisons).

Dependencies:
    - stdlib only for I/O + XML.
    - jieba (https://github.com/fxsjy/jieba) for Chinese word
      segmentation. Install:  pip3 install jieba
      Reason: stdlib can't segment Chinese into words; n-gram sliding
      produces too many false compounds (e.g. "国人" from "中国人民")
      that pollute downstream training. jieba is pure-Python, MIT-
      licensed, and the de-facto Chinese tokenizer.

Run:
    python3 tools/corpus-harvest/harvest_zh_wikipedia.py --date 20260501
    python3 tools/corpus-harvest/harvest_zh_wikipedia.py --dry-run
"""

from __future__ import annotations

import argparse
import bz2
import gzip
import hashlib
import json
import re
import sys
import time
import urllib.request
import xml.etree.ElementTree as ET
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path

HERE = Path(__file__).resolve().parent
OUTPUT_DIR = HERE / "output" / "zh-wikipedia"
CACHE_DIR = HERE / ".cache"
MANIFEST_PATH = HERE / "manifest.yaml"

SOURCE_NAME = "zh-wikipedia"
LICENSE = "CC-BY-SA-4.0"

# Hard caps — keep output finite + reproducible memory footprint.
TOP_N_WORDS = 200_000          # write only the top N by count
MIN_COUNT = 2                  # drop hapax legomena (count=1)
DRY_RUN_ARTICLES = 100         # smoke-test ceiling

# Wiki markup the regex strips before tokenization. Order matters —
# strip nested constructs outer-first.
WIKI_REF_RE = re.compile(r"<ref[^>]*>.*?</ref>|<ref[^/]*/>", re.DOTALL)
WIKI_TAG_RE = re.compile(r"<[^>]+>")
WIKI_TPL_RE = re.compile(r"\{\{[^{}]*\}\}")
WIKI_LINK_RE = re.compile(r"\[\[([^|\]]*\|)?([^\]]*)\]\]")
WIKI_EXT_RE = re.compile(r"\[https?://[^\s\]]+\s*([^\]]*)\]")
WIKI_HEAD_RE = re.compile(r"^=+([^=]+)=+$", re.MULTILINE)
WIKI_BOLDIT_RE = re.compile(r"'{2,5}")
CHINESE_CHAR_RE = re.compile(r"[一-鿿㐀-䶿]+")


def require_jieba():
    """Probe jieba and exit with install hint if missing.

    Importing inside the function so `--help` works without the dep.
    """
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


def parse_manifest() -> dict:
    """Minimal YAML parser — pulls just what we need (url_template +
    sample_date) without adding pyyaml as a dep. The manifest schema
    is intentionally flat enough to parse by hand."""
    text = MANIFEST_PATH.read_text(encoding="utf-8")
    sources = {}
    current = None
    for line in text.splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        if stripped.startswith("- name:"):
            current = {"name": stripped.split(":", 1)[1].strip()}
            sources[current["name"]] = current
        elif current is not None and ":" in stripped:
            k, v = stripped.split(":", 1)
            k = k.strip(); v = v.strip().strip('"').strip("'")
            current[k] = v
    return sources


def url_for(date: str) -> tuple[str, str]:
    """Return (primary_url, fallback_url) given a date string."""
    src = parse_manifest()[SOURCE_NAME]
    primary = src["url_template"].format(date=date)
    fallback = src.get("url_fallback", "")
    return primary, fallback


def download(url: str, dest: Path) -> Path:
    """Download with simple progress + caching. Return path on disk."""
    CACHE_DIR.mkdir(parents=True, exist_ok=True)
    dest.parent.mkdir(parents=True, exist_ok=True)
    if dest.exists() and dest.stat().st_size > 0:
        sys.stderr.write(f"  cached: {dest.name} ({dest.stat().st_size:,} bytes)\n")
        return dest
    sys.stderr.write(f"  GET {url}\n")
    req = urllib.request.Request(url, headers={"User-Agent": "inputx-corpus-harvest/1.0"})
    with urllib.request.urlopen(req, timeout=60) as r, open(dest, "wb") as f:
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


def open_dump(path: Path):
    """Open a Wikipedia dump file with the right decompressor."""
    if path.suffix == ".bz2":
        return bz2.open(path, "rb")
    if path.suffix == ".gz":
        return gzip.open(path, "rb")
    return open(path, "rb")


def strip_wiki_markup(text: str) -> str:
    """Strip wiki markup down to plain text. Order matters."""
    text = WIKI_REF_RE.sub("", text)
    text = WIKI_TPL_RE.sub("", text)
    text = WIKI_TAG_RE.sub("", text)
    text = WIKI_EXT_RE.sub(r"\1", text)
    text = WIKI_LINK_RE.sub(r"\2", text)
    text = WIKI_HEAD_RE.sub(r"\1", text)
    text = WIKI_BOLDIT_RE.sub("", text)
    return text


def iter_articles(dump_path: Path, max_articles: int | None = None):
    """Stream-yield (title, text) for each <page> in a Wikipedia dump.

    Memory-bounded: uses iterparse with elem.clear() after each page.
    """
    ns = "{http://www.mediawiki.org/xml/export-0.11/}"
    fd = open_dump(dump_path)
    yielded = 0
    try:
        for _event, elem in ET.iterparse(fd, events=("end",)):
            if not elem.tag.endswith("page"):
                continue
            title_el = elem.find(f"{ns}title")
            ns_el = elem.find(f"{ns}ns")
            rev = elem.find(f"{ns}revision")
            text_el = rev.find(f"{ns}text") if rev is not None else None
            # ns=0 = main namespace (articles). Skip talk pages,
            # user pages, files, etc.
            if ns_el is None or ns_el.text != "0":
                elem.clear()
                continue
            title = title_el.text if title_el is not None else ""
            text = (text_el.text or "") if text_el is not None else ""
            elem.clear()
            if not title or not text:
                continue
            yield title, text
            yielded += 1
            if max_articles is not None and yielded >= max_articles:
                break
    finally:
        fd.close()


def tokenize(text: str, jieba) -> list[str]:
    """Strip markup, extract Chinese runs, tokenize with jieba."""
    text = strip_wiki_markup(text)
    words = []
    for match in CHINESE_CHAR_RE.finditer(text):
        run = match.group(0)
        # Tokenize the run with jieba in cut-for-search mode — yields
        # both atomic words AND longer compounds, which matches dict
        # coverage needs better than default cut mode.
        for tok in jieba.cut_for_search(run):
            if 2 <= len(tok) <= 6:
                words.append(tok)
            elif len(tok) == 1 and "一" <= tok <= "鿿":
                # keep 1-char too — single-char freq feeds dict
                words.append(tok)
    return words


def harvest(date: str, dry_run: bool, output_path: Path) -> dict:
    """Main pipeline. Returns stats dict."""
    jieba = require_jieba()

    primary_url, fallback_url = url_for(date)
    dump_filename = primary_url.rsplit("/", 1)[-1]
    dump_path = CACHE_DIR / dump_filename

    try:
        download(primary_url, dump_path)
    except Exception as e:
        if not fallback_url:
            raise
        sys.stderr.write(f"  primary failed ({e}), trying fallback\n")
        dump_filename = fallback_url.rsplit("/", 1)[-1]
        dump_path = CACHE_DIR / dump_filename
        download(fallback_url, dump_path)

    counter: Counter[str] = Counter()
    article_count = 0
    max_articles = DRY_RUN_ARTICLES if dry_run else None
    t_start = time.time()
    for title, text in iter_articles(dump_path, max_articles=max_articles):
        article_count += 1
        for word in tokenize(text, jieba):
            counter[word] += 1
        if article_count % 5000 == 0:
            elapsed = time.time() - t_start
            sys.stderr.write(f"  parsed {article_count:,} articles  "
                             f"unique words {len(counter):,}  "
                             f"({elapsed:.0f}s)\n")

    # Filter + cap to TOP_N.
    above_min = [(w, c) for w, c in counter.items() if c >= MIN_COUNT]
    above_min.sort(key=lambda wc: (-wc[1], wc[0]))
    top = above_min[:TOP_N_WORDS]

    # Provenance: include input dump's sha256 in fetched_at metadata
    # so downstream can prove which dump fed this output.
    fetched_at = datetime.now(timezone.utc).strftime("%Y-%m-%d")
    dump_sha = hashlib.sha256(dump_path.read_bytes()[:1_000_000]).hexdigest()[:16]
    fetched_at_full = f"{fetched_at}#dump-sha:{dump_sha}"

    output_path.parent.mkdir(parents=True, exist_ok=True)
    with open(output_path, "w", encoding="utf-8") as f:
        f.write("word\tcount\tsource\tfetched_at\n")
        for word, count in top:
            f.write(f"{word}\t{count}\t{SOURCE_NAME}\t{fetched_at_full}\n")

    return {
        "articles_parsed": article_count,
        "unique_words_seen": len(counter),
        "unique_words_kept": len(top),
        "min_count_threshold": MIN_COUNT,
        "top_n_cap": TOP_N_WORDS,
        "dump_path": str(dump_path),
        "dump_sha256_prefix": dump_sha,
        "elapsed_sec": round(time.time() - t_start, 1),
        "output_path": str(output_path),
        "fetched_at": fetched_at_full,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument(
        "--date",
        help="Dump date YYYYMMDD. Defaults to manifest.yaml sample_date.",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help=f"Smoke test: parse only first {DRY_RUN_ARTICLES} articles.",
    )
    args = parser.parse_args()

    date = args.date
    if not date:
        date = parse_manifest()[SOURCE_NAME].get("sample_date", "")
        if not date:
            sys.stderr.write("ERROR: no --date given and manifest has no sample_date\n")
            sys.exit(2)
        sys.stderr.write(f"using sample_date from manifest: {date}\n")

    output_path = OUTPUT_DIR / f"{date}{'-dry' if args.dry_run else ''}.tsv"

    sys.stderr.write(f"\n=== zh-wikipedia harvest ===\n")
    sys.stderr.write(f"  date: {date}\n")
    sys.stderr.write(f"  output: {output_path}\n")
    sys.stderr.write(f"  dry_run: {args.dry_run}\n\n")

    stats = harvest(date, args.dry_run, output_path)

    sys.stderr.write(f"\n=== stats ===\n")
    sys.stderr.write(json.dumps(stats, indent=2, ensure_ascii=False) + "\n")
    sys.stderr.write(f"\nwrote {stats['unique_words_kept']:,} words → {output_path}\n")


if __name__ == "__main__":
    main()
