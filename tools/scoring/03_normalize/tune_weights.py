#!/usr/bin/env python3
"""Algorithm+weight tuning probe for the CP3d-cutover hybrid normalization.

cutover surfaced a STRUCTURAL ceiling in per-source-log-count: its per-source
[0,1] normalization (ln(1+count)/max_ln[src]) discards cross-source magnitude,
so formal words that live in ONE source (中国: wiki 23257, lccc 0) lose prefix
competitiveness (中国 ranked 37th under zho). Raising news/wiki to rescue them
re-breaks colloquial single chars (把>吧). No single weight vector wins.

User picked: hybrid algorithm. This tool compares three normalizers under
candidate weights, scoring BOTH the colloquial single-syllable invariants AND a
formal-word floor (中国/国家 global percentile), so we pick the algorithm+weight
that satisfies both:

  log-count   Σ α·ln(1+c)/max_ln[src]   (current CP3b — colloquial-good, formal-weak)
  sum-then-log ln(1+Σ α·c)              (CP2 — formal-strong, large-corpus-dominated)
  log-sum     Σ α·ln(1+c)              (HYBRID — per-source log keeps magnitude
                                          without [0,1] flattening; not corpus-count
                                          dominated thanks to ln)

Run: python3 tools/scoring/03_normalize/tune_weights.py
"""
from __future__ import annotations

import math
import subprocess
from pathlib import Path

ROOT = Path(subprocess.run(["git", "rev-parse", "--show-toplevel"],
                           capture_output=True, text=True, check=True).stdout.strip())
EXTRACTED = ROOT / "tools/scoring/data/extracted"
READINGS = ROOT / "core/crates/inputx-pinyin/data/readings.tsv"
SRCS = ["zho_lccc_base", "zho_news_2020_100k", "zho_subtlex_ch_wf", "zho_wikipedia_2018_1m"]

INVARIANTS = [
    ("de", "的"), ("le", "了"), ("ma", "吗"), ("ba", "吧"), ("ne", "呢"),
    ("ya", "呀"), ("a", "啊"), ("wo", "我"), ("ni", "你"), ("ta", "他"),
    ("bu", "不"), ("yi", "一"), ("ge", "个"), ("zhe", "这"), ("na", "那"),
    ("you", "有"), ("zai", "在"), ("jiu", "就"), ("yao", "要"), ("dou", "都"),
    ("hen", "很"), ("mei", "没"), ("hui", "会"), ("neng", "能"),
    ("kan", "看"), ("ting", "听"), ("zuo", "做"), ("shuo", "说"), ("hao", "好"),
    ("xiang", "想"), ("jia", "家"), ("ren", "人"), ("tian", "天"), ("nian", "年"),
    ("chu", "出"), ("qu", "去"), ("lai", "来"), ("shang", "上"), ("xia", "下"),
    ("li", "里"), ("shou", "手"), ("dao", "到"), ("da", "大"), ("he", "和"),
    ("ke", "可"), ("an", "安"), ("ai", "爱"), ("ye", "也"), ("xi", "西"),
    # colloquial-truth single chars (baseline updated to these)
    ("qi", "起"), ("fa", "发"), ("tu", "图"), ("mu", "木"), ("la", "啦"), ("xin", "心"),
    # gate1 coverage
    ("kaopu", "靠谱"), ("geili", "给力"), ("zhagan", "榨干"), ("wanghong", "网红"),
    # formal / common multi-char (must stay)
    ("zhongguo", "中国"), ("women", "我们"), ("shijian", "时间"), ("zhongyao", "重要"),
    ("beijing", "北京"), ("xiexie", "谢谢"), ("pengyou", "朋友"), ("gongzuo", "工作"),
]
# formal words whose GLOBAL percentile we track (prefix-competitiveness proxy)
FLOOR_WORDS = ["中国", "国家", "北京", "重要"]


def load_counts():
    out = {}
    for src in SRCS:
        c = {}
        p = EXTRACTED / src / "freq.tsv"
        if p.exists():
            for line in p.open(encoding="utf-8"):
                if line.startswith("#") or "\t" not in line:
                    continue
                w, n = line.rstrip("\n").split("\t")[:2]
                try:
                    c[w] = c.get(w, 0) + int(n)
                except ValueError:
                    pass
        out[src] = c
    return out


def load_pinyin_words():
    m = {}
    for raw in READINGS.open(encoding="utf-8"):
        line = raw.rstrip("\n")
        if not line or line.startswith("#"):
            continue
        parts = line.split("\t")
        for py in parts[1:]:
            py = py.strip()
            if py:
                m.setdefault(py, []).append(parts[0])
    return m


def score_log_count(counts, weights):
    s = {}
    for src, a in weights.items():
        c = counts[src]
        if not c:
            continue
        mx = max(math.log(1.0 + v) for v in c.values())
        if mx <= 0:
            continue
        for w, v in c.items():
            s[w] = s.get(w, 0.0) + a * math.log(1.0 + v) / mx
    return s


def score_sum_then_log(counts, weights):
    raw = {}
    for src, a in weights.items():
        for w, v in counts[src].items():
            raw[w] = raw.get(w, 0.0) + v * a
    return {w: math.log(1.0 + r) for w, r in raw.items()}


def score_log_sum(counts, weights):
    s = {}
    for src, a in weights.items():
        for w, v in counts[src].items():
            s[w] = s.get(w, 0.0) + a * math.log(1.0 + v)
    return s


def score_hybrid(counts, weights, beta):
    """β·norm(sum-then-log) + (1-β)·norm(log-count): blend formal-magnitude
    (sum-then-log) with colloquial-balance (log-count), each min-maxed to [0,1]."""
    s1 = score_sum_then_log(counts, weights)
    s2 = score_log_count(counts, weights)
    m1 = max(s1.values()) or 1.0
    m2 = max(s2.values()) or 1.0
    keys = set(s1) | set(s2)
    return {w: beta * s1.get(w, 0.0) / m1 + (1 - beta) * s2.get(w, 0.0) / m2 for w in keys}


ALGOS = {
    "log-count": score_log_count,
    "sum-then-log": score_sum_then_log,
    "log-sum": score_log_sum,
    "hybrid0.7": lambda c, w: score_hybrid(c, w, 0.7),
    "hybrid0.5": lambda c, w: score_hybrid(c, w, 0.5),
}


def evaluate(score, py_words):
    passed, fails = 0, []
    for py, exp in INVARIANTS:
        words = py_words.get(py, [])
        if not words:
            fails.append(f"{py}!nordg")
            continue
        top = max(words, key=lambda w: score.get(w, 0.0))
        if top == exp:
            passed += 1
        else:
            fails.append(f"{py}:{top}≠{exp}")
    # floor percentiles
    vals = sorted(score.values())
    n = len(vals)
    import bisect
    floors = {}
    for w in FLOOR_WORDS:
        sv = score.get(w, 0.0)
        pct = 100.0 * bisect.bisect_left(vals, sv) / n if n else 0
        floors[w] = pct
    return passed, fails, floors


def main():
    counts = load_counts()
    py_words = load_pinyin_words()
    # (label, algo, weights lccc/news/subtlex/wiki)
    W = lambda l, n, s, w: {"zho_lccc_base": l, "zho_news_2020_100k": n, "zho_subtlex_ch_wf": s, "zho_wikipedia_2018_1m": w}
    cases = [
        ("sum-log    3/3/3/1", "sum-then-log", W(3, 3, 3, 1)),
        ("sum-log    4/2/4/1", "sum-then-log", W(4, 2, 4, 1)),
        ("sum-log    4/3/4/2", "sum-then-log", W(4, 3, 4, 2)),
        ("hybrid0.7  3/3/3/1", "hybrid0.7", W(3, 3, 3, 1)),
        ("hybrid0.7  4/2/4/1", "hybrid0.7", W(4, 2, 4, 1)),
        ("hybrid0.5  3/3/3/1", "hybrid0.5", W(3, 3, 3, 1)),
        ("hybrid0.5  4/2/4/1", "hybrid0.5", W(4, 2, 4, 1)),
    ]
    # zho-prefix distinct words (real prefix-competition proxy for 中国 top30)
    zho_words = {w for py, ws in py_words.items() if py.startswith("zho") for w in ws}
    print(f"invariants: {len(INVARIANTS)}  (中国rk = 中国's rank among zho* words; <=30 passes prefix test)\n")
    for label, algo, w in cases:
        score = ALGOS[algo](counts, w)
        p, fails, floors = evaluate(score, py_words)
        zg = score.get("中国", 0.0)
        zg_rank = 1 + sum(1 for x in zho_words if score.get(x, 0.0) > zg)
        print(f"{label:20s} {p}/{len(INVARIANTS)}  中国rk={zg_rank:<3d} [中国{floors['中国']:.1f}%]")
        if fails:
            print(f"{'':20s}   fails: {' '.join(fails)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
