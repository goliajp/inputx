# modern_vocab_v1 full audit — 2026-07-10 (宁缺毋滥)

User trigger: shej candidates polluted by dogfood article-segment fragments
(涉及多个政府部门 / 涉及本部门的政务数据校核申请 at flat freq 30000).
Directive: "先处理掉当前的,然后再 audit 所有 2 字以上的词库,宁缺毋滥".

## Root cause

Dogfood strict-segment ingest (A147 era, 2026-06/07) added every non-PASS
article segment to `modern_vocab_v1.tsv` at flat freq 30000 with no
lexicality gate — 24,924 of the file's 26,761 rows were flat-30000 pump.
Clause slices, doc titles, poem lines, news person names, local orgs,
jieba fragments and outright bad-pinyin rows all entered as "vocab".

## Method

Per-row audit (no whitelist heuristic, per 2026-06-22 standing rule),
51 parallel auditor passes + lead review of every keep + fixture-pin
cross-check + curated-freq (≠30000/300) delete re-review.

- Phase 1 — ≥5 hanzi: 1,611 rows → 1,454 deleted, 9 fix-pinyin, 157 keep
  (commit a2b82cb8). Rubric: `rubric-p1-ge5.md`.
- Phase 2 — 2/3/4 hanzi: 24,743 rows → 16,349 deleted (+2 corrupt idiom
  rows replaced with correct pinyin). Rubric: `rubric-p2-2to4.md`.
  - 3-char: 14,721 rows → 9,964 deleted
  - 2-char: 7,062 rows → 4,540 deleted
  - 4-char: 2,960 rows → 1,845 deleted

File: 26,761 → 8,941 lines. Every delete logged in
`corpus_garbage_filter_v1.tsv` (tags `modern-vocab-audit-p1-ge5` /
`modern-vocab-audit-p2`) so future digests can't re-admit.

## Lead-review restore policy (restore-*.tsv, 85 rows)

Agent hard rules over-fired on three protected classes; restored:

1. **Curated colloquial units** (pre-dogfood, freq 45-60k, mostly
   libflag=-): 搬过来/饿死了/知道了/好极了/不知道/急死我了/一起加油…
   These were deliberate IME typing-unit adds; deleting them removes
   the word entirely.
2. **User-polish / sweep-pinned rows** (baseline fixtures pin them):
   忘了/盖在/思思/揍他/带着/一系/盛汤/天都/巨累/大儿媳/削苹果/德黑兰….
   Lesson: two rows (盖在/巨累) were initially missed because the pin
   cross-check matched pinyin-only or a different assert shape — the
   baseline gate caught both. Fixture pins are the real safety net.
3. **俗读 tolerance idiom routes** (07-09 batch, pinned): yimoyiyang
   一模一样 / renshengchaolu 人生朝露 / laodiaozhongdan 老调重弹 etc. —
   these encode how typers actually type, keep. Two rows were corrupt
   beyond tolerance (yiruquanwang 一如既往 / zilishengsheng 自力更生,
   no typer produces those) → replaced with correct-pinyin rows and
   fixture pins updated.

NOT restored (deliberate): flat-30000 boosts of real-but-lib-backed
words (特鲁多/李克强/浙江省/第一个/致力于…) — the word survives in
library.tsv, only the pump is removed. Bad-pinyin curated geo rows
(ailan 爱尔兰 / katar 卡塔尔 / mengjiaguo 孟加拉国 / muniehei 慕尼黑…)
deleted without replacement — library has correct-pinyin entries.

## Ranking side-effects

Deleting flat-30000 mass shifts single-char prior aggregation
(modern_vocab entries aggregate char freq). Phase 1 tip-scaled de(的<得)
and mu(目<木) — fixed via prior_corrections_v1.tsv (的 +6, 目 +2 Q4),
guarded by pre-existing baseline pins. Phase 2 caused no further flips.
