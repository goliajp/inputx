#!/usr/bin/env python3
"""fetch_bbc_news.py — pull recent BBC Chinese news (simplified) into corpus_news/.

Reads RSS, GETs each article URL with /simp suffix, extracts paragraph
text, saves to corpus_news/articles/NNNN_<title-slug>.txt.

Usage: fetch_bbc_news.py [target_count]
"""

import json
import re
import sys
import urllib.parse
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CORPUS = ROOT / "docs/pinyin-dogfood-2026-06-30/scratchpad/corpus_news/articles"
CORPUS.mkdir(parents=True, exist_ok=True)

UA = "Mozilla/5.0 (Macintosh; Intel Mac OS X) AppleWebKit Inputx/0.1"
RSS_URL = "https://feeds.bbci.co.uk/zhongwen/simp/rss.xml"
TARGET = int(sys.argv[1]) if len(sys.argv) > 1 else 10


def fetch(url: str) -> str:
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=15) as r:
        return r.read().decode("utf-8", errors="ignore")


def parse_rss(xml: str) -> list[tuple[str, str]]:
    """Extract (title, link) tuples from RSS feed."""
    items = re.findall(r'<item>(.*?)</item>', xml, re.DOTALL)
    out = []
    for it in items:
        t = re.search(r'<title><!\[CDATA\[(.*?)\]\]></title>', it, re.DOTALL)
        l = re.search(r'<link>(.*?)</link>', it, re.DOTALL)
        if t and l:
            title = t.group(1).strip()
            link = l.group(1).strip()
            # Force /simp variant
            link = re.sub(r'/trad(\?|#|$)', r'/simp\1', link)
            link = link.split('?')[0]  # drop query
            out.append((title, link))
    return out


def extract_article(html: str) -> str:
    """Pull paragraph text out of BBC article HTML."""
    # BBC paragraphs use varied css class hashes; just grab all <p> tags
    paras = re.findall(r'<p[^>]*>(.*?)</p>', html, re.DOTALL)
    cleaned = []
    skip_phrases = ['end of', '热读', '推荐阅读', '相关图集', 'BBC News', 'Copyright', '版权所有']
    for p in paras:
        # Strip nested HTML tags
        text = re.sub(r'<[^>]+>', '', p)
        text = text.replace('&amp;', '&').replace('&lt;', '<').replace('&gt;', '>')
        text = text.replace('&nbsp;', ' ').replace('&quot;', '"').strip()
        if not text or len(text) < 6:
            continue
        if any(p in text for p in skip_phrases):
            continue
        cleaned.append(text)
    return "\n\n".join(cleaned)


def safe_slug(title: str) -> str:
    out = "".join(c if c.isalnum() else "_" for c in title)[:30]
    return out


def main():
    print(f"fetching RSS {RSS_URL}")
    rss = fetch(RSS_URL)
    items = parse_rss(rss)
    print(f"  found {len(items)} items")

    saved = 0
    next_id = 1
    # Skip existing IDs to avoid collisions
    existing = {int(f.name.split("_")[0]) for f in CORPUS.glob("*.txt") if f.name[:4].isdigit()}
    while next_id in existing:
        next_id += 1

    for title, link in items:
        if saved >= TARGET:
            break
        try:
            print(f"  fetching {next_id}: {title[:40]}…", end="")
            html = fetch(link)
            body = extract_article(html)
            cjk = sum(1 for c in body if '一' <= c <= '鿿')
            if cjk < 400:
                print(f"  SKIP ({cjk} CJK chars too short)")
                continue
            fname = f"{next_id:04d}_{safe_slug(title)}.txt"
            (CORPUS / fname).write_text(f"{title}\n\n{body}\n", encoding="utf-8")
            print(f"  OK ({cjk} CJK chars) → {fname}")
            saved += 1
            next_id += 1
            while next_id in existing:
                next_id += 1
        except Exception as e:
            print(f"  FAIL: {e}")
            continue

    print(f"\ndone. {saved} articles saved to {CORPUS}")


if __name__ == "__main__":
    main()
