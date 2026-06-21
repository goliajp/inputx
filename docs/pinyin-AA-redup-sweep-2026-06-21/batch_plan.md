# Batch plan — AA-redup pinyin sweep

## Acceptance rule per batch

For each `(code, word, freq)` candidate:

1. **Keep** if the AA form is an attested real Chinese word in
   modern usage (verb-redup, adj-redup, common reduplicated noun,
   onomatopoeia, kinship term, common nickname). Sources of truth:
   - 现代汉语词典 7th edition entry for the AA form
   - 普通话/口语 attested verb-V-V or adj-AA pattern of the root char
   - Common-knowledge nickname (爸爸/妈妈/姐姐/弟弟/爷爷/奶奶/...)
2. **Delete** if any of:
   - **Name nickname** — the AA form is only attested as someone's
     affectionate name (凡凡/丽丽/帆帆/莎莎). These belong to a
     name dictionary, not the general pinyin candidate pool — they
     dominate top-10 with no general-population utility.
   - **Subword artefact** — the AA form is what you get when jieba
     boundary-cuts a longer phrase (`X X X` → "XX" + "X" leftover);
     the standalone AA isn't used.
   - **Onomatopoeia fragment** — partial of a longer real form
     (哒哒哒/嗒嗒嗒 → keep the triple, drop the double if it's only
     a fragment).
   - **Non-word reduplication** — character × 2 doesn't form any
     attested word (饭饭/犯犯/范范 etc.).
3. **Defer (mark uncertain)** if any doubt. These get audited again
   in batch N+1 rather than guessed.

## Batches

### Batch 1a — freq 10 000-14 999 (E bucket, 502 rows)

Expected delete rate: ~85%. This is the noise-dominant band. Real
words at this freq are rare (most are nickname / sub-word noise).

**Reviewer workflow:**
- Open `candidates.tsv`, filter to rows where `freq` in `[10000, 15000)`.
- For each, decide keep/delete/uncertain per the rule above.
- Emit `batch1a_to_delete.tsv` schema `code\tword\treason`.
- Run `apply_batch.py batch1a_to_delete.tsv` (TBD).

### Batch 1b — freq < 10 000 (F bucket, 278 rows)

Expected delete rate: ~95%. Real words at this freq band of pinyin
buffers are extreme outliers.

### Batch 2 — freq 15 000-19 999 (D bucket, 507 rows)

Expected delete rate: ~50%. Mixed — many real verb/adj reduplications
(`刷刷`/`算算`/`方方`/`稍稍`) sit here, alongside continued nickname
& subword noise.

### Batch 3 — freq 20 000-24 999 (C bucket, 269 rows)

Expected delete rate: ~15%. Mostly real (huihui/会会, guoguo/国国,
weiwei/薇薇... wait, 薇薇 is a name → delete; this band needs care).
Cherry-pick obvious noise; default to keep.

### Out of scope

- Buckets A (>=30k, 73) and B (25-30k, 126) — default keep, only
  hit by explicit user report. No batch audit needed.

## Per-batch deliverables

Each batch run produces:

1. `batchN_to_delete.tsv` — reviewer's keep/delete decision data.
   Schema: `code\tword\treason\t# notes`. `reason` is one of `name`
   / `noise` / `onomatopoeia_partial` / `subword_artefact`.
2. `batchN_uncertain.tsv` — deferred rows requiring research.
3. `apply_batch.py` invocation log appended to this doc.
4. **One commit per batch** with message
   `polish(D1:pinyin): AA-redup sweep batchN — delete K rows`
   following the same style as `35fd1fa` (Phase A).

## Pin tests after each batch

For every deleted (code, word), if some real word in the same buffer
was being out-ranked, add a one-line pin to
`composite/comprehensive_baseline.rs::pinyin_only_multi_syllable`
`("code", "real_word_now_top")` so future regressions are caught.

Examples (anticipated):
- `mama` → `妈妈` (kept; needs pin if any noise removed)
- `papa` → `啪啪` or `爸爸` (audit which one user wants leading)
- `xixi` → `嘻嘻` (kept; 曦曦/淅淅/细细 etc audited)

## Why a sweep and not 100 separate polishes

The user complaint is systemic ("叠词的问题还是很严重"), and the
root cause is a single legacy ingest event with the same noise
profile across 1755 entries. Per-buffer polishing would compound
into 100+ commits with no shared rationale — a sweep with one
audit doc + one commit per batch keeps the trail coherent.

The polyphone-dup sweep at `../pinyin-polyphone-dup-sweep-2026-06-13/`
is the precedent: 3 batches over a week resolved >3700 deletes with
full audit traceability.
