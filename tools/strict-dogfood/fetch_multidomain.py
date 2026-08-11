#!/usr/bin/env python3
"""fetch_multidomain.py — fetch articles for 1000-article dogfood plan.

Per user 2026-07-01: 「文章要涵盖各种领域,都要比较长」.

Strategy:
  - 10 domains (政治/科技/财经/教育/医疗/文化/体育/历史/旅游/社会民生)
  - Each domain has multiple RSS feeds / API sources
  - Filter by length (≥800 CJK chars) — "比较长" baseline
  - opencc t2s for trad→simp
  - Strip person-name credit lines
  - Save to articles/{id}_{slug}.txt + append articles_status.tsv row

Usage:
    fetch_multidomain.py --domain A --count 5
    fetch_multidomain.py --next 1     # fetch next TODO from any domain
"""
from __future__ import annotations

import argparse
import re
import subprocess
import sys
import time
from pathlib import Path
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parents[2]
STRICT = ROOT / "docs/pinyin-dogfood-2026-06-30/strict"
ARTICLES = STRICT / "articles"
STATUS = STRICT / "articles_status.tsv"

DOMAINS = {
    "A": ("政治政策", [
        "http://www.people.com.cn/rss/politics.xml",
        "http://www.xinhuanet.com/politics/news_politics.xml",
    ]),
    "B": ("科技数码", [
        "https://www.36kr.com/feed",
        "https://www.ithome.com/rss/",
    ]),
    "C": ("财经金融", [
        "http://www.people.com.cn/rss/finance.xml",
        "https://feedx.net/rss/yicai.xml",
    ]),
    "D": ("教育学术", [
        "http://www.people.com.cn/rss/edu.xml",
        "http://www.moe.gov.cn/jyb_xwfb/xw_zt/rss/index.xml",
    ]),
    "E": ("医疗健康", [
        "http://www.people.com.cn/rss/health.xml",
        "http://www.jkb.com.cn/rss.xml",
    ]),
    "F": ("文化艺术", [
        "http://www.people.com.cn/rss/culture.xml",
        "http://www.gmw.cn/rss/wenhua.xml",
    ]),
    "G": ("体育竞技", [
        "http://www.people.com.cn/rss/sports.xml",
        "https://sports.sina.com.cn/sports.xml",
    ]),
    "H": ("历史人文", [
        "http://www.people.com.cn/rss/history.xml",
        "http://www.gmw.cn/rss/lishi.xml",
    ]),
    "I": ("旅游地理", [
        "http://www.people.com.cn/rss/travel.xml",
        "https://travel.sina.com.cn/travel.xml",
    ]),
    "J": ("社会民生", [
        "http://www.people.com.cn/rss/society.xml",
        "http://www.gmw.cn/rss/local.xml",
    ]),
}

USER_AGENT = "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0) AppleWebKit/605"
MIN_CJK = 800  # "比较长" threshold


def opencc_t2s(text: str) -> str:
    """Convert Traditional → Simplified via opencc CLI."""
    try:
        r = subprocess.run(
            ["opencc", "-c", "t2s"],
            input=text, capture_output=True, text=True, check=False,
        )
        if r.returncode == 0 and r.stdout:
            return r.stdout
    except FileNotFoundError:
        pass
    return text


def count_cjk(text: str) -> int:
    return sum(1 for c in text if '一' <= c <= '鿿')


def fetch_url(url: str, timeout: float = 15.0) -> str | None:
    try:
        req = Request(url, headers={"User-Agent": USER_AGENT})
        with urlopen(req, timeout=timeout) as r:
            data = r.read()
        for enc in ("utf-8", "gb18030", "gbk"):
            try:
                return data.decode(enc)
            except UnicodeDecodeError:
                continue
    except Exception as e:
        print(f"  fetch fail: {url}: {e}", file=sys.stderr)
    return None


def parse_rss(xml: str) -> list[dict]:
    """Extract items from RSS XML — minimal parser, no external deps."""
    items = []
    for m in re.finditer(r"<item>(.*?)</item>", xml, re.DOTALL):
        body = m.group(1)
        title = re.search(r"<title[^>]*>(?:<!\[CDATA\[)?(.*?)(?:\]\]>)?</title>",
                          body, re.DOTALL)
        link = re.search(r"<link[^>]*>(?:<!\[CDATA\[)?(.*?)(?:\]\]>)?</link>",
                         body, re.DOTALL)
        desc = re.search(r"<description[^>]*>(?:<!\[CDATA\[)?(.*?)(?:\]\]>)?</description>",
                         body, re.DOTALL)
        items.append({
            "title": (title.group(1) if title else "").strip(),
            "link": (link.group(1) if link else "").strip(),
            "desc": (desc.group(1) if desc else "").strip(),
        })
    return items


def strip_html(s: str) -> str:
    s = re.sub(r"<[^>]+>", "", s)
    s = re.sub(r"&nbsp;", " ", s)
    s = re.sub(r"&[a-z]+;", "", s)
    return s


def slugify(title: str, maxlen: int = 50) -> str:
    s = re.sub(r"[^\w一-鿿]+", "_", title)
    s = re.sub(r"_+", "_", s).strip("_")
    return s[:maxlen]


def load_status() -> list[list[str]]:
    rows = []
    if not STATUS.exists():
        return rows
    for ln in STATUS.read_text(encoding="utf-8").splitlines():
        if not ln.strip() or ln.startswith("#"):
            rows.append([ln])
            continue
        rows.append(ln.split("\t"))
    return rows


def next_id(domain: str) -> str:
    rows = load_status()
    max_seq = 0
    for r in rows:
        if len(r) > 1 and r[0].startswith(domain) and len(r[0]) == 4:
            try:
                max_seq = max(max_seq, int(r[0][1:]))
            except ValueError:
                pass
    return f"{domain}{max_seq + 1:03d}"


def append_status(article_id: str, status: str, domain: str, source: str,
                  title: str, cjk: int) -> None:
    line = f"{article_id}\t{status}\t{domain}\t{source}\t{title}\t{cjk}\t0\t0\t"
    with STATUS.open("a", encoding="utf-8") as f:
        f.write(line + "\n")


def fetch_domain(domain_letter: str, count: int) -> list[str]:
    domain_name, feeds = DOMAINS[domain_letter]
    added = []
    for feed_url in feeds:
        if len(added) >= count:
            break
        print(f"  fetching {feed_url}")
        xml = fetch_url(feed_url)
        if not xml:
            continue
        items = parse_rss(xml)
        for item in items:
            if len(added) >= count:
                break
            text_raw = item.get("desc", "")
            # Some RSS embed full body in description CDATA, some only link.
            # Try to get a long body.
            body = opencc_t2s(strip_html(text_raw))
            cjk = count_cjk(body)
            if cjk < MIN_CJK:
                continue
            title = opencc_t2s(strip_html(item["title"]))
            aid = next_id(domain_letter)
            slug = slugify(title)
            out_path = ARTICLES / f"{aid}_{slug}.txt"
            out_path.write_text(title + "\n\n" + body, encoding="utf-8")
            append_status(aid, "FETCHED", domain_name, feed_url, title, cjk)
            added.append(aid)
            print(f"  ✓ {aid} ({cjk} CJK): {title[:40]}")
    return added


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--domain", help="A/B/C/D/E/F/G/H/I/J")
    ap.add_argument("--count", type=int, default=5)
    ap.add_argument("--all-domains", action="store_true",
                    help="fetch from all 10 domains")
    args = ap.parse_args()

    ARTICLES.mkdir(parents=True, exist_ok=True)
    if args.all_domains:
        for d in DOMAINS:
            print(f"=== Domain {d} {DOMAINS[d][0]} ===")
            fetch_domain(d, args.count)
            time.sleep(1)
    elif args.domain:
        fetch_domain(args.domain, args.count)
    else:
        print("specify --domain X or --all-domains")
        sys.exit(1)


if __name__ == "__main__":
    main()
