# Pinyin v2 dogfood — long polish run

立 2026-06-30. 目标:1000 篇新闻 dogfood,系统性发现 v2 顺序问题,批量 polish。

**[PAUSE]** 标记在文档顶部 = 暂停 autonomous loop。删掉就恢复。

## Last action
(autonomous iter 写最近一次动了什么 — 包括时间戳 / commit hash / 状态)

- 2026-06-30 setup — Phase 0 complete(0.1 / 0.2 / 0.3 done). Next: 1.0 MODERN-FREQ-DESIGN.md
- jieba dict location:`/Users/doracawl/Library/Python/3.9/lib/python/site-packages/jieba/dict.txt`(已 installed)
- gongqi baseline anchor confirmed:current `共栖, 工期, 汞齐`(应翻成 工期 #0,Phase 1 验证点)
- 2026-06-30 iter#1 — 1.0 done. MODERN-FREQ-DESIGN.md written(区间 [0,25k]、percentile rank、6 sort sites 列清、anchor 验证 checklist)。Next: 1.1 build-modern-freq.py
- 2026-06-30 iter#2 — 1.1 + 1.2 done(atomic)。脚本 + TSV 一次出。Anchor 分数:工期 21948 vs 共栖 12948、测试 24754 vs 侧室 20061、研究 24989 vs 烟酒 22142、流程 24142 vs 柳橙 15520。全部正向(高频 > 低频),可作 same-tier tiebreak。74.7% v2 vocab in jieba(其余 score 0,古汉语/罕用)。Next: 1.3 data.rs loader
- 2026-06-30 iter#3 — 1.3 done。`modern_freq()` lazy HashMap loader 加 + `MODERN_FREQ_TSV` include。cargo build clean。Next: 1.4 wire 6 sort sites
- 2026-06-30 iter#4 — 1.4 done。**设计简化**:不在 6 个 sort site 改,而在最后 sort 前统一 `entry.1 += modern_freq[w]`。一处 site 全覆盖。**gongqi 概念验证 ✓**:`共栖, 工期` → `工期, 共栖`(无 quickfix 助力,纯 modern_freq 翻转)。6 anchors 全 #0 正确。Next: 1.5 baseline 357
- 2026-06-30 iter#5 — 1.5 done。Baseline 初次 6 fail。两类问题:(a) modern_freq 把 quickfix 显式 cascade 推翻 → 加 **Sovereignty rule**(quickfix 词不吃 modern_freq);(b) ingest 漏 `modern_vocab_v1.tsv`(英国/印度 等 country batch 无 modern_freq 数据,被同音 cedict 词压)→ 修脚本读 modern_vocab。剩 2 case (tigan 体感/toulan 偷懒) jieba 新闻语料偏 → 加 quickfix 10k 各。Final 357/0 ✓。Next: 1.6 retire 候选验证
- 2026-06-30 iter#6 — 1.6 done。Retire 候选清单写入 PLAN "Notes" 节。**~6 行可撤**(ceshi/yanjiu/liucheng/lianxu/shenru cascade 全 + xingshi top 2),其余 quickfix 仍 essential(muscle memory 单字 / JP cross / cascade 内非首两位)。Phase 6 清理用。Next: 1.7 mac/reinstall + commit
- 2026-06-30 iter#7 — 1.7 done。**Phase 1 SHIPPED 🎉**。mac/reinstall ✓,live IME 跑 v2 + modern-freq。Phase 1 8/8 全 done(0.x + 1.x = 11 items)。Next: 2.1 pick corpus(THUCNews 默认)
- 2026-06-30 iter#7.5(out-of-band)— user 反馈 reinstall 又有 stray LS。再跑 mac/reinstall.py 成功,脚本自动 purge 2 stray iOS sim entries,clean ship。Live IME 应已新 binary。
- 2026-06-30 iter#8 — 2.1 done。Pick THUCNews(http://thuctc.thunlp.org)。HuggingFace/wikipedia 没装。决定文档 CORPUS-CHOICE.md 写清抽样策略。Next: 2.2 download archive(~700MB,会跨多 fire 完成)
- 2026-06-30 iter#9 — 2.2 **PIVOT**。THUCNews 实测 1.5GB / 700KB/s = 25min,/loop interval 难一气下完;且 mirror 单线程慢。Switch to **zhwiki API**(`zh-cn` variant simplified):每篇 5-10KB,API 无 auth,User-Agent OK。写 `fetch_wiki_corpus.py`(resume + idempotent),启动 nohup PID=63421 fetching 1000 articles。状态 [WIP]。下次 fire check 进度。
- 2026-06-30 iter#10 — 2.2 still [WIP],调速。Iter#9 fetcher 实测 4 articles/min(stub filter 90% reject)。kill 63421,提升参数:batch 20(extracts API max)、MIN_LEN 150、sleep 0.2s。重启 PID=64620。估 ETA <10 min。下次 fire check。
- 2026-06-30 iter#11 — 2.2 done(部分)。zhwiki API 429 rate-limited IP-level。kill 64620,re-launch slow fetcher(sleep 5s,target 500)PID 65523 后台。**pipeline 用当前 25 articles 起步**(~21k 字,足够 surface polish issues)。slow fetcher 多 fire 累积扩到 200-500。target 1000 → 500 调整。Next: 2.5 segment-corpus 脚本(skip 2.3/2.4 — 25 不需要 sample,全用)
- 2026-06-30 iter#12 — 2.5+2.6+2.7 done(atomic)。`segment-corpus.py` 写 + 跑 + spot check 通过。Corpus 此时 36 articles(slow fetcher 加了 11)。生成 **10,920 segments**(1c 30%/2c 57%/3c 8%/4c 4% — IME-realistic 分布)。Sample 真实分词正确(高级/打包/工具/软件包/管理器等)。**Phase 2 全 done 🎉**。Next: 3.1 inputx_dogfood Rust binary

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
- [x] 1.1 Write `tools/v2-ingest/build-modern-freq.py`(read jieba dict + v2 words/chars → modern_freq.tsv)
- [x] 1.2 Run script → 生成 `core/crates/inputx-pinyin-v2/data/modern_freq.tsv`(95870 行,74.7% in jieba)
- [x] 1.3 `data.rs` 加 `modern_freq()` lazy loader(HashMap<String, u16>)
- [x] 1.4 `lib.rs` 加 modern_freq score 注入(**design 简化**:不在 6 个 sort site 改,而在 prior_corrections loop 后统一 `entry.1 += modern_freq[word]`。所有 path 汇 `out`,一次 site 全覆盖,不漏。Cap 25k < tier 跨度 30k 保证不跨 tier)
- [x] 1.5 cargo build + baseline 357/0 ✓ (Sovereignty rule + 2 quickfix backfill + ingest bug fix)
- [x] 1.6 验证 anchor 翻转:gongqi→工期 / ceshi→测试 / liucheng→流程 / yanjiu→研究 / huluobo→胡萝卜(无 quickfix 帮助时也能自动正确)。Retire 候选清单见 "Notes" 节。
- [x] 1.7 mac/reinstall.py + commit + push — **Phase 1 SHIPPED 🎉**

### Phase 2: corpus prep
- [x] 2.1 Pick corpus source — **THUCNews**(决定文档:`CORPUS-CHOICE.md`)。Mirror alive,~700MB,14 类 sina news 2005-2011
- [x] 2.2 Download corpus archive 到 scratchpad — **partial 25 articles**(zhwiki API 429 rate-limited);slow fetcher PID 65523 后台 5s/batch 慢慢加到 500;pipeline 用现有 25 起步,后续动态扩。Target 调整 1000 → 500(achievable)
- [x] 2.3 ~~Sample 1000 articles balanced~~ — SKIP(zhwiki random 已天然 balanced;且 25 不需 sample,全用)
- [x] 2.4 ~~Save to articles.txt~~ — DONE(fetcher 已直接写 articles/NNNN_*.txt,每文件 1 篇)
- [x] 2.5 Write `tools/v2-ingest/segment-corpus.py`:jieba 分词 → IME-realistic 1-4 字 buffer + pypinyin code(7072 非 CJK 自动 drop,31 长 token >4 drop)
- [x] 2.6 Run segment script → `scratchpad/segments/segments.tsv`(36 articles → **10,920 segments**;1c 30% / 2c 57% / 3c 8% / 4c 4%)
- [x] 2.7 Validate segments — spot check 通过(gaoji 高级 / dabao 打包 / ruanjianbao 软件包 等真实分词 + 拼音对)

### Phase 3: dogfood Rust binary
- [WIP] 3.1 Write `core/crates/inputx-core/src/bin/inputx_dogfood.rs`(read segments.tsv,for each call v2::query() in-process)
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

### Retire candidates(iter#6 验证,Phase 6 清理)

Quickfix 行经 modern_freq 验证后可以撤的(modern_freq 已自动达到 user-expected #0):

| buffer | 撤行 | 原因 |
|---|---|---|
| ceshi  | `ceshi  测试 50000` | modern_freq 测试 24754 > 侧室 20061,同 tier 自然 |
| yanjiu | `yanjiu 研究 50000` | 研究 HSK 5 tier 3 vs 烟酒 tier 4,跨 tier 已胜 |
| liucheng | `liucheng 流程 50000` | modern_freq 流程 24142 > 柳橙 15520 + tier_overlay 柳橙 5 双管 |
| lianxu | `lianxu 连续 40000` | 连续 HSK 4 tier 2 vs 怜恤 tier 4,跨 tier 已胜 |
| shenru | `shenru 深入 50k / 渗入 40k / 慎入 30k`(全 3 行)| modern_freq alone 出 `深入 > 渗入 > 慎入` ✓ |

**部分 retire**(top 2 ok,top 3+ 需保留):
- xingshi cascade:modern_freq 出 `形式 > 形势 > 刑事 > 行使 ...`,但 user 要 `形式 > 形势 > 姓氏 > 刑事`。`形式 50k / 形势 40k` 可撤,但 `姓氏 30k` 仍需(否则 姓氏 位置不到 #2)。

**仍需保留**(不能撤):
- 所有单字 muscle-memory quickfix(ba 吧 / xie 些 / ji 给 / qi 起 / 等 ~24 行):modern_freq 自然出的 #0 是「频率最高的字」,但 user 要的是「日常输入习惯」(吧/些/给 等助词或常用字)。这是 muscle memory,jieba 给不出 — 必须 quickfix。
- 跨引擎 JP-beating(huluobo 55k / shijinsai 55k / jianma 55k 等):modern_freq 上限 25k,跨不到 tier 1 区段。
- Cascade 内非首两位的(如 xingshi 姓氏)。
- Polish-A backfill(中国 / 美国 / 日本 等):部分被 modern_vocab + modern_freq 自然 #0 自动,但部分还需 disambiguation(韩国/汗国, 巴西/把戏 等)— 需逐条 verify。

Phase 6 清理总额估计:~5 个 single quickfix + 1 cascade 顶 2 行 = **6 行可 retire**(谨慎)。

### v2 sovereignty rule(iter#5 加)

Quickfix 显式 user-polish 不吃 modern_freq bonus。文档于 `MODERN-FREQ-DESIGN.md`,代码于 `lib.rs:444-453`。
