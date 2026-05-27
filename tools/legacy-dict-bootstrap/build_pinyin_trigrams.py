#!/usr/bin/env python3
"""Extract Chinese word trigrams (a, b, c) from cached Leipzig corpora.

Output: `tools/scoring/data/supplemental/pinyin_trigrams_v1.tsv` with rows
`a\tb\tc\tcount` sorted by count desc, capped at top-N.

Trigrams are the v1.2 联想 (next-word prediction) data source. With
bigrams alone, prediction is locally greedy — committing top-bigram every
time produces strings like "今天的是在年" where each link is high-freq
but the chain isn't a coherent sentence. Trigrams capture the (a, b, c)
co-occurrence — so after committing 今天 then 的, predicting the next
word uses (今天, 的, *) bigram-of-pairs which is way more selective than
just (的, *).

Source: same corpora as bigrams — zho_wikipedia_2018_1M +
zho_news_2020_100K. Same jieba tokenization + CJK filter. Same per-token
char-split capture (intra-phrase trigrams) so common 3-char phrases
contribute their (a,b,c) signal even when jieba unitized them.

Run: python3 tools/scoring/build_pinyin_trigrams.py [--top N]
"""
from __future__ import annotations
import argparse
import sys
import tarfile
from collections import Counter
from pathlib import Path

try:
    import jieba
except ImportError:
    raise SystemExit("pip3 install --user --break-system-packages jieba")

try:
    from opencc import OpenCC
    _t2s = OpenCC("t2s")
except ImportError:
    raise SystemExit(
        "pip3 install --user --break-system-packages opencc-python-reimplemented"
    )

def to_simplified(s: str) -> str:
    """Same rationale as build_pinyin_bigrams.py::to_simplified — keep
    the trigram FST simplified-only so predictions can't surface
    traditional variants."""
    return _t2s.convert(s)

ROOT = Path(__file__).resolve().parent.parent.parent
CACHE = ROOT / "core/crates/inputx-pinyin/data/corpus/cache"
# Same inter/intra split rationale as build_pinyin_bigrams.py — predict_*
# uses inter only; Viterbi composition reads both. v1.3 联想-conservative
# (2026-05-24): without this split, prediction chains spawn from
# intra-phrase char triples (椒,粉,碎) and the user ends up with
# nonsense compound strings.
OUT_INTER = ROOT / "tools/scoring/data/supplemental/pinyin_trigrams_inter_v1.tsv"
OUT_INTRA = ROOT / "tools/scoring/data/supplemental/pinyin_trigrams_intra_v1.tsv"

SOURCES = [
    "zho_wikipedia_2018_1m",
    "zho_news_2020_100k",
]


def has_cjk(w: str) -> bool:
    return any("一" <= c <= "鿿" for c in w)


def iter_sentences(tar_path: Path):
    with tarfile.open(tar_path, "r:gz") as tar:
        members = [m for m in tar.getmembers()
                   if m.isfile() and m.name.endswith("-sentences.txt")]
        for m in members:
            f = tar.extractfile(m)
            if f is None:
                continue
            for raw in f:
                try:
                    line = raw.decode("utf-8", errors="replace").rstrip("\r\n")
                except Exception:
                    continue
                parts = line.split("\t", 1)
                if len(parts) != 2:
                    continue
                yield parts[1]


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--top", type=int, default=1_000_000,
                    help="Keep top N trigrams (default 1M — trigrams have higher cardinality than bigrams).")
    ap.add_argument("--min-count", type=int, default=2,
                    help="Drop trigrams with count < this (default 2 — trigrams are sparser than bigrams).")
    args = ap.parse_args()

    jieba.initialize()
    counter_inter: Counter[tuple[str, str, str]] = Counter()
    counter_intra: Counter[tuple[str, str, str]] = Counter()
    total_sents = 0
    total_inter = 0
    total_intra = 0

    for src in SOURCES:
        path = CACHE / src
        if not path.exists():
            print(f"warning: {path} missing — run pinyin-fetch-corpus first",
                  file=sys.stderr)
            continue
        print(f"[trigrams] processing {src} ({path.stat().st_size // 1024 // 1024} MB)",
              file=sys.stderr)
        for sent in iter_sentences(path):
            total_sents += 1
            sent_simp = to_simplified(sent)
            words = [w for w in jieba.cut(sent_simp, HMM=True) if has_cjk(w)]
            # Inter-token trigrams (predict_next_words_context input).
            for a, b, c in zip(words, words[1:], words[2:]):
                counter_inter[(a, b, c)] += 1
                total_inter += 1
            # Intra-token char trigrams (Viterbi composition input only).
            for tok in words:
                if len(tok) >= 3:
                    chars = list(tok)
                    for a, b, c in zip(chars, chars[1:], chars[2:]):
                        counter_intra[(a, b, c)] += 1
                        total_intra += 1
            if total_sents % 100_000 == 0:
                print(f"  ... {total_sents} sents, "
                      f"inter={len(counter_inter)} intra={len(counter_intra)}",
                      file=sys.stderr)

    print(f"[trigrams] total sents={total_sents} inter-occ={total_inter} "
          f"intra-occ={total_intra}", file=sys.stderr)
    print(f"[trigrams] unique inter={len(counter_inter)} intra={len(counter_intra)}",
          file=sys.stderr)

    def write_file(out_path: Path, counter: Counter, label: str):
        filtered = [(a, b, c, n) for (a, b, c), n in counter.items()
                    if n >= args.min_count]
        filtered.sort(key=lambda x: (-x[3], x[0], x[1], x[2]))
        filtered = filtered[: args.top]
        out_path.parent.mkdir(parents=True, exist_ok=True)
        with out_path.open("w") as f:
            f.write(f"# Pinyin word trigrams ({label}) — by build_pinyin_trigrams.py\n")
            f.write(f"# source: {', '.join(SOURCES)}\n")
            f.write(f"# stats: kept {len(filtered)} / {len(counter)} unique "
                    f"(min_count={args.min_count}, top={args.top})\n")
            f.write("# format: a\\tb\\tc\\tcount\n")
            for a, b, c, n in filtered:
                f.write(f"{a}\t{b}\t{c}\t{n}\n")
        print(f"[trigrams] wrote {out_path} ({len(filtered)} rows)", file=sys.stderr)

    write_file(OUT_INTER, counter_inter, "inter-token")
    write_file(OUT_INTRA, counter_intra, "intra-token char-triple")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
