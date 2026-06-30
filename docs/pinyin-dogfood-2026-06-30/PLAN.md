# Pinyin v2 dogfood — long polish run

立 2026-06-30. 目标:1000 篇新闻 dogfood,系统性发现 v2 顺序问题,批量 polish。

**[PAUSE]** 标记在文档顶部 = 暂停 autonomous loop。删掉就恢复。

## Last action
(autonomous iter 写最近一次动了什么 — 包括时间戳 / commit hash / 状态)

- 2026-06-30 setup — Phase 0 complete(0.1 / 0.2 / 0.3 done). Next: 1.0 MODERN-FREQ-DESIGN.md
- jieba dict location:`/Users/doracawl/Library/Python/3.9/lib/python/site-packages/jieba/dict.txt`(已 installed)
- gongqi baseline anchor confirmed:current `共栖, 工期, 汞齐`(应翻成 工期 #0,Phase 1 验证点)
- 2026-06-30 iter#1 — 1.0 done. MODERN-FREQ-DESIGN.md written(区间 [0,25k]、percentile rank、6 sort sites 列清、anchor 验证 checklist)。Next: 1.1 build-modern-freq.py

## Status legend
- `[ ]` todo
- `[x]` done
- `[WIP]` in progress (本 iter 跑中,下 iter 别动)
- `[BLOCKED: reason]` 需要 user 介入

## Autonomous protocol summary (full version in PROTOCOL.md)

每个 `/loop` fire:
1. Read this file
2. 看顶部有没有 `[PAUSE]` → 有则只 report 状态 + exit
3. 找**第一个** `[ ]`(跳过 `[WIP]` `[BLOCKED]` `[x]`)
4. mark `[WIP]`(commit pulse 1)
5. 执行
6. mark `[x]` + 写 Last action
7. commit pulse 2
8. exit(让 /loop 调度下一 fire)

## Master checklist

### Phase 0: setup (本文件 + 协议)
- [x] 0.1 Create docs/pinyin-dogfood-2026-06-30/ dir
- [x] 0.2 Write PROTOCOL.md(autonomous execution rules)
- [x] 0.3 Create scratchpad subdirs(corpus/, segments/, failures/, reports/)

### Phase 1: modern-freq scaling(评分 tiebreaker,不入库)
设计文档:`docs/pinyin-dogfood-2026-06-30/MODERN-FREQ-DESIGN.md`(待 1.0 写)

- [x] 1.0 Write MODERN-FREQ-DESIGN.md(refresh 之前讨论的 scaling 设计)
- [ ] 1.1 Write `tools/v2-ingest/build-modern-freq.py`(read jieba dict + v2 words/chars → modern_freq.tsv)
- [ ] 1.2 Run script → 生成 `core/crates/inputx-pinyin-v2/data/modern_freq.tsv`
- [ ] 1.3 `data.rs` 加 `modern_freq()` lazy loader(HashMap<String, u16>)
- [ ] 1.4 `lib.rs` 各 path 排序 key 加 modern_freq DESC 二维(6 个 site:exact / char / initials / prefix / compose / quickfix)
- [ ] 1.5 cargo build + baseline 357/0 ✓
- [ ] 1.6 验证 anchor 翻转:gongqi→工期 / ceshi→测试 / liucheng→流程 / yanjiu→研究 / huluobo→胡萝卜(无 quickfix 帮助时也能自动正确)
- [ ] 1.7 mac/reinstall.py + commit + push

### Phase 2: corpus prep
- [ ] 2.1 Pick corpus source(THUCNews 默认,或 HuggingFace 备选)
- [ ] 2.2 Download corpus archive 到 scratchpad
- [ ] 2.3 Sample 1000 articles balanced(每类 ~70 篇,14 类)
- [ ] 2.4 Save to `scratchpad/corpus/articles.txt`(每行 1 篇,或多文件)
- [ ] 2.5 Write `tools/v2-ingest/segment-corpus.py`:jieba 分词 → IME-realistic 2-4 字 buffer + pypinyin code
- [ ] 2.6 Run segment script → `scratchpad/segments/segments.tsv`(`article_id\tseg_idx\tword\tpinyin\t<HSK_level if known>`)
- [ ] 2.7 Validate segments(spot check 100 行,看分词 / 拼音对不对)

### Phase 3: dogfood Rust binary
- [ ] 3.1 Write `core/crates/inputx-core/src/bin/inputx_dogfood.rs`(read segments.tsv,for each call v2::query() in-process)
- [ ] 3.2 Failure classification logic(HARD = not top10, SOFT = not #0)
- [ ] 3.3 Cargo wire(确保 binary 编译)
- [ ] 3.4 Smoke run on first 100 segments → 验证 pipeline

### Phase 4: full dogfood run
- [ ] 4.1 Run dogfood on 50 articles → `scratchpad/failures/run-001-smoke.tsv`
- [ ] 4.2 Bug-fix pipeline if smoke shows issues
- [ ] 4.3 Run on full 1000 articles → `scratchpad/failures/run-001-full.tsv`
- [ ] 4.4 Aggregate stats:total / HARD / SOFT / top failure patterns
- [ ] 4.5 Write `reports/run-001-analysis.md`(分类 + Top-N systemic patterns)

### Phase 5: batch polish iterations
每 iter:从 failures 选 top N → 自动生成 polish patch → 应用 → 验证 improvement → commit
- [ ] 5.1 Iter A:HARD failures(缺词)batch — 写脚本扫 modern_vocab,生成 patch 候选,review,apply,re-dogfood 该子集
- [ ] 5.2 Iter B:SOFT failures(顺序错)batch 1 — 高 jieba freq 但没胜的词,生成 quickfix patch
- [ ] 5.3 Iter C:SOFT failures batch 2(剩余)
- [ ] 5.4 Iter D:系统模式(发现 framework gap)— 列 + escalate user
- [ ] 5.5(可选 reps)Iter E-Z:继续直到 diminishing return

### Phase 6: 收尾
- [ ] 6.1 Retire obsolete quickfix rows(被 modern-freq 自动覆盖的)
- [ ] 6.2 Write final `reports/final.md`(改进数据 + 仍存在的 framework gaps)
- [ ] 6.3 Update memory with key findings(`memory/dogfood_2026-06-30_findings.md`)
- [ ] 6.4 Tag commit(可选)

## Open questions(任何一项有 update 写这里)

> _(empty)_

## Notes / 系统观察

> _(empty,5.x 阶段把发现写这)_
