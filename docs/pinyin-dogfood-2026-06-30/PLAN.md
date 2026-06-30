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
- 2026-06-30 iter#13 — 3.1+3.2+3.3 done(atomic)。新 binary `inputx-dogfood`,`[[bin]]` 在 inputx-core/Cargo.toml。每行 in-process `inputx_pinyin_v2::query()` → 找 expected word 排名 → 分 PASS/SOFT/HARD。Build clean。Next: 3.4 smoke 100
- 2026-06-30 iter#14 — 3.4 done。Smoke 100:**86% PASS / 11% SOFT / 3% HARD**。验证 pipeline OK。3 类 polish target pattern 识别清:A.proper-noun reject(英语 Ying1yu3 ingest 拒)、B.cedict 漏(管理器/最早 不在源)、C.单字 polysemy jieba freq 偏(为/中/于 等)。**Phase 3 全 done 🎉**。Next: 4.1 50-article smoke(其实 corpus 已 36 篇,直接跑全 → 4.3 full)
- 2026-06-30 iter#15 — 4.1+4.2+4.3+4.4 atomic done。Re-gen segments(66 articles → 47,904 segments)。Full dogfood:**PASS 67.4% / SOFT 18.2% / HARD 14.4%**。Aggregate:3218 distinct HARD,top 集中在 polish 真目标。SOFT 由单字 polysemy 主导(与/为/中/并 等 ~50 个常用功能字 quickfix candidate)。Next: 4.5 analysis report → Phase 5 batch polish。
- 2026-06-30 iter#16 — 4.5 done。`reports/run-001-analysis.md` 写好。关键发现:article 0058(偶像梦幻祭)单文 3989 HARD = 58%,扭曲数据。real-domain HARD ≈ 6.6%。SOFT-1c 66%/SOFT,功能字 muscle memory vs 实词 jieba freq 冲突。Phase 5 plan: 5.1 SOFT-1c top-25 batch、5.2 ordinal/locale compound、5.3 specialty、5.4 lang names。预估 67%→83%。**Phase 4 全 done 🎉**。Next: Phase 5.1 batch quickfix top-25 单字
- 2026-06-30 iter#17 — 5.1 done。14 quickfix 尝试 → baseline 反对 3(xiang/yi/you),11 落地(yu 与/wei 为/zhong 中/bing 并/hou 后/yu 于/jiang 将/ceng 曾/hua 话/huo 或/deng 等)。Run-002 dogfood:**PASS 67.4 → 71.5%(+4.1pp,SOFT -1983 条)**。baseline 357/0 ✓ mac/reinstall ✓。Next: 5.2 HARD ordinal/locale compound batch。
- 2026-06-30 iter#18 — 5.2 done。25 modern_vocab compounds 加(中队 / 一名/一架/一张/一位/两名/两个/一直/这是/三年 / 微软/国际足联 / 民国/台湾/四川省/浙江/江西 / 英语/英文/法语/德语/日语/汉语/韩语/苏格兰)。Re-gen modern_freq。**PASS 71.5 → 71.8%(+0.3pp,真词 178 个 from HARD → PASS)**。bump 小因为 article 0058 anime 拖。baseline 357/0 ✓ deploy ✓。Next: 5.3 next compound batch + 单字函数字补充。
- 2026-06-30 iter#19 — 5.3 done。14 modern_vocab(环球小姐 / 这场/该届 / 译作 / 单人滑 / 巡回演唱 / 监委 / 联合会杯 / 交大 / 很快/很多/最大/明朝/管理器)+ 1 quickfix(gai 该 30k)。1 retired(zhi 至 撞 baseline zhi→只)。Run-004:**PASS 71.8 → 72.0%(+0.2pp,HARD -136)**。Corpus 136 articles(slow fetcher 跑 28 min)。Next: 5.4 expand corpus + re-run。
- 2026-06-30 iter#20 — 5.4 done。Corpus 扩 140 articles → 72,064 segments(+24k)。Run-005 baseline 71.1%(扩大后 niche article 16 个,带新 HARD)。Iter D batch:12 modern_vocab(太平洋/作词/亚足联/奥运/甲级联赛/毛主席/德甲/广东省/射手榜/外围赛/靠前/中英街)+ 1 quickfix(qian 前 30k)。**Run-006: PASS 71.1 → 71.4%(+0.3pp,HARD -197)**。baseline 357/0 ✓ deploy ✓。Next: 5.5 batch 4 + framework gap 检查。
- 2026-06-30 iter#21 — 5.5 done。Niche analysis:16 articles >100 HARD 占 68% segments。非 niche pure PASS 71.0%。31 modern_vocab cross-article HARD batch(存于/纪录/赛果/一项/四个/...第三/非洲/七月/印尼 等)。**Run-007: PASS 71.4 → 71.7%(+0.3pp,HARD -315)**。每 batch 增益 +0.2-0.3pp,但累计 effective(67.4% → 71.7%)。baseline 357/0 ✓ deploy ✓。Next: 5.6 next batch + diminishing return 检查。
- 2026-06-30 iter#22 — 5.6 done。大 batch(50 mod_vocab + 4 quickfix,1 retired hao 号)。**Run-008: PASS 71.7 → 72.0%(+0.3pp,HARD -245)**。Diminishing return 验证(各 batch +0.2-0.3pp 稳定)。累计 6 batch 共 ~200 polish 行 → PASS 67.4 → 72.0%(+4.6pp)。baseline 357/0 ✓ deploy ✓。Next: 5.7 final batch + Phase 6 准备。
- 2026-06-30 iter#23 — 5.7 done。Iter G batch:17 modern_vocab(大韩民国/检察工作/网络攻击/英联邦/清华大学/东南亚 等)。**Run-009: PASS 72.0 → 72.1%(+0.1pp,HARD -92)**。Diminishing return final acknowledge。**Phase 5 全 done 🎉**(7 iter,~220 polish 行)。累计 PASS 67.4 → 72.1%。Next: Phase 6.1 retire obsolete quickfix。
- 2026-06-30 iter#24 — 6.1 done。9 retire 候选试,7 落地 retired(ceshi 测试 / yanjiu 研究 / liucheng 流程 / lianxu 连续 / shenru cascade 3 行)。2 还原(xingshi cascade 不能部分撤,姓氏/刑事 30k+20k 仍需 形式/形势 50k+40k 顶上保 cascade)。Lesson:cascade quickfix 整体 — retire 必须全或不。PASS 72.1% 不变(modern_freq 接管成功)baseline 357/0 ✓。Next: 6.2 final report。
- 2026-06-30 iter#25 — 6.2 done。`reports/final.md` 全 7 节(TL;DR / 25 iter 总览 / design 区间 / 10 runs 数据 trajectory / polish data stats / 7 lessons / outstanding gaps / 0 framework gap / files / conclusion)。Next: 6.3 update memory。
- 2026-06-30 iter#26 — 6.3 done。`memory/project_dogfood_2026-06-30.md` 写。MEMORY.md 加 anchor(放在 Project state 顶,标 "新 anchor")。涵盖 modern_freq 设计 / dogfood pipeline / 7 lessons / outstanding。**Phase 6 3/4**。Next: 6.4 tag(可选)/ ALL DONE。
- 2026-06-30 iter#27 — 6.4 done。**ALL DONE 🎉**。Final sanity:baseline 357/0 ✓,live IME mtime 18:52(modern_freq + 全 polish 部署),全 commits push。Cron `f4ea17d8` cancelled(/loop 3m 停止)。 PushNotification 通知用户 done。
- 2026-06-30 iter#28 — **REOPENED**:user /loop 3m 再启(job 14135fef)说"全部做完"。Phase 5 重开。corpus 272 articles(slow fetcher 已 1h)→ 111,204 segments。Run-011 baseline 71.8%。Iter H batch 41 modern_vocab cross-article(这一/奥林匹克/这次/职棒/挑战赛/胜方/第四/西班牙语/中华/民主党/四川/国会/系列赛 等)。**Run-012: PASS 71.8 → 72.1%(+0.3pp,HARD -435)**。baseline 357/0 ✓ deploy ✓。Next: 5.10 Iter I batch。
- 2026-06-30 iter#29 — 5.10 done。17 modern_vocab Iter I(第一场/第二张/取自/国联/该校/首张/亚特兰大/东区/总冠军/奥运会/残奥会/每队/这颗/多出/有机溶剂/致力于/威廉)。**Run-013: PASS 72.1 → 72.2%(+0.1pp,HARD -85)**。Discovery:modern_vocab 上限 tier 3(freq≥60k),无法盖 HSK 2 cedict tier 2(以为/一向 等)。Next: 5.11 Iter J batch。
- 2026-06-30 iter#30 — 5.11 done。32 modern_vocab Iter J(第六/北约/转换成/赛程表/音源/一所/同为/最具/清朝/排名第/最低/一年/队史/双循环/请参阅/银河系/很少/实时/国共/东京都/两位/本季/该项/第三任/下半区/第十届/现名/各项/两种/这项/台中市/五人)。**Run-014: PASS 72.2 → 72.3%(+0.1pp,HARD -148)**。Diminishing return 极尖锐 — 接下来要么 wrap 要么换 attack vector。Next: 5.12 decide。

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
- [x] 3.1 Write `core/crates/inputx-core/src/bin/inputx_dogfood.rs`(read segments.tsv,for each call v2::query() in-process)
- [x] 3.2 Failure classification logic(HARD/SOFT/PASS — 三段)
- [x] 3.3 Cargo wire — `[[bin]] inputx-dogfood`,build clean
- [x] 3.4 Smoke run on first 100 segments → 验证 pipeline ✓。**86% PASS / 11% SOFT / 3% HARD**。3 类 pattern: A.proper-noun reject(英语 Ying1yu3 / etc),B.cedict 漏(管理器/最早),C.单字 polysemy jieba freq 偏(为/中/于 等被同音字压)。

### Phase 4: full dogfood run
- [x] 4.1 Run dogfood on 50 articles ~~smoke~~ — skipped, full corpus only 66 articles, run all
- [x] 4.2 Bug-fix pipeline — pipeline OK from iter#14 smoke (86% PASS)
- [x] 4.3 Run on full corpus (47,904 segments from 66 articles) → `failures/run-001-full.tsv`。**PASS 67.4% / SOFT 18.2% / HARD 14.4%**
- [x] 4.4 Aggregate stats:3218 distinct HARD, top: 宣传语(148)/祭(115)/梦之咲(101)/预选赛(88)/存于(83)。SOFT 单字 polysemy 主导:与(448)/为(395)/中(308)/并(240)/以(179)/后(171)/于(153)/将(141)等。
- [x] 4.5 Write `reports/run-001-analysis.md` — top-line stats、corpus skew warning(article 0058 anime → 58% HARD)、SOFT-1c top 30 batch candidates、HARD 分类(general vs niche skip)、Phase 5 plan(5.1-5.4 + predicted PASS rate trajectory 67% → 83%)

### Phase 5: batch polish iterations
每 iter:从 failures 选 top N → 自动生成 polish patch → 应用 → 验证 improvement → commit
- [x] 5.1 Iter A:**SOFT-1c 批量 quickfix** done。11 quickfix 应用(yu 与/wei 为/zhong 中/bing 并/hou 后/yu 于/jiang 将/ceng 曾/hua 话/huo 或/deng 等)。3 retired(xiang/yi/you 撞老 baseline)。**PASS 67.4% → 71.5%(+4.1pp,SOFT -1983)**。baseline 357/0 ✓。mac/reinstall ✓。
- [x] 5.2 Iter B done。25 modern_vocab compounds 加(中队/一名/一架/...微软/英语/英文/法语/德语/汉语/韩语/苏格兰/台湾/浙江/江西/四川省/民国 等)。Re-gen modern_freq.tsv。**PASS 71.5 → 71.8%(+0.3pp,HARD -178)**。baseline 357/0 ✓。Anime article 0058 仍然拖 HARD 总数。
- [x] 5.3 Iter C done。14 modern_vocab + 1 quickfix(gai 该)落地。1 retired(zhi 至 撞 baseline)。**PASS 71.8 → 72.0%(+0.2pp,HARD -136)**。baseline 357/0 ✓ deploy ✓。corpus 增至 136 articles(slow fetcher 持续)。
- [x] 5.4 Iter D done。Corpus 扩 140 articles → 72,064 segments(+24k)。Run-005:71.1% baseline。Iter D batch:12 modern_vocab(太平洋/作词/亚足联/奥运/甲级联赛/毛主席/德甲/广东省/射手榜/外围赛/靠前/中英街)+ 1 quickfix(qian 前 30k)。Run-006:**PASS 71.1 → 71.4%(+0.3pp,HARD -197)**。baseline 357/0 ✓ deploy ✓。
- [x] 5.5 Iter E done。Niche analysis(16 articles 占 68% segments / wider noise)。非 niche PASS 71.0%(close to overall)。31 cross-article HARD batch 加(存于/纪录/赛果/一项/四个/本赛季/一座/三名/一组/亚历山大/示例/每组/第一届/亚洲杯/卫冕冠军/该国/中华民国/第三/决赛圈/非洲/第二名/台北市/优胜者/五个/大事记/一首/一场/七月/印尼/专页 等)。**Run-007: PASS 71.4 → 71.7%(+0.3pp,HARD -315)**。baseline 357/0 ✓ deploy ✓。

### Phase 6: 收尾
- [x] 6.1 Retired 7 quickfix rows(ceshi 测试 / yanjiu 研究 / liucheng 流程 / lianxu 连续 / shenru 深入+渗入+慎入 cascade)— modern_freq 接管。Cascade lesson:xingshi 4 行不能部分撤(姓氏 30k 会顶 #0 since 形式 没了快接)→ 还原 形式/形势,只单独 word polish 撤。dogfood PASS 72.1% 不变 ✓ baseline 357/0 ✓ deploy ✓。
- [x] 6.2 Write final `reports/final.md` — 7 节(TL;DR / phases / 设计 / 数据轨迹 / polish stats / lessons / outstanding / framework gaps / files / conclusion)
- [x] 6.3 Update memory — `memory/project_dogfood_2026-06-30.md` 写好(项目摘要 + 7 lessons + outstanding + 关联其它 memory),MEMORY.md 加 anchor
- [x] 6.4 ALL DONE — final verify(baseline 357/0 ✓ + live IME deployed ✓ + 全 commits push)+ cron `f4ea17d8` cancelled。

## ALL DONE — dogfood concluded 🎉

shipped 2026-06-30. PASS 67.4 → 72.1% on 72k segments. modern-freq + 7 polish batches + 7 retired quickfix. Phase 6 收尾。Final report:`reports/final.md`。

## Reopened 2026-06-30 (user /loop 3m again)

User wants 全部做完。Reopen Phase 5 — corpus grew to 272 articles (slow fetcher 1h+ running). Continue polish iterations with bigger corpus + tighter focus on residual targets.

### Phase 5 v2 checklist
- [x] 5.8 Re-segment 272 articles → 111,204 segments;Run-011 baseline 71.8%
- [x] 5.9 Iter H batch:**41 modern_vocab** cross-article(这一/奥林匹克/这次/挑战赛/职棒/巡回赛/胜方/第四/病疫情/羽联/西班牙语/男子双打/维基/三个/两天/一款/中华/预选赛/本次/两人/总教练/四川/国会/首个/第一位/飞往/上届/一条/第二位/每场/三种/民主党/爱尔兰/乙级/五人制/第一阶段/系列赛/棒球场/某个/而成/一站/西安)。**Run-012: PASS 71.8 → 72.1%(+0.3pp on 111k segs,HARD -435)**。
- [x] 5.10 Iter I done。17 modern_vocab(第一场/第二张/取自/国联/该校/首张/亚特兰大/东区/总冠军/奥运会/残奥会/每队/这颗/多出/有机溶剂/致力于/威廉)。**Run-013: PASS 72.1 → 72.2%(+0.1pp,HARD -85)**。Discovery:modern_vocab tier 限制 — 35k → tier 5,无法盖 HSK 4 cedict 词(以为/一向 等)。 单音节同音冲突(yiwei 一位 vs 以为,yixiang 一项 vs 一向)需 quickfix 才能强制 #0 — but skip for now since both legitimate uses。
- [x] 5.11 Iter J done。32 modern_vocab(第六/北约/转换成/赛程表/音源/一所/同为/最具/清朝/排名第/最低/一年/队史/双循环/请参阅/银河系/很少/实时/国共/东京都/两位/本季/该项/第三任/下半区/第十届/现名/各项/两种/这项/台中市/五人)。**Run-014: PASS 72.2 → 72.3%(+0.1pp,HARD -148)**。
- [ ] 5.12 Decide: continue or final wrap?


## Open questions(任何一项有 update 写这里)

> _(empty)_

- [x] 5.7 Iter G done。17 modern_vocab(大韩民国/检察工作/网络攻击/英联邦/清华大学/东南亚/第三轮/一片/文件名/北平市/保障卡/运动型/西区/通联/涅瓦河/浦项/翰林院)。**Run-009: PASS 72.0 → 72.1%(+0.1pp,HARD -92)**。Phase 5 收尾,DR 明显。**Phase 5 全 done 🎉**。Next: 6.1 retire obsolete quickfix。
- [x] 5.6 Iter F done。大 batch:50 modern_vocab(可用/中有/星系/检察长/已有/圣地亚哥/慕尼黑/阿肯色州/首名/英格兰/该次/净胜球/苏联/北京市/上赛季/升降级/宋朝/第三名/每轮/大西洋/该州/北美洲/该地/大碟/决出/第三位/单场/种子队/分组赛/四队/菲律宾/一颗/进球数/两队/维多利亚/是因为/主客场/决选/第二代/中以/最早/一词/之意/联同/四名/第一个/共和党/亿光年/北宋/之子)+ 4 quickfix(dan 但/zu 组/ci 此/ming 名)。1 retired(hao 号 撞 baseline)。**Run-008: PASS 71.7 → 72.0%(+0.3pp,HARD -245)**。Diminishing return 明显(每 batch 稳定 +0.2-0.3pp)。baseline 357/0 ✓ deploy ✓。Next: 5.7 决定 wrap or continue。

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
