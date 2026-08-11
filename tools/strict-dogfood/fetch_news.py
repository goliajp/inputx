#!/usr/bin/env python3
"""fetch_news.py — pull recent Chinese news from RSS feeds into corpus_news/.

Sources (multi-feed):
  - 人民网 RSS 时政 (politics)
  - 人民网 RSS 国际 (international) — broader topics
  - 人民网 RSS 社会 (society)

Body is in <description> CDATA (full text, not snippet).
opencc t2s applied as safety net.

Usage: fetch_news.py [target_count]
"""

import re
import sys
import urllib.request
from pathlib import Path
import opencc  # type: ignore

ROOT = Path(__file__).resolve().parents[2]
CORPUS = ROOT / "docs/pinyin-dogfood-2026-06-30/scratchpad/corpus_news/articles"
CORPUS.mkdir(parents=True, exist_ok=True)

UA = "Mozilla/5.0 (Macintosh; Intel Mac OS X) AppleWebKit Inputx/0.1"
TARGET = int(sys.argv[1]) if len(sys.argv) > 1 else 10
CC = opencc.OpenCC('t2s')

FEEDS = [
    "http://www.people.com.cn/rss/politics.xml",
    "http://www.people.com.cn/rss/world.xml",
    "http://www.people.com.cn/rss/society.xml",
]


def fetch(url: str) -> str:
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=15) as r:
        return r.read().decode("utf-8", errors="ignore")


def parse_rss(xml: str) -> list[tuple[str, str]]:
    """Extract (title, body_html) — body is in <description> CDATA."""
    items = re.findall(r'<item>(.*?)</item>', xml, re.DOTALL)
    out = []
    for it in items:
        t = re.search(r'<title><!\[CDATA\[(.*?)\]\]></title>', it, re.DOTALL)
        d = re.search(r'<description><!\[CDATA\[(.*?)\]\]></description>', it, re.DOTALL)
        if t and d:
            out.append((t.group(1).strip(), d.group(1)))
    return out


def extract_text(html: str) -> str:
    """Pull paragraph text from <p> tags, drop img/script/style."""
    # Strip img tags
    html = re.sub(r'<img[^>]*>', '', html)
    # Pull <p> contents
    paras = re.findall(r'<p[^>]*>(.*?)</p>', html, re.DOTALL)
    out = []
    skip_phrases = ['策划', '统筹', '主编', '主笔', '视觉', '编辑', '新华社', '制作', '监制', '出品']
    for p in paras:
        text = re.sub(r'<[^>]+>', '', p)
        text = (text.replace('&amp;', '&').replace('&lt;', '<').replace('&gt;', '>')
                    .replace('&nbsp;', ' ').replace('&quot;', '"')
                    .replace('　', '').strip())
        if not text or len(text) < 8:
            continue
        # Skip credits (策划/统筹/etc)
        if any(p in text[:8] for p in skip_phrases):
            continue
        out.append(text)
    return "\n\n".join(out)


def safe_slug(title: str) -> str:
    return "".join(c if c.isalnum() else "_" for c in title)[:30]


def main():
    existing = {int(f.name.split("_")[0]) for f in CORPUS.glob("*.txt") if f.name[:4].isdigit()}
    # Clear existing to start fresh with simp-only source
    print("clearing existing corpus_news (switching to simp-only source)")
    for f in CORPUS.glob("*.txt"):
        f.unlink()
    existing = set()
    next_id = 1
    saved = 0

    all_items = []
    for feed in FEEDS:
        try:
            print(f"fetching {feed}")
            xml = fetch(feed)
            items = parse_rss(xml)
            print(f"  {len(items)} items")
            all_items.extend(items)
        except Exception as e:
            print(f"  FAIL: {e}")

    # Dedup by title
    seen = set()
    deduped = []
    for t, d in all_items:
        if t in seen:
            continue
        seen.add(t)
        deduped.append((t, d))

    print(f"\n{len(deduped)} unique items, target {TARGET}")
    for title, body_html in deduped:
        if saved >= TARGET:
            break
        body = extract_text(body_html)
        body_simp = CC.convert(body)
        title_simp = CC.convert(title)
        cjk = sum(1 for c in body_simp if '一' <= c <= '鿿')
        if cjk < 400:
            print(f"  SKIP ({cjk} CJK chars) {title_simp[:40]}")
            continue
        fname = f"{next_id:04d}_{safe_slug(title_simp)}.txt"
        (CORPUS / fname).write_text(f"{title_simp}\n\n{body_simp}\n", encoding="utf-8")
        print(f"  OK ({cjk} chars) → {fname}")
        saved += 1
        next_id += 1

    print(f"\ndone. {saved}/{TARGET} articles saved")


if __name__ == "__main__":
    main()
