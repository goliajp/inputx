# 拼音 library 全量审计 plan (2026-06-28)

接 [[宁缺毋滥]] standing + 4-gate 全关。**目标**:把 `core/crates/inputx-pinyin/data/library.tsv`
414,747 行(单源:几乎全 corpus digested)系统性过一遍,清掉历年累积的「淤血」——
字字直拼 / 人名 / 文言 / 繁体 / 错读 / polyphone-dup / 灌水高频 等。

**不是**单条 polish,**也不是**一次性大 sweep。是分段攻坚,每段端到端跑通(detector → 候选 →
per-row audit → D1 删 + garbage filter 双写 → polish-rebuild → baseline → mac/reinstall → commit)。

---

## 1. Ground truth(2026-06-28 实测)

**总行数**:`414,747`(`# header` + 空行已排除)

**by source**:
- digested  `414,650` (99.97%)
- polish        `97` (0.02%)

**by char length(中文字符数)**:

| 类 | 行数 | 占比 |
|---|---:|---:|
| 1c | 41,443 | 10.0% |
| 2c | 130,722 | 31.5% |
| 3c | 140,658 | 33.9% |
| 4c | 93,256 | 22.5% |
| 5-6c | 6,303 | 1.5% |
| 7+c | 2,365 | 0.6% |

**by freq tier**:

| tier | 行数 | 占比 | 含义 |
|---|---:|---:|---|
| f0 | 160,199 | 38.6% | 罕用/未进 corpus,大概率可大量删 |
| f1k-10k | 145,298 | 35.0% | 中尾,真词与噪音密度最高的战场 |
| f10k-50k | 109,095 | 26.3% | 常用真词主力 + 灌水嫌疑 |
| f50k+ | 155 | 0.04% | 超高频,几乎全真词 |

**polyphone-dup risk**(一个 word 出现在 N 个 code):
- 1 code:`368,232` words
- 2 codes:`20,097` words(已知部分 = 错读拷贝噪音,详见 [[polyphone-dup-sweep-2026-06-13]])
- 3-4 codes:`1,598`
- 5+ codes:`77`(几乎都是多音字 / 拷贝噪音边界)

**polyphone normal**(一个 code N 个 word reading):
- 1 word/code:`254,648`
- 2-4:`32,636`
- 5-9:`4,659`
- 10-19:`697`
- 20+:`418`(高频 code 如 yi/shi/wo 的同音池)

---

## 2. 分段(覆盖完备 + 互不重叠)

主轴 = **char-length × freq-tier** 24 格,合并为 4 个 tier + 6 个长度子段:

```
       1c       2c       3c       4c     5-6c    7+c
f0   A1 31735 A2 11125 A3 66518 A4 47066 A5 2366 A6 1389  ← Tier-A: rare bucket (160k)
f1k  B1 3784  B2 49850 B3 53619 B4 33834 B5 3365 B6  846  ← Tier-B: mid bucket  (145k)
f10k C1 5781  C2 69735 C3 20521 C4 12356 C5  572 C6  130  ← Tier-C: high bucket (109k)
f50k D1  143  D2   12  ─        ─        ─      ─        ← Tier-D: super-high  (155)
```

**覆盖检查**:24 格(实际 22 格,Tier-D 只有 1c/2c)合计 = 414,747 ✓

每行恰落一段。

### 段内典型噪音类(用于 detector 设计)

| 段 | 主要噪音模式 | detector 思路 |
|---|---|---|
| A1 (1c f=0) | 罕用 CJK 单字、异体字、CJK Ext-B/C/D | charset block 过滤(Unihan Basic/A 外置弱信任) |
| A2 (2c f=0) | 字字直拼 + 罕用 2 字组合 | 字字直拼 detector(下) |
| A3/A4 (3-4c f=0) | 字字直拼成语 / 长 jieba 拼接 | 同上 |
| A5/A6 (5+c f=0) | 长机构名 / 长古文 / 长成语 rare | 人工眼过(小) |
| B2 (2c f1k-10k) | jieba 字字直拼 + 人名 + 真词主力混杂 | 字字直拼 + 人名双 detector,真词白名单兜底 |
| B3 (3c f1k-10k) | 3 字人名 + 3 字 jieba 字字直拼 | 同上,3 字人名 detector 加强 |
| B4 (4c f1k-10k) | 4 字成语 + 4 字 jieba 拼接 + 公司/机构 | 成语字典 vs 字字直拼对比 |
| C2 (2c f10k-50k) | 灌水高频(如 却是 26633)、文言、武侠 | 高频但日常零使用的 register-mismatch detector(难) |
| 全段 | polyphone-dup(同 word 多 code,freq 雷同) | [[polyphone-dup-sweep-2026-06-13]] 现成框架 |
| 全段 | 繁体污染 | 跟 wubi 繁体 sweep 同套 charset 检测 |
| 全段 | 错读(code 跟 word 实际读音不对) | KANJIDIC2/cccedict reading cross-check |

### 段内典型真词(用于保留校验,反向 sanity check)

| 段 | 真词样例 |
|---|---|
| A1 (1c f=0) | 罕用真字(罕、龘、龍 之类的)、CJK Ext-A 真传统字 |
| B2 (2c f1k-10k) | 一般日常 2 字词(电脑、视频、报告、数据 …… 高密度真词) |
| C1 (1c f10k-50k) | 常用字主力(的、一、是、了 …… ) |
| C2 (2c f10k-50k) | 高频 2 字(我们、什么、可以 …… ) |
| D (f50k+) | 超高频(的、一、了 …… )和最常用 2 字几乎肯定全保留 |

---

## 3. 攻击顺序(per [[methodology/perf-decomposition-vs-polish]] 类比)

按 **放大半径 × 安全度 × 操作量** 排序。小+安全先,大+高 cost 放后。

| # | 段 | 行数 | 类型 | 理由 |
|---:|---|---:|---|---|
| 1 | **D1 + D2** | 155 | 眼过 | 超高频几乎全真词;5 分钟过完。Baseline + 信心 |
| 2 | **A6 + B6 + C6** | 2,365 | 眼过 / 半 detector | 7+c 全部一次过(长串噪音 vs 真长机构名好辨) |
| 3 | **A5 + B5 + C5** | 6,303 | 同上 | 5-6c 同 |
| 4 | **A1** | 31,735 | charset detector | 1c f=0 罕字。批量删 + 真异体字白名单 |
| 5 | **A2** | 11,125 | 字字直拼 detector | 2c f=0 大概率字字直拼,删率会很高 |
| 6 | **A4** | 47,066 | 字字直拼 + 成语对比 | 4c f=0,jieba 拼接重灾区 |
| 7 | **A3** | 66,518 | 字字直拼 + 人名 | 3c f=0,体量最大的 rare 段 |
| 8 | **B4** | 33,834 | 成语字典 vs 字字直拼 + 人名 | 4c f1k-10k,真成语高密度但 jieba 拼接也多 |
| 9 | **B2** | 49,850 | **最难一段:**字字直拼 + 人名 + 真词主力混 | 用前面学到的 detector 套路 + 真词白名单兜底 |
| 10 | **B3** | 53,619 | 同 B2 (3c) | 3 字人名 detector 关键 |
| 11 | **C2** | 69,735 | 灌水高频 / register-mismatch | 高 cost 段最后做,前面所有 detector 成熟后 |
| 12 | **C1 + C3 + C4** | 38,658 | 边界 + 收尾 | 全段过一次 sanity check |
| 13 | **B1** | 3,784 | 收尾 | 1c f1k-10k 小段补齐 |
| 14 | **polyphone-dup cross-pass** | (跨全表) | 续 [[polyphone-dup-sweep-2026-06-13]] | MIXED 双向 + RISKY 助词 batch2/3 |

**14 个 phase。** 总覆盖 = 414,747 ✓(polyphone-dup cross-pass 不增量,只处理已在段内但需 cross-code 视角的)

---

## 3.5 工具:HTML 高密度审计台

`tools/audit-ui/index.html`(单文件 SPA,无依赖,本地双击打开)。

**生候选**:`python3 tools/audit-ui/gen-candidates.py <PHASE>` 输出
`tools/audit-ui/candidates-<PHASE>.tsv`。phase id 见脚本 `--help`:
`D / 7plus / 56 / A1..A4 / B1..B4 / C1..C4`。

**操作**:
- 顶部 file input 加载 `candidates-XX.tsv`
- 键盘:`j/k` 上下 · `d` 删 · `s` 保留 · `t` 降 tier · `f` 标存疑 · `u` 撤销
- `n / N` 跳下/上一个未决 · `/` 过滤 · `e` 编辑当前行 note
- `x` 切换紧凑模式 · 表格 ~60 行/屏(紧凑模式 ~80 行/屏)
- 右侧面板自动显示:相同 word 在其它 code(polyphone-dup 信号) + 相同 code 其它 word reading
- 决策实时落 localStorage(按文件指纹 scope);上方「Export」导出 `decisions-<tag>.tsv`

**decisions.tsv 格式**:`<code>\t<word>\t<dec>\t<note>`
- `dec ∈ {d=delete, s=keep, t=tier-demote, f=flag-uncertain, u=undecided}`

下游 `apply.py` 脚本(每 phase 自带)消费 decisions.tsv → 写 library.tsv /
corpus_garbage_filter_v1.tsv / tier_overlay.tsv。

## 4. 每段标准流程(per [[feedback-sweep-per-row-audit]])

每段开干前先建子目录 `docs/pinyin-library-audit-2026-06-28/PHASE-XX-<short-name>/`,
里面放:

1. `BEFORE.md` —— 该段 ground truth(行数 + 抽样 20 真词 + 抽样 20 疑噪音)
2. `detect.py` —— 该段专用 detector 脚本(纯过滤,不动 library)
3. `candidates.tsv` —— detector 输出:`<code>\t<word>\t<freq>\t<flag>\t<reason>`
4. `audit-decisions.tsv` —— 人工 per-row audit 后:`<code>\t<word>\t<decision>\t<reason>`
   决策枚举:`delete` / `keep` / `tier-demote` / `flag-uncertain`
5. `apply.py` —— 执行 audit 决定:`delete` 行从 library.tsv 删 + corpus_garbage_filter.tsv 加,
   `tier-demote` 入 tier_overlay.tsv,等
6. `AFTER.md` —— 该段 done:删了多少 / 留了多少 / baseline test 全绿验证

**Detector 跑完 > 100 候选** → MUST per-row audit(用户 2026-06-22 翻车 57 误删的教训)。
不允许 whitelist 启发式默认 delete。

**每个 phase 一个 commit**:
- 一段做完才 commit(不允许中途半段 commit)
- 大段(>1万候选)拆 sub-batch,每 batch 一个 commit,但 BEFORE/PLAN/AFTER 仍是段级单位

---

## 5. 风险 + 兜底

**风险 1:误删真词**
- 兜底:per-row audit 强制 + 真词白名单(常用现代词,可用 2c f10k-50k 当 reference set)
- 兜底:每段做完跑 baseline 72+ test → 任何回归立即 STOP,撤本段 commit

**风险 2:detector 漏召**
- 接受 —— 这次 audit 不追求一刀切。漏召的 next round audit 再扫
- 用户实战 polish 继续作为漏召 detector 的输入(reactive polish skill 一直 active)

**风险 3:全 audit 期间 library 持续变动(用户加新词 / 别的 sweep)**
- 兜底:严格按 phase 顺序,每 phase 开干前 `git pull` + `git status` 干净
- 兜底:每 phase 一个 commit,冲突局部解决

**风险 4:体量过大 burnout**
- 兜底:phase 1-3 都是小段(< 1 万),开局快速建立节奏 + detector 库
- phase 4+ 段大,但此时 detector 已熟,per-row audit 才是瓶颈

---

## 6. 完成判定

整 audit 收尾 = **同时满足**:
- 14 phase 全部 commit + push + mac/reinstall ✓
- baseline 72+ test 全绿
- reactive polish 报告 rate 显著下降(每周报告数 < audit 前 30%)
- `docs/pinyin-library-audit-2026-06-28/SUMMARY.md` 总账(总删 / 总留 / detector 库索引)

---

## 7. 不在 scope

明确不做(避免漂移):

- **不动 baseline 72 test 来给 audit 让路**(baseline 是终点裁判)
- **不重开 4 gate**(audit 期间需要 literal-only 环境观察净效果)
- **不动 engine_weights.toml**(ranking 公式,跟 data audit 正交)
- **不动 jieba / unigram 上游 corpus**(那是 ingest 阶段;本 audit 是 already-ingested 的 library)
- **wubi / nihongo 不在本 audit**(各有独立 sweep 历史和阶段;先把 pinyin 干完)
