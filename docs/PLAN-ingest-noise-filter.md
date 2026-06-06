# 主动 corpus 噪音过滤 — 设计提案 (2026-06-06)

> 用户 2026-06-06 directive: "不光要删，还要看看为什么，我们其实
> 输入法是宁缺毋滥的，缺了补就行，滥了用户还要面临从垃圾堆里选
> 东西的感受".
>
> 这份文档分析为什么 jieba 主词典 ingest 会塞进字字直拼噪音
> (叫朱/椒猪/交住/间体/土里/...)，并提出一个 ingestion-time
> proactive 过滤层，让噪音**永远不进库**，而不是等用户报 polish。
>
> 状态：**设计提案，未实施**。等架构 review 通过再落地。

## 1. 现象 — "字字直拼" 噪音

每隔几天就有用户报 polish：

| Buffer | 字字直拼噪音 | 真词 | D1 commit |
|---|---|---|---|
| jianti | 间体 | 简体/健体/舰体 | 4033b1c |
| tuli   | 土里/图里/土粒 | (无单独词，只在 埋在土里 等) | 546518a |
| jiaozhu | 叫朱/较著/椒猪/交住 | 教主/叫住/浇筑/脚注/校注/浇铸 | cfdad9c |

**共同模式**：

- 单字甲 + 单字乙拼出 buffer
- 单字甲、单字乙各自都常见、各自的拼音在 buffer 里都合法
- 但**组合**没有词典意义、不进现代汉语词典、jieba 自己 segment 时也不会把它们当词
- 这些条目 source 全是 `digested` → 来自 `结巴中文分词词典 (主词典 dict.txt)` 那次 24757 行的 external ingest (`digest_log.toml` 中第一个 external event)

## 2. 根因 — jieba 主词典本身的杂质

jieba 的 `dict.txt` 不是经过严格人工整理的现代汉语词典——它是 jieba
在大量训练 corpus 上 segment 得到的 2-字共现高频对的合集。这意味着：

- **真词**（你好/中国/今天 etc）占大多数
- 但**字字共现频率高的 2 字组合**也会被收入，即使语义上不成词
  - 例：椒猪 = 椒 (常见调料字) + 猪 (常见动物字)，两字在烹饪 corpus
    里高频共现 → jieba 收作"词" → 我们的 ingest 不加判别就吃下

我们的现行 ingest pipeline (`tools/scoring/data/digest_log.toml`
event 1) 没有对单条目做"是不是真词"的判别，所以这类垃圾按 frequency
全部进了 `library.tsv`，source 标 `digested`，从此和真词混在一起。

## 3. 现行 reactive 防线 — 不够

`tools/scoring/data/polish/corpus_garbage_filter_v1.tsv` 是 D1 删除
的 audit log，目的是"这条已经判为垃圾，未来再 ingest 不要再放回"。
但是：

- **被动**：等用户碰到才知道这条是垃圾
- **覆盖率低**：jieba 主词典 24757 行 noise 还有多少没被发现的、用户
  哪天才碰到的？积压库存
- **每条 D1 commit 也是用户成本**：用户要打到那个 buffer、要发现噪音、
  要报 polish；我们的 polish skill 跑一遍

> "宁缺毋滥"的承诺要求：**新 ingest 时**就把可疑条目拦下，不要让
> 它们成为 library.tsv 的一员，从根上消灭后续 D1 的需要。

## 4. 提案 — proactive ingest-time filter

`PLAN-corpus-治理.md` §0 digest pipeline 已经有 step 4 "garbage
filter (拒绝黑名单)"——指的是消费 `corpus_garbage_filter_v1.tsv`
的拒绝。我们要在它**前面**或扩展它，加入两条新规则：

### 4.1 Frequency cliff detection

对 jieba 主词典每个候选 `(code, word, freq)`：

- 同 `code` 下，查 ingest 前 library.tsv 已有的 max freq → `peer_max`
- 如果 `freq < peer_max × 0.30` AND `word` 长度 = 2 字 → **flag 为可疑**

理由：真常见双字词在 jieba 的统计里 freq 不会比同 code 已有真词低
70% 还得入选。如果一个新候选 freq 这么低还是 jieba 推上去的，大概率
是 jieba 的训练 corpus 里碰巧有几个 outlier sentence 把这个 2 字
共现冲高的。

可调：阈值 0.30 是经验起点；若 false-reject 率过高（真词被拦），
回调到 0.20。

### 4.2 Real-word lexicon cross-check

用一份 **modern Chinese lexicon ground truth** (现代汉语词典/
SUBTLEX-CH/CCD 等) 当白名单：

- 候选 `word` 在 lexicon 中 → 直接通过
- 候选不在 lexicon → 必须满足 4.1 的 frequency cliff **更宽松版本**
  (e.g. freq > peer_max × 0.50)，或同时被多份 corpus 推上去

> Caveat：lexicon 选哪份要议题。jieba 自带不算，因为我们要拦的就是
> jieba 自己的杂质。候选包括 SUBTLEX-CH 校对版 / 现代汉语词典电子化
> /Wiktionary 中文条目 dump 等。每份都有 license / size / 偏差
> trade-off。

### 4.3 Pipeline 位置

```
[jieba ingest] → normalize → dedupe →
  ┌──────────────────────────────┐
  │ NEW: noise filter            │
  │  4.1 freq cliff              │ → reject_log
  │  4.2 lexicon cross-check     │ →  ↓
  └──────────────────────────────┘  → digest_log.toml
            │                         (rows_rejected counter)
            ▼
[既有 garbage filter (黑名单)] →
[diff + write 入库]
```

reject_log 不入 library.tsv 但**持久化**到 `digest_log.toml` 的
`rows_rejected` 字段（已存在 schema），并把具体 rejected (code, word)
写入新的 `tools/scoring/data/polish/ingest_rejected_v1.tsv`，方便 audit
+ 必要时人工 unblock (`source=polish` 加回 library 即可)。

## 5. 落地阶段

| Phase | 内容 | 估时 |
|---|---|---|
| NF1 | 选 lexicon 数据源 + license review | 半天 |
| NF2 | 写 noise-filter 模块（独立 binary，跑在 ingest pipeline 中） | 1 天 |
| NF3 | 对现行 library.tsv 跑**回顾扫描**：报告"这一行如果今天才 ingest 会不会被拦"——给已有噪音一个名单 | 半天 |
| NF4 | 用户 review NF3 报告，确认拦截规则不误伤 | 半天 |
| NF5 | 若 NF4 通过，把 NF3 名单批量 D1 + 加进 corpus_garbage_filter_v1.tsv | 半天 |
| NF6 | 把 noise-filter 接入 absorption pipeline (PLAN-corpus-治理.md §0 step 4 扩展) | 1 天 |

总计 ~3.5 天工作量。

## 6. 不做这个的成本

按现行 reactive 模式估算：

- jieba 主词典 24757 行，假设 ~5% (~1200 行) 是字字直拼噪音
- 每个用户每打到一条噪音 buffer 就要一次 polish report → reinstall
- polish skill 流程每次 ~2 分钟用户时间 + commit 时间
- 假设单个用户每周碰到 3 条噪音 buffer → 一年 ~150 次 polish
- 多用户的话乘 N

每次 polish 还都是 dispatch/scoring 没有问题、纯数据洁净的劳动力浪费。

J 这套机制做完之后，新 corpus 进来 / jieba 升级版进来 / 我们换源
进来——噪音都在 ingest 那一刻被拦下，**用户从此不再要为 corpus
质量问题付任何 attention 成本**。和你今天定的"reinstall 永远不用
关心"是同一类目标，只是对象换成"corpus ingest"。

## 7. 当下不做的边界

这份 doc 写完落档就停。没有 task 跑 NF1-NF6。等架构 review 拍板。

短期内继续走 reactive 路线 (用户报 → 我跑 polish skill → D1 + garbage
filter) 直到 NF 系列落地。

## 8. 命名说明（2026-06-06）

这份 doc 内部的子任务编号 **NF1..NF6** (Noise Filter) 是**纯
within-doc 索引**，不作为项目级工件名出现在 BACKLOG / commit
message / 沟通中。项目级语义名是 **入库质量门 (proactive corpus
noise filter)**。

历史：原稿曾用 "Phase J1..J6"，跟 ranking-model series 的 "Phase J"
(后者也已经 retire 为 **音节意识细化**) 撞名，先改成 NF1..NF6；
2026-06-06 进一步统一约定 —— **新工作不再用单字母编号**，独立
工件用描述性名字，cycle 内子单位（如 v1.9 的 WU-π/ρ/σ）保留
原 Greek-letter 因为它们只在 cycle 内有意义。

---

References:
- `tools/scoring/data/digest_log.toml` — corpus ingest audit log
- `tools/scoring/data/polish/corpus_garbage_filter_v1.tsv` — D1 黑名单
- `.claude/PLAN-corpus-治理.md` — 总体 corpus 治理架构
- `.claude/PLAN-corpus-digest.md` — digest 子系统
- 涉及的 D1 commits: 4033b1c (jianti 间体), 546518a (tuli), cfdad9c (jiaozhu 4 件)
