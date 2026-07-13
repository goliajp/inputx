# Garbage-filter false-positive recall sweep — 2026-07-14

User directive 2026-07-13 (triggered by the 窗外 double-kill): "你一个个过,
别搞什么策略,你用自己 llm 的中文常识一个个来复审".

## Scope
All 17,800 corpus_garbage_filter_v1 rows tagged `modern-vocab-audit-p1/p2`
(the 2026-07-10 audit deletions). The 141k `freq=0` rows (zero-corpus
mechanical deletions, different class) were NOT in scope.

## Method
- 40 reviewer agents, 450 rows each, per-row Chinese-lexicality judgment
  (context columns: pre-audit mv freq, v1 lib freq, cedict presence,
  original audit tag — hints, not verdicts). 宁缺毋滥: unsure → KEEP.
- Coverage verified: 17,799 unique rows, 0 missing, 0 extra.
- Agent proposals: 697 RECALL (3.9%).
- Lead per-row review of all 697: 691 accepted, 6 rejected
  (灿若星辰/高热不退/接水盘/年轻态/凝心聚力/小嫚).

## Key finding — the double-kill mechanism
The audit's "lib 兜底真词不回捞" assumption was wrong at the filter level:
a garbage-filter record ALSO (a) drops the v1 library row at
pinyin-build-dict time and (b) suppresses the v2 cedict row via the
runtime exclusion union. 586 of the 691 accepts had live v1 corpus freq —
they were silently un-typeable since 2026-07-10.

## Apply
- 691 filter rows removed (185,786 → 185,095 entries; v1 dropped_garbage
  4,640 → 4,053).
- 689 modern_vocab rows re-added @ 15000 (105 lib=0 + 584 lib>0;
  skipped 立案/激昂 — see below; a few already present).
- Ambiguous-segmentation codes: (lian,立案) and (jiang,激昂) resurfaced
  ABOVE single-char muscle memory once unsuppressed. tier_overlay → 5
  for both; the stale dogfood-era quickfix `lian 立案 50000` (aa6b06e82)
  was removed — it was dormant under the filter and winner-take-all on
  unsuppression.

## Files
- `recalls_all.tsv` — the 697 agent proposals with context.
- `final_decisions.tsv` — per-row lead verdicts (691 ACCEPT / 6 REJECT).
