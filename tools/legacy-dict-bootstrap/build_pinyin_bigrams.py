#!/usr/bin/env python3
"""Extract Chinese word-bigram counts from cached Leipzig corpora.

Output: `tools/scoring/data/supplemental/pinyin_bigrams_v1.tsv` with rows
`prev_word\tnext_word\tcount` sorted by count desc, capped at top-N.

The bigram table is consumed by the pinyin engine to rerank candidates
based on the most-recently-committed word. This is the offline analogue
of Sogou's per-user freq adaptation — instead of cloud-trained per-user
freq, we use static cross-corpus bigram counts to capture the "what word
usually follows what" signal.

Architecture rationale:
  - Today (v1.1.x): pinyin candidate score = base + freq. No context.
  - With bigrams: pinyin candidate score = (base + freq) ×
    bigram_boost(last_committed_word, candidate). last_committed empty
    → multiplier = 1.0 (no behavior change for cold sessions).
  - Closes a major UX gap vs Sogou (which makes "今天 → 好" #0 because
    "今天好" is a high-count bigram).

Source: zho_wikipedia_2018_1M (~1M sentences) + zho_news_2020_100K
(~100K sentences). SUBTLEX is freq-list, not raw text, so skipped.

Run: python3 tools/scoring/build_pinyin_bigrams.py [--top N]
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
    """Pipe a string through OpenCC traditional→simplified.

    Why: Leipzig zho_wikipedia_2018_1M contains mixed simplified/traditional
    content (zh.wikipedia is bilingual). Without t2s, bigrams like
    (是, 於) and (是, 于) BOTH exist in the FST, and `predict_next_words("是")`
    returns 於 as a top hit (user-reported 2026-05-24 — predictions
    panel showed 1於 2于 3此 ... in a simplified-mode session).
    Converting at extraction time means the FST has SIMPLIFIED-ONLY
    keys/values, so traditional forms can never leak into predictions
    regardless of user mode (we always simplify; mode only affects
    candidate-side display, not corpus content).
    """
    return _t2s.convert(s)

ROOT = Path(__file__).resolve().parent.parent.parent
CACHE = ROOT / "core/crates/inputx-pinyin/data/corpus/cache"
# TWO output files (separation matters — see v1.0 联想 design):
#   inter: word-token-to-word-token bigrams (truly adjacent in corpus
#          after jieba). USED BY predict_next_words for next-word
#          predictions. Required to avoid prediction chains spawning
#          from intra-phrase char pairs (椒→粉 etc.) that aren't
#          real sentence continuations.
#   intra: char-pair bigrams within a multi-char token (你好 → (你, 好)).
#          USED BY Viterbi composition (bigram_boost) to help segment
#          long buffers into known phrases. Has zero value for
#          predictions — it's "what chars co-occur inside one word",
#          not "what word follows what word".
OUT_INTER = ROOT / "tools/scoring/data/supplemental/pinyin_bigrams_inter_v1.tsv"
OUT_INTRA = ROOT / "tools/scoring/data/supplemental/pinyin_bigrams_intra_v1.tsv"

# Leipzig tarballs contain a directory like `zho_wikipedia_2018_1M/`
# with several files; the one we want is `*-sentences.txt`. Each line:
#   <id>\t<sentence>
SOURCES = [
    "zho_wikipedia_2018_1m",
    "zho_news_2020_100k",
]


def has_cjk(w: str) -> bool:
    return any("一" <= c <= "鿿" for c in w)


def iter_sentences(tar_path: Path):
    """Yield raw sentence strings from a Leipzig tar.gz."""
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
                # `<id>\t<sentence>` — split once, take the tail.
                parts = line.split("\t", 1)
                if len(parts) != 2:
                    continue
                yield parts[1]


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--top", type=int, default=500_000,
                    help="Keep top N bigrams (default 500k).")
    ap.add_argument("--min-count", type=int, default=3,
                    help="Drop bigrams with count < this (default 3).")
    args = ap.parse_args()

    jieba.initialize()
    counter_inter: Counter[tuple[str, str]] = Counter()
    counter_intra: Counter[tuple[str, str]] = Counter()
    total_sents = 0
    total_inter = 0
    total_intra = 0

    for src in SOURCES:
        path = CACHE / src
        if not path.exists():
            print(f"warning: {path} missing — run pinyin-fetch-corpus first",
                  file=sys.stderr)
            continue
        print(f"[bigrams] processing {src} ({path.stat().st_size // 1024 // 1024} MB)",
              file=sys.stderr)
        for sent in iter_sentences(path):
            total_sents += 1
            # 1) t2s simplify FIRST — drops traditional variants before
            #    they ever enter the bigram counter (see to_simplified).
            # 2) jieba segment on the simplified sentence.
            sent_simp = to_simplified(sent)
            words = [w for w in jieba.cut(sent_simp, HMM=True) if has_cjk(w)]
            # Inter-token bigrams (predict_next_words input): truly
            # adjacent tokens — real "what word follows what word"
            # signal. Prediction chains can ONLY be built from this set;
            # see header comment for the rationale.
            for prev, nxt in zip(words, words[1:]):
                counter_inter[(prev, nxt)] += 1
                total_inter += 1
            # Intra-token char bigrams (Viterbi composition input):
            # adjacent chars inside one multi-char jieba token. Captures
            # (你, 好) from "你好". Helps Viterbi segment a long buffer
            # into known phrases, but has zero meaning as a next-word
            # prediction — kept STRICTLY separate from inter.
            for tok in words:
                if len(tok) >= 2:
                    chars = list(tok)
                    for a, b in zip(chars, chars[1:]):
                        counter_intra[(a, b)] += 1
                        total_intra += 1
            if total_sents % 100_000 == 0:
                print(f"  ... {total_sents} sents, "
                      f"inter={len(counter_inter)} intra={len(counter_intra)}",
                      file=sys.stderr)

    print(f"[bigrams] total sents={total_sents} inter-occ={total_inter} "
          f"intra-occ={total_intra}", file=sys.stderr)
    print(f"[bigrams] unique inter={len(counter_inter)} intra={len(counter_intra)}",
          file=sys.stderr)

    def write_file(out_path: Path, counter: Counter, label: str):
        filtered = [(p, n, c) for (p, n), c in counter.items() if c >= args.min_count]
        filtered.sort(key=lambda x: (-x[2], x[0], x[1]))
        filtered = filtered[: args.top]
        out_path.parent.mkdir(parents=True, exist_ok=True)
        with out_path.open("w") as f:
            f.write(f"# Pinyin word bigrams ({label}) — by build_pinyin_bigrams.py\n")
            f.write(f"# source: {', '.join(SOURCES)}\n")
            f.write(f"# stats: kept {len(filtered)} / {len(counter)} unique "
                    f"(min_count={args.min_count}, top={args.top})\n")
            f.write("# format: prev\\tnext\\tcount\n")
            for p, n, c in filtered:
                f.write(f"{p}\t{n}\t{c}\n")
        print(f"[bigrams] wrote {out_path} ({len(filtered)} rows)", file=sys.stderr)

    write_file(OUT_INTER, counter_inter, "inter-token")
    write_file(OUT_INTRA, counter_intra, "intra-token char-pair")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
