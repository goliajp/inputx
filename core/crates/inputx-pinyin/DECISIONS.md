# golia-pinyin · key decisions

Append-only log of consequential design / data / license decisions. Each
entry: what was chosen, what was rejected, why. New entries go at the top.

---

## D2 — jieba dict.txt as phrase source (2026-05-11)

**Decision:** vendor `fxsjy/jieba/jieba/dict.txt` (5.07 MB, 349,046 entries) as the v0.2 phrase source. Strip the POS tag column, keep `word\tfreq` only.

**Why:** completes the v0.2 data pipeline without requiring a separate corpus n-gram extraction step. jieba is a well-established Chinese segmenter; its default dictionary is the de facto reference for "common Chinese phrases" in MIT-licensed open source.

**License audit findings:**
- jieba's root `LICENSE` = MIT (Copyright © 2013 Sun Junyi).
- No separate data-attribution / source-credit file in the repo (no `DATA_SOURCES.md`, `ATTRIBUTION`, etc.).
- README mentions only the dict file format, not upstream sources.
- Conclusion: jieba publishes dict.txt under MIT via the blanket repo LICENSE; downstream use is permitted per MIT (preserve copyright notice).

**Attribution obligation:** bundle `LICENSE-JIEBA` (verbatim copy of jieba's MIT notice) in the published crate root; reference jieba in README + this file.

**Caveat (low-risk):** if jieba upstream were technically derived from BY-SA sources without disclosure, downstream liability is theoretical but unprovable. Standard OSS practice trusts upstream license declarations; we document our use chain so any future challenge can be addressed cleanly.

**Scope of use:**
- Vendor only `jieba/dict.txt` (NOT `dict.txt.big` or other variants — stay minimal).
- Drop POS tag column; keep only `word\tfreq`.
- jieba's `freq` used as input ranking signal during heteronym candidate selection (item 16); final per-pair frequency comes from our own corpus pipeline (items 18-21).

**Fallback if audit changes:** if any future jieba release or independent review surfaces upstream contamination, fall back to corpus n-gram extraction (do items 18-22 first, derive a phrase list from corpus segmentation, then return to compose).

---

## D1 — pinyin reading data sources (2026-05-11)

**Decision:** v0.2 data pipeline uses **Unihan + algorithmic phrase composition + hand-curated heteronym overrides**. Skip OpenCC, CC-CEDICT, Wiktionary, pypinyin data.

**Why:** keeps the published binary cleanly MIT/Apache-2.0 with zero copyleft inheritance risk. Same principle that drove the v0.1 bootstrap pivot (CC-CEDICT → hand-curated, see CHANGELOG).

### Source survey

| Source | License | Pinyin data? | Verdict | Rationale |
|---|---|---|---|---|
| **Unihan database** | Unicode License v3 (permissive, MIT-compatible) | Yes — `kMandarin` (primary single-char reading), `kHanyuPinyin` (multi-reading + freq from 汉语大词典), `kXHC1983`, `kTGHZ2013` | ✅ **Use** | Authoritative single-char source; PD-equivalent terms; baked-in attribution requirement is trivial (one line in README + LICENSE-UNICODE bundled in published crate). Covers ~92k CJK chars including Ext A–G. |
| **OpenCC** | Apache 2.0 | **No** — surveyed `BYVoid/OpenCC/data/dictionary/` directly: only S↔T conversion tables (STCharacters / STPhrases / TSPhrases / HKVariants / JPShinjitai / TWPhrases). Zero pinyin data. | ❌ **Skip** | My initial recommendation listed OpenCC for word-level pinyin — that was wrong; OpenCC is a Simplified↔Traditional converter, not a pinyin resource. |
| **CC-CEDICT** | CC-BY-SA 3.0 | Yes — word-level `traditional simplified [pinyin] /english/` | ❌ **Skip** | ShareAlike clause: any work derived from BY-SA data must also be BY-SA. Shipping a derived FST in our binary likely propagates SA to downstream consumers (incl. inputx). Even the conservative reading — that frequencies are factual but reading-mappings are creative — is enough to disqualify. |
| **Wiktionary zh dump** | CC-BY-SA 4.0 (text) + GFDL | Yes — per-word readings in entry markup | ❌ **Skip** | Same SA issue. Plus extraction effort is high (parse MediaWiki templates). |
| **pypinyin data files** | Code MIT; data provenance unclear | Yes — single-char + phrase dicts | ❌ **Skip** | pypinyin's `pinyin_dict.py` is plausibly Unihan-clean; phrase dict (`phrases_dict.py`) historically credits CC-CEDICT-derived sources — same SA risk. Even MIT-labeled, you can't relicense BY-SA upstream as MIT, so the actual data may carry BY-SA invisibly. Avoid the audit cost; we already have a clean path. |
| **THUOCL** (清华大学开放中文词库) | Unspecified / academic-restrictive in places | Some lists have pinyin | ❌ **Skip** | License unclear → can't ship in MIT/Apache binary. Academic word lists aren't worth the audit effort given alternatives. |
| **rime-pinyin / sunpinyin / fcitx** | GPL / LGPL | Yes | ❌ **Skip** | Same reason we kicked rime-wubi out at wubi v0.2: GPL/LGPL contamination of distributed binary. |
| **Hand-curated heteronyms** | Original work, MIT/Apache | Targeted (500-1000 entries) | ✅ **Use** | Covers the genuinely ambiguous phrases (银行, 长大, 重要, 还有, 都是, …) that algorithmic composition can't disambiguate. ~1 day of careful curation. |
| **Corpus frequency signals** | Per-corpus license (Leipzig CC-BY 4.0; SUBTLEX-CH CC-BY 4.0) | N/A — provides freq, not readings | ✅ **Use** (as in wubi v0.3-v0.4) | Frequencies are factual data, not derivative works in any meaningful sense. Already cleared at wubi v0.3. |

### Composed pipeline (v0.2)

```
1. Unihan kMandarin + kHanyuPinyin
   → readings_unihan.tsv  (single-char level, ~92k chars × ~1-3 readings each)

2. Corpus high-freq phrase extraction (Leipzig + SUBTLEX-CH)
   → phrases_freq.tsv  (top ~30-50k phrases by freq, no reading yet)

3. Algorithmic composition: for each phrase in phrases_freq.tsv,
   look up each char's readings in readings_unihan.tsv,
   emit cartesian product as candidate (pinyin_string, phrase) entries
   → phrases_composed.tsv  (multiple candidate readings per phrase)

4. Hand-curated heteronym override: ~500-1000 entries listing the
   correct reading for known-ambiguous phrases.
   → heteronyms_curated.tsv

5. Final dict assembly:
   - Take readings_unihan.tsv as the single-char base
   - Take phrases_composed.tsv FILTERED by heteronyms_curated.tsv
     (when a phrase appears in heteronyms, only the curated reading wins)
   - Apply corpus freq from wubi-style weights pipeline
   → pinyin-data-v1.tar.gz (FST + provenance.toml)
```

### Heteronym strategy

The non-trivial part of v0.2 is the heteronym override. Without it, 银行
generates both `yinhang` AND `yinxing` candidates, with no way for the
ranker to know which is correct in the absence of dialog context (which
we don't have in an IME).

The pragmatic minimum: hand-curate the top ~500 heteronyms (by corpus
freq of the phrase), bias them toward the correct reading at compile
time. The remaining long tail accepts both readings; the L0 user-learning
in v0.3 (Phase 3 J) handles per-user disambiguation organically.

Reference list to start from (~50 most common heteronyms; expand from
corpus):
银行 yinhang, 重要 zhongyao, 都是 doushi, 长大 zhangda, 还是 haishi,
还有 haiyou, 没有 meiyou, 行不行 xingbuxing, 好像 haoxiang, 看见 kanjian,
重庆 chongqing, 长沙 changsha, 朝阳 chaoyang, …

### Attribution obligations

Bundled in the published crate's `README.md` + `LICENSE-UNICODE`:

> Single-character readings derived from the Unicode Character Database (UCD), specifically the Unihan kMandarin and kHanyuPinyin fields. © 1991-2026 Unicode, Inc. Distributed under the Unicode License v3. https://www.unicode.org/license.txt

Per-corpus attribution clauses already established in wubi `README.md`
and `data/weights/provenance.toml`; pinyin will mirror that pattern.

### Rejected alternatives

- **Use CC-CEDICT for "just the frequencies"**: even strict frequency-only
  extraction has BY-SA risk if courts interpret the schema choice
  (which words exist in the dict) as creative selection. Conservative
  default: skip entirely.
- **Use pypinyin's data behind feature flag**: doesn't help — once shipped,
  audit liability remains. Cleaner to never depend on it.
- **Defer phrase-level pinyin to v0.3**: tempting but means Phase 3 (J/K/L)
  publishes a single-char-only dict, which the dual-engine in inputx
  would find anemic. Phrase-level is core to commercial-grade.

---

## D0 — bootstrap dict source (2026-05-10)

**Decision:** v0.1 bootstrap dict is hand-curated (`data/bootstrap.tsv`, ~125 entries), not CC-CEDICT subset.

**Why:** original ROADMAP plan called for CC-CEDICT subset (CC-BY-SA 4.0), but shipping CC-BY-SA-derived FST data in an MIT/Apache binary risks SA propagation into inputx (where pinyin lib will be a path/version dep). Hand-curating ~125 most-common entries (50 single chars + 25 phrases) takes < 1h and is original work. v0.2 replaces this with the self-collected pipeline (D1).

**Recorded in:** v0.1 commit `86540db` ("license-clean MIT/Apache, replacing planned CC-CEDICT to avoid SA propagation").
