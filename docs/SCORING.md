# Candidate Scoring System

The polish-quality target for Inputx v0.2+. Replaces today's layered
hard-rule merge (wubi → JP kanji → pinyin → JP kana) with a unified,
data-driven score that all engines feed into.

## TL;DR

```
score(c) = ENGINE_MULT[c.engine] × LAYER_FLOOR[c.layer] × normalized_freq(c) × LENGTH_BIAS(c) × (1 + L0_BOOST(c))
```

Sort all candidates by `score` desc. The **only hard rule** is wubi
一级简码 / 二级简码 — Inputx is *Inputx 五笔*, those entries get a
score floor that no other source can beat. Everything else is
quantitative.

## Why we're doing this

Today (Phase 0) hard-codes layered merge order. Symptoms:

- **Phrase-quality regressions** (jixu → 曳光弹 at #1 even though
  继续 has 5× the corpus freq) — pinyin's prefix completion path
  pulls in low-relevance long-prefix matches whose score isn't
  truly compared to the higher-freq exact-match entries.
- **JP-vs-Chinese miscalibration** — JP kanji match for `e`
  (会 reading "e") wins over wubi `e → 有` (一级简码) when JP layer
  is placed above wubi.
- **Coverage gaps** (kaoqian → 靠前 not in dict at all) — data-layer
  problem, but symptom is felt through the ranking layer.
- **User can't tune** — no single dial moves toward "better feel";
  every fix requires a code edit.

Sogou / Microsoft IME / Apple IME all do **unified scoring** with
engine-specific multipliers + corpus-derived frequency + user-
learning weighting. We follow that model — code-first, statistical,
LLM only for boundary cases.

## Reference: Sogou-style techniques (from public papers / blog posts)

- **N-gram language model**: phrase prob `P(中国) = freq(中国) /
  total_phrase_count`, smoothed via add-k or Kneser-Ney. Our wubi /
  pinyin dicts already approximate this via per-entry freq, but
  scales differ per corpus.
- **Cross-corpus normalization**: 微博 / 新闻 / 维基 freq tables
  combined with interpolation weights `α_weibo × P_weibo(w) + α_news
  × P_news(w) + α_wiki × P_wiki(w)`. Inputx currently uses Leipzig +
  jieba; v0.2 should consider adding a modern-web corpus.
- **Length bias**: short phrases (2–3 chars) get a slight boost since
  they're most-common in real typing; very long phrases get penalty
  unless explicitly typed.
- **简拼 (initials-only) handling**: separate score class — initials
  matches always rank below exact matches, but above prefix
  completion. We have a Path 2 for this; needs explicit scoring
  rather than just "added after Path 1".
- **Prefix completion noise**: pinyin's Path 3 (FST prefix scan)
  pulls long-prefix entries that share a prefix but aren't what
  the user typed. Sogou uses a freq cutoff + a "user is mid-syllable"
  vs "user is done" gate. We should explicitly de-rank these.
- **User-learning (L0) weight cap**: Sogou's user dict only floats
  a candidate up by a bounded amount, never strictly to #0, to
  avoid muscle-memory contamination from accidental picks. Our
  L0-pin-to-#0 is too aggressive (the `wcng → 鹟` accidental pin
  observed during JP-plugin landing).

## Unified score formula

```rust
score(c) =
    ENGINE_MULT[c.engine]               // wubi 1.3, pinyin 1.0, jp_kanji 0.85, jp_kana 0.3
  * LAYER_FLOOR[c.layer]                // jianma1: 10x, jianma2: 3x, jianma3: 1.5x, zigen: 2x, phrase: 1x, auto: 0.5x
  * (1.0 + LOG_FREQ(c) / LOG_FREQ_MAX)  // 0..2 from log-normalized corpus freq
  * LENGTH_BIAS(c)                      // 1.05 for 2-char phrase, 1.0 base, 0.9 for 5+ char
  * (1.0 + L0_BOOST(c))                 // capped at +0.5 from user pin / pick counts
```

`ENGINE_MULT` values are user-tunable in `Settings → 高级`. Default
chosen to satisfy "Inputx 五笔 is wubi-first" while letting pinyin
exact matches beat low-conviction JP suggestions.

## Wubi 简码 hard floor

The single non-quantitative rule. When `c.engine == Wubi` AND
`c.layer ∈ { Jianma1, Jianma2, Jianma3 }`, `score(c)` is multiplied
by 1e6 — guaranteeing top-N position for the canonical wubi
shortcuts (`e → 有`, `go → 来`, etc.) regardless of any other
engine's score.

The floor is layered (jianma1 > jianma2 > jianma3) so simcodes
within wubi still order correctly, but they all dominate non-
simcode entries from any source.

## Corpus rebuild pipeline (v0.2 scope)

```
tools/build_unified_scores.py
  ├─ load: jieba word freq, Leipzig corpus stats, Unihan readings
  ├─ for each engine:
  │    compute log-normalized freq per (code, word) pair
  │    bucket into LAYER_FLOOR via existing layer assignments
  ├─ cross-corpus interpolation per engine
  ├─ write: weights_unified.tsv
  └─ regenerate FST/PHF artifacts
```

Reproducible, deterministic. Re-run after corpus updates.

## LLM-assisted edge-case tuning

LLM is **not on the runtime hot path**. Two offline uses:

1. **Ambiguous-case annotation**: given (code, top-10 candidates),
   LLM ranks "most likely user intent" for queries where corpus
   freq is too close to discriminate (e.g., 几许 vs 急需 for jixu).
   Output: a small TSV of (code → preferred-word) overrides applied
   as a final L0-pin-equivalent boost during build.
2. **Validation against polish-log**: parse PolishLog jsonl (mac
   IME captures user's non-#0 picks), LLM groups patterns
   ("repeated `kaoqian → 靠前` misses"), suggests data fixes.

Both phases are batch jobs; humans review the LLM output before it
lands in the build. No LLM call at IME runtime.

## Validation: polish-log driven

Every user pick of a non-#0 candidate writes a jsonl row to
`~/Library/Containers/jp.golia.inputmethod.wubi/Data/Library/Application Support/Inputx/polish-log.jsonl`
(macOS) / App Group container (iOS). Schema:

```json
{ "ts": "2026-05-22T08:15:23.456Z",
  "buffer": "jixu",
  "candidates": ["曳光弹", "继续", "急需", ...],
  "pickedIdx": 1,
  "pickedWord": "继续",
  "engineMode": 0,
  "japaneseEnabled": true }
```

Aggregated polish-log entries become regression tests:
`tests/input_corpus.tsv` rows (`code \t expected_top \t why`) that
the engine must satisfy. Adding a row + fixing the engine to pass
it = one polish-loop cycle.

## Phasing

- **Phase 0 (done)** — layered hard-rule merge: wubi → JP kanji →
  pinyin → JP kana. Wubi 简码 implicitly top via layer_base. JP
  rank-boost reverted. PolishLog telemetry live.
- **Phase 1** — expose per-candidate `score: f64` from each engine.
  `merge()` sorts by score. Wubi 简码 hard floor explicit. No data
  changes yet — re-uses existing weights.
- **Phase 2** — corpus rebuild pipeline. Cross-corpus normalization.
  Modern-web freq table.
- **Phase 3** — LLM-assisted boundary tuning. Validation via
  polish-log corpus.
- **Phase 4** — user-facing ENGINE_MULT sliders in Settings → 高级
  (power users only; defaults stay tuned by us).

## Out of scope

- Runtime LLM calls (latency + privacy + reproducibility).
- N-gram > unigram modeling (jukugo+phrase coverage already gives
  context; bigram modeling is a v0.4 stretch).
- Auto-discovery of new compound words from user typing (the
  manual data pipeline is the source of truth).
