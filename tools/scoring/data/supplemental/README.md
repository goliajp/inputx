# `tools/scoring/data/supplemental/` — work-in-progress data drops

This directory holds the *intermediate* TSVs produced by claude-CLI-driven
data-generation rounds, before the entries are folded into the canonical
data files in the relevant Rust crates.

**Source of truth is NOT here.** This dir is for review + reproducibility
of how the canonical data got built up over time. The committed FST
(`core/crates/inputx-pinyin/data/pinyin.fst`) and the source data files in
each crate's `data/` directory are what actually ship.

## Files

| File | Generated | Status |
|---|---|---|
| `phrases_v1.tsv` / `phrases_v1_clean.tsv` | round-A: tech/modern phrases | folded into weights.tsv |
| `words_v1.tsv` / `phrases_v2.tsv` / `phrases_v2_final.tsv` | round-B: internet phrases | folded into weights.tsv |
| `words_v3.tsv` / `phrases_v3_clean.tsv` | round-B-2: everyday spoken | folded into weights.tsv |
| `words_v4.tsv` / `phrases_v4_clean.tsv` | round-C: tech/internet/work | folded into weights.tsv |
| `idioms_v1.tsv` | round-D: 4-char idioms | folded into weights.tsv |
| `casual_v1.tsv` | round-E: casual/polite/slang | folded into weights.tsv |
| `heteronyms_v1.tsv` (malformed) | round-F draft | superseded by v2 |
| `heteronyms_v2.tsv` | round-F: heteronym readings | **migrated to** `core/crates/inputx-pinyin/data/heteronyms_curated.tsv` |

## Workflow per round

```
1. claude CLI prompts for (word, freq) lines per category
2. pypinyin computes the full reading per word
3. dedup against existing weights.tsv
4. result: <pinyin>\t<word>\t<freq> additions
5. append to core/crates/inputx-pinyin/data/weights/weights.tsv
6. rebuild pinyin.fst via `cargo run --features tools --release --bin pinyin-build-fst`
```

## Migration into crate data

The `phrases_*` rounds are **freq overrides + new entries** — they live in
`weights.tsv` because that's where (pinyin, word, freq) tuples belong.

The `heteronyms_*` round is structurally different — it tells the build
pipeline which *canonical reading* a phrase has, suppressing the cartesian
fallback's wrong-reading variants. That data belongs in
`core/crates/inputx-pinyin/data/heteronyms_curated.tsv` (format:
`phrase<TAB>canonical_pinyin`). The `heteronyms_v2.tsv` content has been
migrated there as of 2026-05-22; the source TSV is kept for review history.

## Polish-log driven supplemental

`data/polish_reports/` (one level up) contains aggregator outputs from
`tools/scoring/07_validate/aggregate_polish_log.py` — reading the user's
PolishLog jsonl, surfacing repeat misses, emitting `quickfix_boost.tsv`
patches ready for review + manual application.

## Future

Once the v0.2 corpus rebuild pipeline (SCORING.md §1.2 stages 02-08)
is operational, all of these supplemental rounds become an audit trail
rather than a live data source — the pipeline will produce weights.tsv
from primary sources (Wikipedia / jieba / Leipzig / heteronyms_curated)
deterministically.
