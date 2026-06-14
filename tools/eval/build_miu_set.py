#!/usr/bin/env python3
"""Build MIU evaluation set from corpus raw downloads.

MIU = Maximum Input Unit (Jia & Zhao 2013, KySS):
  the longest run of contiguous CJK ideographs bounded by non-CJK
  characters. Used as the unit of measurement for IME top-K accuracy.

Sources (must complete CP-0.1 first):
- tools/eval/corpus/raw/wiki/zhwiki-latest-pages-articles.xml.bz2
- tools/eval/corpus/raw/pd/THUCNews.zip (selected categories)

Output:
- tools/eval/miu_set.tsv  (one MIU per line, deduplicated)

Determinism: --seed flag controls all randomness (sampling).
Same seed + same inputs + same caps -> byte-equal output.

Notes:
- We do NOT use jieba here. MIU extraction is regex-only over CJK
  ranges; jieba is needed for LM corpus tokenization, deferred to
  Phase 2 (CP-2.2 cleaning pipeline).
- We do NOT use MinHash. Exact-string dedup is sufficient at our
  scale (~50k MIUs); MinHash deferred to Phase 2 if needed.
"""

from __future__ import annotations

import argparse
import bz2
import random
import re
import sys
import zipfile
from pathlib import Path
from xml.etree.ElementTree import iterparse

ROOT = Path(__file__).resolve().parent.parent.parent  # repo root
RAW = ROOT / "tools" / "eval" / "corpus" / "raw"
DEFAULT_OUT = ROOT / "tools" / "eval" / "miu_set.tsv"

WIKI_PATH = RAW / "wiki" / "zhwiki-latest-pages-articles.xml.bz2"
NEWS_PATH = RAW / "pd" / "THUCNews.zip"

# MIU = run of CJK ideographs (Unihan basic block; ignore extensions
# for Phase 0 — they're rare in news/encyclopedia text)
CJK = re.compile(r"[一-鿿]+")

# Sentence boundaries: Chinese terminal punctuation + newline
SENT_BREAK = re.compile(r"[。!?…;.\n\r]+")

# Wiki markup we strip ROUGHLY before extracting CJK (lightweight,
# no mwparserfromhell dependency)
WIKI_RE_TEMPLATE = re.compile(r"\{\{[^{}]*\}\}")
WIKI_RE_REF = re.compile(r"<ref[^>]*?(?:/>|>.*?</ref>)", re.DOTALL)
WIKI_RE_TAG = re.compile(r"<[^>]+>")
WIKI_RE_LINK = re.compile(r"\[\[(?:[^|\]]+\|)?([^\]]+)\]\]")
WIKI_RE_HEADER = re.compile(r"^={2,}.*={2,}$", re.MULTILINE)
WIKI_RE_TABLE = re.compile(r"\{\|.*?\|\}", re.DOTALL)
WIKI_NS = "{http://www.mediawiki.org/xml/export-0.11/}"
WIKI_SKIP_TITLE_PREFIXES = (
    "Wikipedia:", "Template:", "File:", "Category:", "Help:",
    "Portal:", "Draft:", "User:", "MediaWiki:", "Module:",
    "Talk:", "User talk:", "Template talk:", "Category talk:",
    "Wikipedia talk:", "Help talk:", "Portal talk:",
)

# THUCNews has 14 categories; we sample a representative mix
THUCNEWS_CATEGORIES = ["财经", "科技", "社会", "时政", "教育"]

# Length filter for MIUs (characters, not bytes).
# Rationale: a single IME input session is rarely > 20 chars; longer
# CJK runs reflect long uninterrupted writing (article paragraphs)
# and dominate the sampled set if not capped, hurting representativeness.
# Phase 0 default targets typical IME input range.
MIN_MIU_LEN = 4
MAX_MIU_LEN = 20

# Input caps (keep script finishing in minutes).
# Wiki cap is small enough that THUCNews still contributes unique
# news-flavored MIUs (proper nouns, brands, etc.). With wiki=3000
# we get ~500k unique MIUs, news adds ~30k more — good mix.
DEFAULT_WIKI_PAGES = 3_000
DEFAULT_NEWS_PER_CAT = 800
DEFAULT_TARGET = 50_000


def strip_wiki_markup(s: str) -> str:
    """Lightweight wiki markup stripper.

    Not perfect — leaves some artifacts — but good enough for
    extracting CJK runs (we only care about hanzi content, not
    structure).
    """
    s = WIKI_RE_REF.sub("", s)
    s = WIKI_RE_TABLE.sub("", s)
    # Templates can nest; iterate a few times
    for _ in range(3):
        new = WIKI_RE_TEMPLATE.sub("", s)
        if new == s:
            break
        s = new
    s = WIKI_RE_LINK.sub(lambda m: m.group(1), s)
    s = WIKI_RE_HEADER.sub("", s)
    s = WIKI_RE_TAG.sub("", s)
    return s


def iter_wiki_pages(bz2_path: Path, limit: int):
    """Stream-yield body text from wiki dump; bounded by `limit` pages."""
    n = 0
    with bz2.open(bz2_path, "rb") as f:
        context = iterparse(f, events=("start", "end"))
        _, root = next(context)
        for ev, el in context:
            if ev != "end":
                continue
            if el.tag != f"{WIKI_NS}page":
                continue
            title_el = el.find(f"{WIKI_NS}title")
            title = title_el.text if title_el is not None and title_el.text else ""
            if not any(title.startswith(p) for p in WIKI_SKIP_TITLE_PREFIXES):
                rev_el = el.find(f"{WIKI_NS}revision")
                if rev_el is not None:
                    text_el = rev_el.find(f"{WIKI_NS}text")
                    if text_el is not None and text_el.text:
                        yield strip_wiki_markup(text_el.text)
                        n += 1
            # free memory — both the page and root's accumulating children
            el.clear()
            root.clear()
            if n >= limit:
                return


def _fix_zip_name(name: str) -> str:
    """THUCNews.zip stores UTF-8-encoded CJK filenames but doesn't set the
    UTF-8 flag (flag_bits 0x800), so zipfile mis-decodes them as cp437.
    Round-trip through cp437 -> UTF-8 to recover.
    """
    try:
        return name.encode("cp437").decode("utf-8")
    except (UnicodeDecodeError, UnicodeEncodeError):
        return name


def iter_thucnews(zip_path: Path, categories: list, max_per_cat: int):
    """Yield body text from selected THUCNews categories."""
    counts = {c: 0 for c in categories}
    with zipfile.ZipFile(zip_path) as z:
        for info in z.infolist():
            if not info.filename.endswith(".txt"):
                continue
            if info.filename.startswith("__MACOSX"):
                continue
            fixed = _fix_zip_name(info.filename)
            parts = fixed.split("/")
            # Expect structure THUCNews/{category}/{id}.txt
            if len(parts) < 3:
                continue
            cat = parts[1]
            if cat not in counts:
                continue
            if counts[cat] >= max_per_cat:
                continue
            # NB: use info (not fixed name) for z.open — zip lookup is
            # by raw stored name, not our decoded one.
            with z.open(info) as f:
                yield f.read().decode("utf-8", errors="replace")
            counts[cat] += 1
            if all(c >= max_per_cat for c in counts.values()):
                return


def to_mius(text: str):
    """Yield MIU strings from text after sentence-splitting."""
    for sent in SENT_BREAK.split(text):
        for m in CJK.findall(sent):
            if MIN_MIU_LEN <= len(m) <= MAX_MIU_LEN:
                yield m


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--seed", type=int, default=42,
                        help="RNG seed (default 42); deterministic output for fixed seed+inputs+caps")
    parser.add_argument("--target", type=int, default=DEFAULT_TARGET,
                        help=f"target MIU count after sampling (default {DEFAULT_TARGET})")
    parser.add_argument("--out", default=str(DEFAULT_OUT),
                        help="output TSV path")
    parser.add_argument("--wiki-pages", type=int, default=DEFAULT_WIKI_PAGES,
                        help=f"max wiki articles to scan (default {DEFAULT_WIKI_PAGES})")
    parser.add_argument("--news-per-cat", type=int, default=DEFAULT_NEWS_PER_CAT,
                        help=f"max THUCNews files per category (default {DEFAULT_NEWS_PER_CAT})")
    args = parser.parse_args()

    if not WIKI_PATH.exists():
        print(f"ERROR: missing {WIKI_PATH}\n  Run CP-0.1 first.", file=sys.stderr)
        return 1
    if not NEWS_PATH.exists():
        print(f"ERROR: missing {NEWS_PATH}\n  Run CP-0.1 first.", file=sys.stderr)
        return 1

    random.seed(args.seed)
    miu_list: list = []
    seen: set = set()

    print(f"[1/3] wiki: streaming up to {args.wiki_pages:,} pages...")
    for text in iter_wiki_pages(WIKI_PATH, args.wiki_pages):
        for miu in to_mius(text):
            if miu not in seen:
                seen.add(miu)
                miu_list.append(miu)
    print(f"  -> {len(miu_list):,} unique MIUs after wiki")

    print(f"[2/3] thucnews: {THUCNEWS_CATEGORIES}, up to {args.news_per_cat:,}/cat...")
    n0 = len(miu_list)
    for text in iter_thucnews(NEWS_PATH, THUCNEWS_CATEGORIES, args.news_per_cat):
        for miu in to_mius(text):
            if miu not in seen:
                seen.add(miu)
                miu_list.append(miu)
    print(f"  -> +{len(miu_list) - n0:,} new MIUs, total {len(miu_list):,}")

    if len(miu_list) > args.target:
        random.shuffle(miu_list)
        miu_list = miu_list[:args.target]
        # sort by length descending then lex for stable output
        miu_list.sort(key=lambda x: (-len(x), x))

    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    with out.open("w", encoding="utf-8") as f:
        for miu in miu_list:
            f.write(miu + "\n")

    print(f"[3/3] wrote {len(miu_list):,} MIUs -> {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
