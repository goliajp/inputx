# AA-redup sweep — pinyin (2026-06-21)

Systematic clean-up of AA single-character reduplication noise in
`core/crates/inputx-pinyin/data/library.tsv`. Sibling protocol to
[`../pinyin-polyphone-dup-sweep-2026-06-13/`](../pinyin-polyphone-dup-sweep-2026-06-13/).

## Background

User report 2026-06-21 (verbatim):
> fanfan 感觉现在叠词的问题还是很严重，有相当多根本不是个词，
> 日常也用不了

The `fanfan` segment showed 8/10 candidates that are not real Chinese
words (饭饭/反反/烦烦/范范/帆帆/犯犯/繁繁/翻翻). Phase A
([commit `35fd1fa`](https://github.com/goliajp/inputx)) deleted 10
fanfan-segment rows; this Phase B doc generalises the cleanup across
the remaining 1755 AA reduplications in `library.tsv`.

## Root cause

All AA-redup entries in `library.tsv` are tagged `source=digested`,
contributed by the single legacy ingest event
`pinyin-legacy-2026-06-03` (see `tools/scoring/data/digest_log.toml`).
That event dumped 397 493 pre-治理 rows from a non-deterministic
`pinyin-build-weights` chain consuming jieba phrase tables,
pypinyin lists, and other upstream corpora. The legacy chain
admitted three categories of AA noise without filtering:

1. **Jieba reduplication artefacts** — boundary-cut sub-words where
   the prior character's last token + the following character's first
   token happen to be the same hanzi (e.g. `…X X…` → "XX" extracted).
2. **Name nicknames** — Chinese affectionate name patterns
   (`凡凡`, `帆帆`, `莎莎`, `丽丽`) cluttering common-word buffers.
3. **Onomatopoeia byproducts** — half-words split from longer
   onomatopoeic forms (e.g. `哒哒哒` segmented to `哒哒` + `哒`).

The legitimate AA reduplications (妈妈/谢谢/看看/想想/慢慢...) are
all in the freq >= 25k band — the noise concentrates below.

## Scope

| Bucket | Range | Count | Verdict |
|---|---|---|---|
| A | freq >= 30 000 | 73 | Keep all (high-confidence real words) |
| B | freq 25 000-29 999 | 126 | Default-keep, spot-check sample |
| C | freq 20 000-24 999 | 269 | **Audit batch 3** |
| D | freq 15 000-19 999 | 507 | **Audit batch 2** |
| E | freq 10 000-14 999 | 502 | **Audit batch 1a** (high-delete-rate) |
| F | freq < 10 000 | 278 | **Audit batch 1b** (near-total delete) |
| total | | 1755 | |

**Batch 1 (E + F = 780 rows):** deepest noise; expected ~90% delete rate.
Run first as the highest-yield clean-up. Bucket E in particular maps
1:1 to the `fanfan` segment's profile (饭饭=21k was C, 翻翻=24k was C,
all freq-22k or below was where Phase A drew the line).

**Batch 2 (D = 507 rows):** mixed; expected ~50% delete rate. Many real
verb-redup (`刷刷`/`算算`) and adj-redup (`细细`/`方方`) hide here.

**Batch 3 (C = 269 rows):** mostly real (mama-tier reduplications
naturally fall here when the underlying char is mid-frequency); expected
~15% delete rate. Cherry-pick obvious noise only.

## Files

- `find_aa_redups.py` — scan `library.tsv`, emit `candidates.tsv`
  sorted by freq desc with a heuristic `class_hint` column. Run again
  whenever `library.tsv` changes.
- `candidates.tsv` — 1755 rows, `code\tword\tfreq\tsource\tclass_hint`.
  Authoritative input for the batch reviewer.
- `batch_plan.md` — bucket boundaries + per-batch acceptance rules +
  whitelist-recapture mechanism (see "Whitelist recapture" below).
- `batch1_to_delete.tsv` (TBD) — reviewer-curated subset of batches 1a+1b
  for deletion. Schema `code\tword\treason` (one of:
  `name` / `noise` / `onomatopoeia_partial` / `subword_artefact`).
- `apply_batch.py` (TBD) — applies a `batchN_to_delete.tsv`:
  removes rows from `library.tsv`, appends to
  `tools/scoring/data/polish/corpus_garbage_filter_v1.tsv`,
  runs `make polish-rebuild`, prints diff summary.

## Whitelist recapture

If a deletion turns out to remove a word the user actually wants
(e.g. user types `fanfan` intending to write the verb-reduplication
`翻翻 [书]` standalone), the recovery path is **not** to revert the
sweep, but to add the word back as a `polish`-tagged row in
`library.tsv` with a curated freq (~peer of the legitimate AA words
in the same buffer band). Reference: how `daizhe` recovered `带着`
in commit `9a3...` style.

Whitelist additions go into `core/crates/inputx-pinyin/data/library.tsv`
with `source=polish` (not `source=digested`), so the next regen of
`pinyin-build-weights` won't churn them.

## Phase A summary (already shipped)

| Buffer | Deleted | Kept |
|---|---|---|
| `fanfan` | 10 rows (凡凡/反反/帆帆/烦烦/犯犯/番番/繁繁/范范/饭饭/翻翻) | 翻番 (double), 泛泛 (superficial) |

Commit: `35fd1fa`. Pin in
`composite/comprehensive_baseline.rs::pinyin_only_multi_syllable`:
`("fanfan", "翻番")`.

## Phase B status

- 2026-06-21 — doc framework + candidates.tsv generated. **Awaiting
  user audit of `batch1_to_delete.tsv` (TBD) before running
  `apply_batch.py`.**

No `library.tsv` writes in this phase commit. User pulls the trigger
on each batch.
