# Evaluation corpus sources

Phase 0 evaluation harness corpus. Used to build MIU test set
(see [pinyin-todo-board.html](../../docs/research/pinyin-todo-board.html)
CP-0.2 → CP-0.5).

Files live under `tools/eval/corpus/raw/` and are gitignored — regenerable
from upstream dumps. Directories kept via `.gitkeep`.

---

## zhwiki (severe written-style baseline)

| field | value |
|---|---|
| URL | https://dumps.wikimedia.org/zhwiki/latest/zhwiki-latest-pages-articles.xml.bz2 |
| local path | `tools/eval/corpus/raw/wiki/zhwiki-latest-pages-articles.xml.bz2` |
| compressed size | **3.1 GB** (actual, verified 2026-06-14 22:56) |
| sha256 (prefix) | `e4d157bef5b3e22a…` |
| extracted size | ~15-18 GB raw XML, 2-3 GB cleaned text (after wikiextractor) |
| license | CC BY-SA 3.0 + GFDL |
| commerce_risk | safe (attribution required) |
| fetched | 2026-06-14 |
| notes | Traditional/simplified mixed → must run OpenCC `t2s.json`. Wiki markup heavy → use wikiextractor. Disambiguation pages filter recommended. |

---

## THUCNews (news baseline — replaces People's Daily)

| field | value |
|---|---|
| URL | http://thuctc.thunlp.org/source/THUCNews.zip |
| local path | `tools/eval/corpus/raw/pd/THUCNews.zip` |
| compressed size | **1.5 GB** (actual, verified 2026-06-14 23:19) |
| sha256 (prefix) | `8d6aef71c4431ba2…` |
| extracted size | 2.4 GB / 1,672,196 files |
| document count | 740,000+ articles in 14 categories (财经/体育/科技/...) |
| time span | 2005-2011 (Sina RSS based) |
| license | research-only (commercial use risk; OK for internal eval) |
| commerce_risk | note (eval only — do not ship redistributively) |
| fetched | 2026-06-14 |
| notes | Chosen over People's Daily online scraping for stability. Older timespan → lacks recent vocabulary, but MIU evaluation cares about word-formation patterns more than freshness. |

---

## LCCC-base (colloquial / dialogue · fetched in Phase 2)

| field | value |
|---|---|
| URL | https://huggingface.co/datasets/silver/lccc (HF datasets, no direct curl) |
| local path | `tools/eval/corpus/raw/lccc/` |
| compressed size | TBD — populated after CP-2.2 fetch |
| sha256 (prefix) | TBD — populated after CP-2.2 fetch |
| extracted size | ~1.5-2 GB plain text (one utterance per line, after dedup) |
| document count | ~6.8M dialogue sessions / ~12M utterances (base split) |
| license | MIT (per thu-coai/CDial-GPT) |
| commerce_risk | note (ship derived bigram OK; do NOT redistribute raw corpus) |
| fetched | TBD — Phase-2 CP-2.2 will run `from datasets import load_dataset; load_dataset("lccc", "base")` |
| notes | Originally deferred from Phase 0 (no direct curl URL). LCCC's colloquial/dialogue contribution is critical for Phase 2 LM training per `pinyin-quality-gap-2026-06-14.html` §06.7. Underlying user-generated content (Weibo / open forums) — license per LCCC packaging is MIT but cite Wang et al. EMNLP-Findings 2020 in shipped LICENSES_THIRD_PARTY.md. |

---

## CC-100 zh (web-scale diverse-domain · Phase 2 only)

| field | value |
|---|---|
| URL | https://huggingface.co/datasets/cc100 (HF datasets, no direct curl) |
| local path | `tools/eval/corpus/raw/cc100/` |
| compressed size | TBD — target ~10 GB subsample after CP-2.2 |
| sha256 (prefix) | TBD — populated after CP-2.2 fetch |
| extracted size | ~10 GB plain text (subsample; full zh ≈ 50 GB) |
| license | MIT (cc_net pipeline) + Common Crawl Terms of Use |
| commerce_risk | note (dataset card states "intended for non-commercial language modeling research"; derived bigram OK to ship — smoothed probabilities are not verbatim) |
| fetched | TBD — Phase-2 CP-2.2 will stream + sub-sample on the fly |
| notes | Use streaming mode to avoid downloading the full 50 GB zh split before subsampling. If ship-time legal escalates this to `blocked`, retrain without CC-100 by dropping the id from `tools/scoring/09_bigram_lm/sources.yaml` and re-running CP-2.2..CP-2.3. Cite Conneau et al. ACL 2020. |

---

## Phase 2 LM training manifest

The 4 sources above are the LM training corpus per climb plan CP-2.1.
Machine-readable manifest with license + commerce_risk fields lives at
[`tools/scoring/09_bigram_lm/sources.yaml`](../scoring/09_bigram_lm/sources.yaml).
CP-2.1 acceptance gate (`sources.yaml` has 4 entries, all with license +
commerce_risk, no `blocked`) is **met**.

---

## How to verify the corpus is fully fetched

```bash
# Sizes
du -sh tools/eval/corpus/raw/{wiki,pd}

# Wiki line count (after extraction)
ls -lh tools/eval/corpus/raw/wiki/

# News count
unzip -l tools/eval/corpus/raw/pd/THUCNews.zip | tail -1
```

Expected after CP-0.1 completes:
- `wiki/` directory contains `zhwiki-latest-pages-articles.xml.bz2` ≥ 3 GB → **3.1 GB ✓**
- `pd/` directory contains `THUCNews.zip` ≥ 1 GB → **1.5 GB ✓**
- All directories present, gitignored, `.gitkeep` committed → **✓**

### Sanity decompress checks (run 2026-06-14)

```
$ bzcat wiki/zhwiki-latest-pages-articles.xml.bz2 | head -c 1500
<mediawiki xmlns="http://www.mediawiki.org/xml/export-0.11/" ...
  <siteinfo><sitename>Wikipedia</sitename><dbname>zhwiki</dbname>...
  → 标准 mediawiki dump XML 头, OK
```

```
$ unzip -p pd/THUCNews.zip "THUCNews/财经/798977.txt" | head -c 500
新增资金入场 沪胶强势创年内新高
　　记者 魏曙光 ...
  → 合法简体中文新闻文本, OK
```

---

## Provenance log

| date | event | actor |
|---|---|---|
| 2026-06-14 | Phase 0 initial fetch (wiki + THUCNews), branch `feature/eval-cp-0.1-corpus-fetch` | doracawl + claude opus 4.7 |
| 2026-06-15 | CP-2.1: LCCC + CC-100 entries added, sources.yaml manifest written, license + commerce_risk audited, branch `feature/lm-cp-2.1-sources-yaml` | doracawl + claude opus 4.7 |
