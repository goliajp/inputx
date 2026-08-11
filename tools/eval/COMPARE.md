# MIU accuracy — inputx vs libpinyin (CP-0.6 external reference line)

Phase-0 external comparison. Both engines were driven over the **same**
gold/silver eval sets with the **same** metric so the numbers are directly
comparable:

> **Top-K MIU accuracy** = fraction of rows whose gold hanzi string is an
> exact full-string match within the engine's first K sentence candidates.
> Input is continuous pinyin (the TSV's syllable-separating spaces are
> stripped — a real user types no spaces). Exact match, **no traditional→
> simplified normalization**, so the ~16% traditional-char gold rows are a
> miss for both engines (same handicap).

Date: 2026-06-15 · 51,000 rows (1,000 gold + 50,000 silver) · 48,340 of
them polyphone-flagged.

| engine | overall Top-1 | gold Top-1 | silver Top-1 | polyphone Top-1 |
|---|---|---|---|---|
| **inputx** (v1.4.0, gates OFF) | **0.73%** | **1.70%** | **0.71%** | **0.55%** |
| **libpinyin** (2.10.3) | **32.02%** | **32.00%** | **32.02%** | **31.63%** |
| gap (× ahead) | ~44× | ~19× | ~45× | ~57× |

Top-5 / Top-10 add essentially nothing for either engine and are omitted
from the headline table:

| engine | overall T1 / T5 / T10 |
|---|---|
| inputx | 0.73% / 0.74% / 0.74% |
| libpinyin | 32.02% / 32.02% / 32.02% |

- **inputx** exposes no n-best sentence list, so T5/T10 ≈ T1 (a row hits
  only when the whole buffer is a single dict phrase landing at rank 0).
- **libpinyin**'s stable simple API exposes only the 1-best sentence
  (`pinyin_get_sentence` asserts on an out-of-range nbest index and there
  is no public count getter), so this harness measures Top-1 and reports
  T5/T10 equal to it. Top-1 MIU is the canonical comparison number anyway.

## Reading the gap

libpinyin leading by ~44× is the **expected, healthy** result and the
acceptance signal for CP-0.6: it confirms the evaluation harness is sound
(a real mainstream engine scores an order of magnitude higher; if the two
had been close, the harness would have had a bug and we'd reopen CP-0.5).

The inputx number is **not** the engine's ceiling — it is the deliberate
pre-Phase-1 **floor**. As of v1.4.0 the engine runs in minimal-debug state
with the COMPOSE / ASSOCIATION / FUZZY / PREDICTION pipeline gates
hard-disabled as compile-time consts
(`core/crates/inputx-core/src/composite/pinyin_adapter.rs:75-78`). With
COMPOSE off, the long-buffer Viterbi sentence path (Path 0b) never fires,
so multi-syllable sentence rows almost always miss. Flipping the COMPOSE
gate is exactly Phase-1 work ("翻 gate") and is what should close most of
this gap.

libpinyin's ~32% is itself dragged down from its true capability by the
no-trad-normalization rule and the mixed-source silver set (pypinyin
auto-labels, un-audited); on a clean simplified People's-Daily-style set
a trigram engine like libpinyin sits nearer the ~50% Top-1 MIU cited in
the quality-gap analysis. So ~32% is a conservative, fair reference bar —
the climb target is to approach it, then pass it.

## Reproduce

```sh
# inputx (in-tree Rust runner) — run from core/
cargo run -p inputx-eval-runner --release            # writes results/<date>.json
cat tools/eval/results/baseline.json

# libpinyin external reference (needs `brew install libpinyin`)
python3 tools/eval/runner_libpinyin.py               # writes baselines/libpinyin.json
```

Artifacts: `tools/eval/results/baseline.json` (inputx, tracked) ·
`tools/eval/baselines/libpinyin.json` (libpinyin, tracked).
