#!/usr/bin/env python3
"""New-word discovery from corpus full text (dict-pipeline CP3c).

Mines multi-char Chinese strings that ARE words (high internal cohesion +
free boundaries) yet are NOT in readings.tsv — i.e. the jieba dict's coverage
gaps (给力 / 吐槽 / 靠前 / 榨干 / 靠谱 ...). The output feeds
compose_phrase_readings (which assigns pinyin via Unihan per-char readings)
→ merge_readings → readings.tsv, after which build_weights gives each
discovered word a corpus-derived freq_score. Discovery only decides WHICH
strings are words; ranking is handled downstream.

Algorithm — classic cohesion + boundary-entropy (Matrix67 / HanLP family),
with two precision filters added after the first full run showed ~92k
candidates dominated by (a) cross-word collocations 去看/想吃/有一 and
(b) traditional-Chinese fragments from un-simplified wiki:
  1. Pass 1: count 1-gram + 2-gram freqs over corpus full text (within
     maximal CJK runs; non-CJK chars are hard boundaries). Cached to disk —
     it is the slow part (~15 min full-corpus); re-tuning thresholds reuses it.
  2. Cohesion: PMI = log2( P(ab) / (P(a)·P(b)) ). High ⇒ chars stick together.
  3. STOPWORD filter: drop a 2-gram whose first/last char is a high-freq
     function word / verb / quantifier — these produce structural pairs
     (去看 = 去+看) that PMI + entropy cannot reject. Curated to EXCLUDE
     chars that begin real words (给→给力 etc.).
  4. t2s filter: drop a candidate that changes under traditional→simplified
     conversion (i.e. contains a traditional/variant char) — kills wiki's
     繁体 fragments.
  5. Pass 2 (survivors only): left/right neighbor-char entropy; freedom =
     min(H_left, H_right). Drops fixed fragments of longer words.
  6. Drop candidates already in readings.tsv. Emit the rest.

Modes:
  --diagnose W...        print freq/pmi/H stats for given words (calibration)
  --list-candidates      pass1-only: print freq+pmi+stopword+t2s survivors
                         (seconds with --reuse-cache; for tuning steps 3-4)
  (default)              full run → data/supplemental/phrases_discovered.tsv

Examples:
  python3 tools/scoring/discover_new_words.py --sources lccc,news,wiki --diagnose 给力 点赞
  python3 tools/scoring/discover_new_words.py --sources lccc,news,wiki --list-candidates --reuse-cache
  python3 tools/scoring/discover_new_words.py --sources lccc,news,wiki --reuse-cache
"""
from __future__ import annotations
import argparse
import gzip
import json
import math
import pickle
import sys
import tarfile
from collections import Counter, defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
CACHE = ROOT / "core/crates/inputx-pinyin/data/corpus/cache"
READINGS = ROOT / "core/crates/inputx-pinyin/data/readings.tsv"
SUPP = ROOT / "tools/scoring/data/supplemental"
OUT = SUPP / "phrases_discovered.tsv"

SOURCES = {
    "lccc": ("zho_lccc_base", "jsonl_dialog"),
    "news": ("zho_news_2020_100k", "tar_gz"),
    "wiki": ("zho_wikipedia_2018_1m", "tar_gz"),
}

# Single-char function words / very-high-freq verbs / quantifiers. A 2-gram
# whose first OR last char is one of these is rejected: PMI + boundary-entropy
# systematically pass structural high-freq pairs (我也 / 的人 / 去看 / 想吃 /
# 个月) that are two words glued, not one word. Curated to EXCLUDE chars that
# frequently begin real words (给→给力, 向→向往, 点→点赞), so genuine
# colloquial gaps survive. Tune via --list-candidates.
STOPWORDS = set(
    "的了着过吗呢吧啊么呀哦嗯"      # particles / interjections
    "我你他她它们咱"                # pronouns
    "这那是在有"                    # demonstratives / copula / existential
    "也都就还又很太挺真才更最"      # adverbs
    "把被让对和与而但却则因所之其此该"  # prepositions / conjunctions
    "不没别"                        # negation
    "要去会想能来到说看做得用觉"    # ultra-high-freq verbs (cross-word glue)
    "个多些一"                      # quantifiers / numeral 一 (每一/另一/句一)
    "好完啦嘛"                      # 好X adjectives / X完 resultatives / 啦嘛 particles
    "吃里睡再"                      # ultra-high-freq verb/locative glue (吃点/子里/快睡/后再)
)


def is_cjk(c: str) -> bool:
    return "一" <= c <= "鿿"


def iter_segments(source_names: list[str]):
    """Yield raw text segments (one utterance / sentence) — never concatenated
    across utterances, so n-grams never span a sentence boundary."""
    for name in source_names:
        cid, fmt = SOURCES[name]
        path = CACHE / cid
        if not path.exists():
            print(f"warning: {path} missing — skipping {name}", file=sys.stderr)
            continue
        print(f"[discover] scanning {name} ({path.stat().st_size//1024//1024} MB, {fmt})",
              file=sys.stderr)
        if fmt == "jsonl_dialog":
            with gzip.open(path, "rt", encoding="utf-8", errors="replace") as f:
                for line in f:
                    line = line.strip()
                    if not line:
                        continue
                    try:
                        dlg = json.loads(line)
                    except Exception:
                        continue
                    if isinstance(dlg, dict):
                        dlg = dlg.get("dialog", [])
                    if isinstance(dlg, list):
                        for utt in dlg:
                            if isinstance(utt, str):
                                yield utt.replace(" ", "")
        elif fmt == "tar_gz":
            with tarfile.open(path, "r:gz") as tar:
                for m in tar.getmembers():
                    if not (m.isfile() and m.name.endswith("-sentences.txt")):
                        continue
                    fobj = tar.extractfile(m)
                    if fobj is None:
                        continue
                    for raw in fobj:
                        s = raw.decode("utf-8", "replace").rstrip("\r\n")
                        parts = s.split("\t", 1)
                        yield parts[1] if len(parts) == 2 else s


def cjk_runs(seg: str):
    run: list[str] = []
    for c in seg:
        if is_cjk(c):
            run.append(c)
        elif run:
            yield run
            run = []
    if run:
        yield run


def entropy(counter: Counter) -> float:
    total = sum(counter.values())
    if total == 0:
        return 0.0
    h = 0.0
    for n in counter.values():
        p = n / total
        h -= p * math.log2(p)
    return h


def run_pass1(source_names: list[str]):
    char_freq: Counter[str] = Counter()
    bigram_freq: Counter[str] = Counter()
    total_chars = 0
    seg_count = 0
    for seg in iter_segments(source_names):
        for run in cjk_runs(seg):
            n = len(run)
            total_chars += n
            for i in range(n):
                char_freq[run[i]] += 1
                if i + 1 < n:
                    bigram_freq[run[i] + run[i + 1]] += 1
        seg_count += 1
        if seg_count % 4_000_000 == 0:
            print(f"  ... pass1 {seg_count:,} segs, {len(bigram_freq):,} uniq bigrams",
                  file=sys.stderr)
    print(f"[discover] pass1 done: {total_chars:,} chars, {len(char_freq):,} uniq chars, "
          f"{len(bigram_freq):,} uniq bigrams", file=sys.stderr)
    return char_freq, bigram_freq, total_chars


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--sources", default="lccc,news,wiki")
    ap.add_argument("--min-freq", type=int, default=10)
    ap.add_argument("--min-pmi", type=float, default=1.0)   # candidate floor (loose)
    ap.add_argument("--min-entropy", type=float, default=1.5)  # low-freq entropy floor
    # Tiered final gate (calibrated 2026-05-25 from the diagnose comparison set).
    ap.add_argument("--lowfreq-cut", type=int, default=50,
                    help="freq below this is gated on PMI (name fragments cluster <2)")
    ap.add_argument("--lowfreq-pmi", type=float, default=2.2)
    ap.add_argument("--hifreq-pmi", type=float, default=2.0)
    ap.add_argument("--hifreq-entropy", type=float, default=3.0,
                    help="high-freq cross-word collocations have low boundary entropy")
    ap.add_argument("--lowfreq-char-floor", type=int, default=2000,
                    help="low-freq word must have both chars >= this char_freq "
                         "(rare-char fragments like 三灶/丝蕴 sit below; 榨=2480 survives)")
    ap.add_argument("--diagnose", nargs="*", default=None)
    ap.add_argument("--list-candidates", action="store_true",
                    help="pass1-only: print freq+pmi+stopword+t2s survivors, skip pass2")
    ap.add_argument("--reuse-cache", action="store_true",
                    help="load pass1 from disk cache if present (skip re-scan)")
    args = ap.parse_args()
    source_names = [s.strip() for s in args.sources.split(",") if s.strip()]

    registered: set[str] = set()
    with READINGS.open() as f:
        for line in f:
            if line.startswith("#") or not line.strip():
                continue
            registered.add(line.split("\t", 1)[0])
    print(f"[discover] {len(registered):,} registered words in readings.tsv", file=sys.stderr)

    # ---- Pass 1 (cached) ----
    cache_path = SUPP / f".discover_cache_{'-'.join(sorted(source_names))}.pkl"
    if args.reuse_cache and cache_path.exists():
        print(f"[discover] loading pass1 cache {cache_path}", file=sys.stderr)
        with cache_path.open("rb") as f:
            char_freq, bigram_freq, total_chars = pickle.load(f)
        print(f"[discover] cache: {total_chars:,} chars, {len(bigram_freq):,} uniq bigrams",
              file=sys.stderr)
    else:
        char_freq, bigram_freq, total_chars = run_pass1(source_names)
        SUPP.mkdir(parents=True, exist_ok=True)
        with cache_path.open("wb") as f:
            pickle.dump((char_freq, bigram_freq, total_chars), f, protocol=pickle.HIGHEST_PROTOCOL)
        print(f"[discover] wrote pass1 cache {cache_path}", file=sys.stderr)

    def pmi(ab: str) -> float:
        fab, fa, fb = bigram_freq[ab], char_freq[ab[0]], char_freq[ab[1]]
        if fab == 0 or fa == 0 or fb == 0:
            return float("-inf")
        return math.log2(fab * total_chars / (fa * fb))

    # lazy t2s — only built if needed
    _t2s = None
    def has_traditional(w: str) -> bool:
        nonlocal _t2s
        if _t2s is None:
            from opencc import OpenCC
            _t2s = OpenCC("t2s")
        return _t2s.convert(w) != w

    # ---- Candidate set: freq + PMI + stopword + t2s ----
    candidates: set[str] = set()
    for ab, c in bigram_freq.items():
        if c < args.min_freq:
            continue
        if ab[0] in STOPWORDS or ab[1] in STOPWORDS:
            continue
        if pmi(ab) < args.min_pmi:
            continue
        if has_traditional(ab):
            continue
        candidates.add(ab)
    print(f"[discover] {len(candidates):,} candidates after freq+PMI+stopword+t2s",
          file=sys.stderr)

    diagnose = args.diagnose
    if diagnose:
        candidates.update(w for w in diagnose if len(w) == 2)

    # ---- list-candidates: pass1-only quick look (tune steps 3-4) ----
    if args.list_candidates and not diagnose:
        ranked = sorted(candidates, key=lambda w: -bigram_freq[w])
        print(f"\n# {len(ranked)} candidates (pre-entropy, pre-unregistered). top80 + targets:")
        for w in ranked[:80]:
            tag = "REG" if w in registered else "gap"
            print(f"  {w}\t{bigram_freq[w]}\t{pmi(w):.2f}\t{tag}")
        print("# --- target words ---")
        for w in ["给力", "吐槽", "靠前", "榨干", "靠谱", "点赞"]:
            inc = "IN" if w in candidates else "OUT"
            print(f"  {w}\t{inc}\tfreq={bigram_freq[w]}\tpmi={pmi(w):.2f}"
                  f"\t{'REG' if w in registered else 'gap'}")
        return 0

    # ---- Pass 2: neighbor entropy for candidates ----
    BORDER = "\x00"
    left_nb: dict[str, Counter] = defaultdict(Counter)
    right_nb: dict[str, Counter] = defaultdict(Counter)
    for seg in iter_segments(source_names):
        for run in cjk_runs(seg):
            n = len(run)
            for i in range(n - 1):
                ab = run[i] + run[i + 1]
                if ab in candidates:
                    left_nb[ab][run[i - 1] if i > 0 else BORDER] += 1
                    right_nb[ab][run[i + 2] if i + 2 < n else BORDER] += 1
    print("[discover] pass2 done", file=sys.stderr)

    if diagnose:
        print(f"\n{'word':<8}{'freq':>9}{'pmi':>8}{'H_l':>7}{'H_r':>7}{'minH':>7}  registered")
        for w in diagnose:
            if len(w) != 2:
                print(f"{w:<8}  (only 2-char supported)")
                continue
            f, p = bigram_freq[w], pmi(w)
            hl, hr = entropy(left_nb[w]), entropy(right_nb[w])
            pstr = f"{p:.2f}" if p != float("-inf") else "-inf"
            reg = "REGISTERED" if w in registered else "—(gap)"
            print(f"{w:<8}{f:>9}{pstr:>8}{hl:>7.2f}{hr:>7.2f}{min(hl,hr):>7.2f}  {reg}")
        return 0

    # ---- Full run: tiered gate + unregistered filter ----
    # Two regimes, each targeting a different noise mode (see diagnose data):
    #   low-freq (< lowfreq_cut): person/place-name fragments sit at pmi < 2
    #     (丁飞 1.47 / 东浦 1.87 / 三牧 1.62) while real low-freq words are
    #     higher (榨干 2.93 / 靠前 2.46 / 靠谱 7.73) → gate on PMI.
    #   high-freq: cross-word collocations have low boundary entropy
    #     (点睡 2.10 / 句话 2.17 / 件事 2.30 / 前几 1.56) while real words are
    #     high (网红 5.06 / 给力 3.30 / 淡定 3.47) → gate on min entropy.
    discovered: list[tuple[str, int]] = []
    for ab in candidates:
        if ab in registered:
            continue
        f = bigram_freq[ab]
        mh = min(entropy(left_nb[ab]), entropy(right_nb[ab]))
        p = pmi(ab)
        if f < args.lowfreq_cut:
            if p < args.lowfreq_pmi or mh < args.min_entropy:
                continue
            # rare-char fragment guard: real low-freq words are built from
            # common chars (榨=2480, 谱=4847); name/place fragments contain
            # rare chars (谥=409, 蕴=917, 灶=1190) → drop if either char is rare.
            if min(char_freq[ab[0]], char_freq[ab[1]]) < args.lowfreq_char_floor:
                continue
        else:
            if p < args.hifreq_pmi or mh < args.hifreq_entropy:
                continue
        discovered.append((ab, f))
    discovered.sort(key=lambda x: (-x[1], x[0]))

    OUT.parent.mkdir(parents=True, exist_ok=True)
    with OUT.open("w") as f:
        f.write("# phrases_discovered.tsv — new-word discovery (CP3c), by discover_new_words.py\n")
        f.write(f"# sources: {','.join(source_names)}  gates: freq>={args.min_freq} "
                f"pmi>={args.min_pmi} entropy>={args.min_entropy} +stopword +t2s\n")
        f.write(f"# {len(discovered)} unregistered words. format: word\\tcorpus_freq\n")
        for w, c in discovered:
            f.write(f"{w}\t{c}\n")
    print(f"[discover] wrote {OUT} ({len(discovered)} unregistered words)", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
