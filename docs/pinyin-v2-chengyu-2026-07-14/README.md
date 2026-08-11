# v2 chengyu backfill sweep — 2026-07-14

User directive: "做 v2 backfill sweep 补成语" + "你来判断如何做,但必须,
你用自己 llm 专业知识一个个审查才能入库".

## Why
v2's word list is a CC-CEDICT + HSK ingest; it never absorbed the v1
corpus vocabulary. Six separate polish reports (黑屏 / 武僧 / 躲过去 /
墨宝 / 盖住 / 花草树木) traced to the same shape: word present in v1
library.tsv, absent from v2 → the buffer returns nothing (or composed
junk). Idioms are the densest cluster of that gap.

## Scope — no threshold, nothing skipped
All 36,674 four-hanzi v1 library words absent from v2 words.tsv and
modern_vocab, and not already recorded in corpus_garbage_filter.
freq range 1,493 – 38,294. NO freq cutoff was applied: per the user
directive the candidate pool was reviewed in full.

## Method — per-row LLM review, twice
1. 82 reviewer agents × 450 rows. Verdict per row: RECALL (a real
   idiom / fixed four-character expression AND the code is its correct
   full pinyin) or KEEP (syntactic collocation, news/policy phrase,
   proper noun, substitutable greeting, onomatopoeia, truncated
   half-idiom, wrong pinyin). 宁缺毋滥: unsure → KEEP.
   - A server-side rate limit killed ~half the agents mid-write; the
     redo pass only re-judged the rows a batch had not yet written, so
     no row was judged twice by accident and none was skipped.
   - Coverage verified: 36,674 / 36,674, zero missing.
2. Lead (me) reviewed all 4,697 RECALL proposals row by row.
   4,668 accepted, 29 rejected — every rejection a hard defect:
   - misspellings: 大作文章 / 惹事生非 / 默默无名 / 小题大作 /
     莫明其妙 / 普渡众生 / 公诸于众 / 有缘无份 / 丢三拉四 /
     消声匿迹 / 墨守陈规 / 扑天盖地 / 卓而不群 / 百堕俱举
   - non-standard variant glyphs: 一见锺情 (锺), 夸大其辞, 一面之辞,
     名符其实, 不尽人意, 铤鹿走险, 入不敷支, 奇离古怪
   - bad pinyin codes: 青灯古佛 (gufu→gufo), 借花献佛 (xianfu→xianfo),
     刚直不阿 ("bua" is not a legal syllable)
   - duplicate/non-standard collisions on a shared code: 飘洋过海,
     上窜下跳, 六根清静, 虚无缥渺

## Apply
- modern_vocab_v1.tsv: +4,668 rows @ 15000 (tier 4 — idioms surface at
  their own long buffers and never crowd short common ones).
- polish-rebuild green; the tip-scale single-char pins held (the
  2026-07-10 vocab-audit lesson: bulk modern_vocab edits can flip
  single-char priors — they did not this time).
- Probe: 叶公好龙 / 庖丁解牛 / 程门立雪 / 班门弄斧 / 无所畏惧 all #0.

## Deliberately NOT done
The 29 rejected misspellings still live in v1 library.tsv, so they
remain typeable — declining to backfill them into v2 does not remove
them. Purging misspelling variants from the v1 main dict belongs to
the library-audit project (separate backlog item), not to this sweep.

## Files
- `recalls_all.tsv` — the 4,697 agent proposals with freq + reason.
- `final_decisions.tsv` — per-row lead verdicts (4,668 ACCEPT / 29 REJECT).
