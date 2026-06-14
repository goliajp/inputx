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

## LCCC-base · DEFERRED to Phase 2

Not fetched in Phase 0. Only available via Baidu Netdisk / Google Drive /
HuggingFace `datasets` library — no direct curl URL.

Phase 0's MIU evaluation harness is covered by wiki + THUCNews. LCCC's
colloquial/dialogue contribution is critical for Phase 2 LM training
(per `pinyin-quality-gap-2026-06-14.html` §06.7), where we'll use
`from datasets import load_dataset; load_dataset("lccc", "base")`.

When fetched, will live at `tools/eval/corpus/raw/lccc/`.

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
| 2026-06-14 | initial fetch, branch `feature/eval-cp-0.1-corpus-fetch` | doracawl + claude opus 4.7 |
