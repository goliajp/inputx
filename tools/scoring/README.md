# `tools/scoring/` — Static-DB build pipeline

Operator's guide for the 8-step pipeline that builds Inputx's
unified-score `weights.tsv`. Each step is independently runnable
and cacheable. See [`docs/SCORING.md`](../../docs/SCORING.md) for the
design rationale; this README is *how to run the thing*.

## Quickstart

```bash
# Full rebuild (slow, ~hours):
make rebuild

# Single step:
python 01_fetch/fetch_wikipedia_zh.py --version 20260501

# Validate without rebuilding:
make validate
```

## Pipeline overview

```
Source corpora                 step             output
───────────────────────────    ────             ──────
  Wikipedia (zh, ja)           01_fetch         data/raw/*
  jieba bundled dict
  Leipzig Corpora              02_extract       data/extracted/<source>/freq.tsv
  Unihan readings              03_normalize     data/normalized/<source>/score.tsv
  ...                          04_layer_assign  data/layers/{wubi,pinyin,jp}.tsv
                               05_merge         data/merged/weights.tsv
                               06_llm_annotate  data/annotations/llm_overrides.tsv
                               07_validate      reports/build-<version>.html
                               08_pack          ../../core/crates/inputx-pinyin/data/pinyin.dict
                                                (wubi: build.rs → OUT_DIR/wubi86.dict)
```

Each step writes to `data/` (gitignored) — only the final `.dict` /
`.fsa` artifacts (self-built `inputx-fsa`, not the `fst` crate) in
`core/crates/*/data/` are committed. (Wubi's `wubi86.dict` is built at
compile time by `inputx-wubi/build.rs` into `OUT_DIR`, not committed.)

## Steps in detail

### 01_fetch — versioned corpus download

Each source has its own fetcher script. Pinned by version + sha256
so rebuilds are deterministic. Run order doesn't matter; sources
are independent.

```
01_fetch/
  fetch_wikipedia_zh.py        --version 20260501 → data/raw/wiki-zh-20260501.xml.bz2
  fetch_wikipedia_ja.py        --version 20260501 → data/raw/wiki-ja-20260501.xml.bz2
  fetch_jieba.py               --version 0.42.1   → data/raw/jieba-dict-0.42.1.txt
  fetch_leipzig_zh.py          --version 2024     → data/raw/leipzig-zh-2024.tar.gz
  fetch_unihan.py              --version 15.1     → data/raw/Unihan-15.1.zip
  fetch_kanjidic2.py           --version 2024.05  → data/raw/kanjidic2-2024.05.xml.gz
  manifest.toml                  ← lists currently-pinned versions
```

`manifest.toml` is the **single source of truth** for what corpus
versions feed the build. Updating the dict = bump entries here +
re-run pipeline.

### 02_extract — corpus → (word, freq)

Per-source extraction. Each script outputs a uniform 2-column TSV:

```
data/extracted/<source>/freq.tsv:
  <word>\t<absolute_count>
```

```
02_extract/
  extract_wikipedia_zh.py      data/raw/wiki-zh-*.xml.bz2 → data/extracted/wikipedia-zh/freq.tsv
  extract_wikipedia_ja.py      → data/extracted/wikipedia-ja/freq.tsv
  extract_jieba.py             → data/extracted/jieba/freq.tsv
  extract_leipzig_zh.py        → data/extracted/leipzig-zh/freq.tsv
  extract_unihan_readings.py   → data/extracted/unihan/readings.tsv
                                  data/extracted/unihan/codepoints.tsv
  extract_kanjidic2.py         → data/extracted/kanjidic2/readings.tsv
  segment_with_jieba.py        --input data/raw/<corpus> → freq w/ proper word boundaries
```

### 03_normalize — log-rank normalization

Cross-source comparison is the hard part. Wikipedia counts are
absolute (millions). jieba weights are arbitrary integers. Leipzig
is per-million normalized. We unify via log-rank within source:

```python
# Per source:
sorted_words = sort(words by freq desc)
for rank, (word, freq) in enumerate(sorted_words):
    score = 1.0 - log(rank + 1) / log(len(sorted_words))   # ∈ [0, 1]
    out.write(f"{word}\t{score:.6f}\n")
```

Output: `data/normalized/<source>/score.tsv`, all scores comparable
in `[0, 1]`.

### 04_layer_assign — wubi simcode floors

Wubi has a structural hierarchy that must be respected. Reads
`core/crates/inputx-wubi/data/jianma*.txt` + `seed.txt` + `zigen86.txt`
to know which entries belong in which layer. Writes:

```
data/layers/wubi.tsv:
  <code>\t<word>\t<layer>           # Jianma1/Jianma2/Jianma3/Zigen/Phrase/Auto
data/layers/pinyin.tsv:
  <code>\t<word>\t<layer>           # SingleChar/Phrase
data/layers/jp.tsv:
  <code>\t<word>\t<layer>           # Jukugo/SingleKanji/Kana
```

Layer choice determines the runtime `layer_floor` multiplier; see
SCORING.md §2.

### 05_merge — unify into weights.tsv

Combines normalized scores + layer assignments. For each engine:

```
final_score(code, word) = engine_mult × layer_floor × normalized_score × length_bias
```

Applies cross-engine collision detection — when a wubi Phrase code
also exists in pinyin as a high-freq reading (e.g., `jixu` for both
曳光弹 wubi + 继续 pinyin), the wubi Phrase gets demoted unless its
freq dominates.

Output:
```
data/merged/weights.tsv:
  # version: 2026.05.22
  # source-versions: { ... }
  <code>\t<word>\t<engine>\t<layer>\t<score>
```

### 06_llm_annotate — boundary-case rerank

Reads merged weights, finds codes with ambiguous top-N (top-2 score
gap < 5%), batches them to Claude API for judgment. Output:

```
data/annotations/llm_overrides.tsv:
  <code>\t<preferred_word>\t<confidence>\t<model>\t<reason>
```

Re-merge with overrides as score boost (+50% on LLM-preferred top).

Cost: ~$10 per full run. Cached per-prompt so re-runs are free.
See SCORING.md §3 for prompt template + reproducibility notes.

### 07_validate — quality gates

Four gates (see SCORING.md §1.4):

```
07_validate/
  gate1_regression_corpus.py       reads tests/input_corpus.tsv
  gate2_stability_budget.py        compares vs previous release
  gate3_coverage_delta.py          row counts vs previous
  gate4_llm_judge.py               samples 200 codes, asks LLM
  report.py                        writes reports/build-<version>.html
```

A failing gate aborts the build. The report is reviewed by a human
before the artifact ships.

### 08_pack — emit index artifacts

Final step: `weights.tsv` → `inputx-fsa` two-level `Dict` binary
(`pinyin.dict`; wubi's `wubi86.dict` is emitted by `inputx-wubi/build.rs`
at compile time). Packing format `(layer << FREQ_BITS) | freq_score`
with `FREQ_BITS = 20` (zerodep E1 dense-pack; was `<< 56` under the old
`fst`-crate path).

## Tools

- **`make rebuild`** — full pipeline 01–08
- **`make validate`** — just step 07 against current merged data
- **`make clean`** — wipe `data/` (NOT the committed `*.dict` / `*.fsa`)
- **`make diff-vs-shipped`** — show top-100 candidate-list changes
  between current pipeline output and what's currently shipped

## Versioning

Each `weights.tsv` header includes the source-version manifest. To
reproduce a build:

```bash
git checkout v1.2.3                     # repo state
cp manifest-v1.2.3.toml 01_fetch/manifest.toml
make rebuild                            # gets same data, same weights
```

## Reproducibility checklist

Before promoting a pipeline output to a release:

- [ ] `manifest.toml` source versions all pinned (no "latest").
- [ ] LLM model + prompt version pinned (no "claude-3-opus" — use the dated alias).
- [ ] All 4 validate gates pass.
- [ ] Top-100 candidate-list diff vs previous release manually reviewed.
- [ ] Polish-log corpus from previous release has been folded into `input_corpus.tsv`.
- [ ] Source-version manifest checked into git as `manifest-v<version>.toml`.

## Distribution

The final `pinyin.dict` artifact is committed to `inputx-pinyin/data/`;
wubi's `wubi86.dict` is built into `OUT_DIR` by `inputx-wubi/build.rs`
(not committed). App builds embed them via `include_bytes!`. Side-loading
via override directory is a v0.4 future (see SCORING.md §1.3).
