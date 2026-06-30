#!/usr/bin/env python3
"""render_dashboard.py — read strict/data/*.json + strict/logs/polish_log.tsv
+ strict/articles_status.tsv → emit a self-contained HTML dashboard.

Usage:
    render_dashboard.py [--out path]
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
STRICT = ROOT / "docs/pinyin-dogfood-2026-06-30/strict"
DATA_DIR = STRICT / "data"
POLISH_LOG = STRICT / "logs/polish_log.tsv"
STATUS = STRICT / "articles_status.tsv"


def load_articles() -> list[dict]:
    out = []
    for f in sorted(DATA_DIR.glob("*.json")):
        out.append(json.loads(f.read_text(encoding="utf-8")))
    return out


def load_polish_log() -> list[dict]:
    out = []
    if not POLISH_LOG.exists():
        return out
    for ln in POLISH_LOG.read_text(encoding="utf-8").splitlines():
        if not ln.strip() or ln.startswith("#"):
            continue
        cols = ln.split("\t")
        if len(cols) < 8:
            continue
        out.append({
            "date": cols[0],
            "article": cols[1],
            "seg": cols[2],
            "buffer": cols[3],
            "expected": cols[4],
            "got": cols[5],
            "action": cols[6],
            "file": cols[7],
            "notes": cols[8] if len(cols) > 8 else "",
        })
    return out


def load_status() -> list[dict]:
    out = []
    if not STATUS.exists():
        return out
    for ln in STATUS.read_text(encoding="utf-8").splitlines():
        if not ln.strip() or ln.startswith("#"):
            continue
        cols = ln.split("\t")
        if len(cols) < 5:
            continue
        out.append({
            "id": cols[0],
            "status": cols[1],
            "done": cols[2],
            "total": cols[3],
            "last_seg": cols[4],
            "note": cols[5] if len(cols) > 5 else "",
        })
    return out


HTML_TEMPLATE = r"""<!doctype html>
<html lang="zh-CN"><head>
<meta charset="utf-8">
<title>Inputx pinyin v2 — dogfood polish strict mode</title>
<style>
body { font: 13px/1.4 -apple-system, "PingFang SC", sans-serif; margin: 0; background: #0d1117; color: #c9d1d9; }
header { padding: 16px 24px; background: #161b22; border-bottom: 1px solid #30363d; position: sticky; top: 0; z-index: 100; }
header h1 { margin: 0 0 6px; font-size: 18px; color: #58a6ff; }
header .sub { color: #8b949e; font-size: 12px; }
.layout { display: grid; grid-template-columns: 280px 1fr; min-height: 100vh; }
.sidebar { padding: 16px; background: #0d1117; border-right: 1px solid #30363d; overflow-y: auto; max-height: calc(100vh - 60px); position: sticky; top: 60px; }
.sidebar h2 { font-size: 13px; color: #8b949e; text-transform: uppercase; letter-spacing: 0.5px; margin: 12px 0 6px; }
.article-link { display: block; padding: 6px 10px; color: #c9d1d9; text-decoration: none; border-radius: 4px; margin-bottom: 2px; font-size: 12px; }
.article-link:hover { background: #161b22; }
.article-link.done { color: #56d364; }
.article-link.wip { color: #d29922; }
.article-link .id { color: #8b949e; font-family: monospace; margin-right: 6px; }
.main { padding: 24px; max-width: 1400px; }
.kpi-row { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 12px; margin-bottom: 24px; }
.kpi { background: #161b22; border: 1px solid #30363d; border-radius: 6px; padding: 14px; }
.kpi .label { font-size: 11px; color: #8b949e; text-transform: uppercase; letter-spacing: 0.5px; }
.kpi .value { font-size: 26px; font-weight: 600; margin-top: 4px; color: #58a6ff; }
.kpi .sub { font-size: 11px; color: #8b949e; margin-top: 2px; }
.article { background: #161b22; border: 1px solid #30363d; border-radius: 6px; padding: 18px; margin-bottom: 18px; }
.article h2 { margin: 0 0 8px; font-size: 16px; color: #58a6ff; }
.article .meta { font-size: 12px; color: #8b949e; margin-bottom: 12px; }
.article details { margin-top: 12px; }
.article summary { cursor: pointer; padding: 4px 0; color: #8b949e; font-size: 12px; }
.article .raw { background: #0d1117; border-radius: 4px; padding: 10px; max-height: 160px; overflow-y: auto; font-size: 11px; color: #8b949e; white-space: pre-wrap; }
table { width: 100%; border-collapse: collapse; font-size: 12px; }
th, td { padding: 6px 8px; text-align: left; border-bottom: 1px solid #30363d; vertical-align: top; }
th { color: #8b949e; font-weight: 500; background: #0d1117; position: sticky; top: 0; }
.verdict { display: inline-block; padding: 2px 8px; border-radius: 10px; font-size: 10px; font-weight: 600; }
.v-PASS { background: rgba(86, 211, 100, 0.15); color: #56d364; }
.v-SOFT { background: rgba(210, 153, 34, 0.15); color: #d29922; }
.v-HARD { background: rgba(248, 81, 73, 0.15); color: #f85149; }
.top10 { font-family: -apple-system, "PingFang SC", sans-serif; font-size: 11px; color: #8b949e; }
.top10 .winner { color: #58a6ff; font-weight: 600; }
.top10 .target { color: #56d364; font-weight: 600; }
.top10 .target-soft { color: #d29922; font-weight: 600; }
.pinyin { font-family: SF Mono, monospace; color: #79c0ff; font-size: 11px; }
.code { font-family: SF Mono, monospace; color: #a5d6ff; font-size: 11px; }
.expected { font-weight: 600; color: #c9d1d9; }
.polish-log { margin-top: 24px; }
.polish-log table { font-size: 11px; }
.polish-log .action { color: #f0883e; font-weight: 600; }
.bar { display: inline-block; height: 4px; background: #30363d; border-radius: 2px; width: 60px; vertical-align: middle; margin-left: 6px; }
.bar > div { height: 100%; background: #56d364; border-radius: 2px; }
</style>
</head>
<body>
<header>
  <h1>Inputx pinyin v2 — dogfood polish (strict)</h1>
  <div class="sub">每篇文章 100% PASS 才算 DONE · per-segment LLM 判 candidate · realtime polish</div>
</header>

<div class="layout">
  <aside class="sidebar">
    <h2>Articles processed</h2>
    <div id="article-nav">__NAV__</div>
    <h2 style="margin-top: 24px;">Polish summary</h2>
    <div style="font-size: 12px; color: #8b949e;">__POLISH_SUMMARY__</div>
  </aside>

  <main class="main">
    <div class="kpi-row">
      __KPI__
    </div>

    <div id="articles">__ARTICLES__</div>

    <section class="polish-log">
      <h2 style="color: #58a6ff; font-size: 14px;">📝 Polish action log (append-only)</h2>
      <table>
        <thead><tr><th>Date</th><th>Article</th><th>Seg</th><th>Buffer</th><th>Expected</th><th>Got</th><th>Action</th><th>File</th><th>Notes</th></tr></thead>
        <tbody>__POLISH_TABLE__</tbody>
      </table>
    </section>
  </main>
</div>

</body></html>
"""


def render_top10(top10: list[str], expected: str, verdict: str) -> str:
    if not top10:
        return '<span class="top10" style="font-style: italic;">(empty)</span>'
    parts = []
    for i, w in enumerate(top10):
        cls = ""
        if i == 0:
            cls = "winner"
            if w == expected and verdict == "PASS":
                cls = "target"
        elif w == expected:
            cls = "target-soft"
        if cls:
            parts.append(f'<span class="{cls}">{w}</span>')
        else:
            parts.append(w)
    return '<span class="top10">' + ', '.join(parts) + '</span>'


def render_article(art: dict) -> str:
    aid = art["id"]
    title = art["title"]
    stats = art["stats"]
    pass_pct = 100 * stats["pass"] // max(1, stats["total"])
    pass_color = "#56d364" if pass_pct == 100 else ("#d29922" if pass_pct >= 80 else "#f85149")

    rows = []
    for s in art["segments"]:
        rows.append(
            f'<tr><td>{s["idx"]}</td>'
            f'<td><span class="expected">{s["expected"]}</span></td>'
            f'<td><span class="pinyin">{s["pinyin"]}</span></td>'
            f'<td><span class="verdict v-{s["verdict"]}">{s["verdict"]}</span></td>'
            f'<td>{render_top10(s["top10"], s["expected"], s["verdict"])}</td>'
            f'<td><span style="font-size:11px;color:#8b949e;">{s["reason"]}</span></td>'
            f'</tr>'
        )

    raw_preview = (art.get("raw_text", "") or "")[:1200]
    return f"""
<section class="article" id="art-{aid}">
  <h2>📄 {aid} — {title}</h2>
  <div class="meta">
    {art["cjk_char_count"]} CJK chars ·
    <strong style="color: {pass_color};">{stats["pass"]}/{stats["total"]} PASS ({pass_pct}%)</strong> ·
    SOFT {stats["soft"]} · HARD {stats["hard"]}
  </div>
  <details><summary>📖 Show raw article text (preview)</summary>
    <div class="raw">{raw_preview}</div>
  </details>
  <table style="margin-top: 12px;">
    <thead><tr><th>#</th><th>Expected</th><th>Pinyin</th><th>Verdict</th><th>Top-10 (current)</th><th>Reason</th></tr></thead>
    <tbody>{''.join(rows)}</tbody>
  </table>
</section>
"""


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=str(STRICT / "dashboard.html"))
    args = ap.parse_args()

    articles = load_articles()
    polish = load_polish_log()
    status = load_status()

    # KPIs
    total_segs = sum(a["stats"]["total"] for a in articles)
    pass_segs = sum(a["stats"]["pass"] for a in articles)
    soft_segs = sum(a["stats"]["soft"] for a in articles)
    hard_segs = sum(a["stats"]["hard"] for a in articles)
    pass_pct = 100 * pass_segs / max(1, total_segs)

    kpi_html = ""
    for label, val, sub in [
        ("Articles done", f"{len(status)}/1000", f"{sum(1 for s in status if s['status']=='DONE')} DONE"),
        ("Segments processed", str(total_segs), f"across {len(articles)} structured articles"),
        ("PASS rate", f"{pass_pct:.1f}%", f"{pass_segs}/{total_segs}"),
        ("Polish actions", str(len(polish)), "cumulative"),
    ]:
        kpi_html += f'<div class="kpi"><div class="label">{label}</div><div class="value">{val}</div><div class="sub">{sub}</div></div>'

    # Sidebar nav
    nav_html = ""
    for a in articles:
        st = a["stats"]
        cls = "done" if st["pass"] == st["total"] else "wip"
        nav_html += f'<a class="article-link {cls}" href="#art-{a["id"]}"><span class="id">{a["id"]}</span>{a["title"]} <span style="color:#8b949e">({st["pass"]}/{st["total"]})</span></a>'

    # Polish summary
    polish_summary = f"{len(polish)} polish actions logged across {len({p['article'] for p in polish})} articles + seeds."

    # Articles
    articles_html = ""
    for a in articles:
        articles_html += render_article(a)

    # Polish table
    polish_rows = []
    for p in polish:
        polish_rows.append(
            f'<tr><td>{p["date"]}</td><td>{p["article"]}</td><td>{p["seg"]}</td>'
            f'<td class="code">{p["buffer"]}</td>'
            f'<td><span class="expected">{p["expected"]}</span></td>'
            f'<td>{p["got"]}</td>'
            f'<td class="action">{p["action"]}</td>'
            f'<td><span class="code">{p["file"]}</span></td>'
            f'<td><span style="color:#8b949e">{p["notes"]}</span></td></tr>'
        )

    html = (HTML_TEMPLATE
        .replace("__NAV__", nav_html)
        .replace("__POLISH_SUMMARY__", polish_summary)
        .replace("__KPI__", kpi_html)
        .replace("__ARTICLES__", articles_html)
        .replace("__POLISH_TABLE__", "".join(polish_rows))
    )

    Path(args.out).write_text(html, encoding="utf-8")
    print(f"wrote {args.out} ({len(articles)} articles, {len(polish)} polishes)")


if __name__ == "__main__":
    main()
