# Modern-freq scaling design

立 2026-06-30. 配套 dogfood polish run.

## 动机

v2 词库干净(HSK + CC-CEDICT,无 corpus 噪音),但 tier 桶太粗,同 tier 内顺序不稳:

```
现状(无 modern-freq):
  gongqi  → 共栖 (生物学) > 工期 (项目)        # 用户报告
  ceshi   → 侧室 (文言) > 测试 (软件)          # 用户报告
  yanjiu  → 烟酒 (口语) > 研究 (学术)          # 用户报告
  ...
```

根因:tier 4(CC-CEDICT 2字 非 HSK)挤进太多古汉语+现代词,同 tier 内只剩 char prominence + codepoint 做 tiebreak。

## 设计原则

1. **干净度归 v2 dict**(words.tsv,HSK + CC-CEDICT 编辑级)
2. **频率信号外引**(jieba dict — modern Chinese corpus 派生)
3. **频率**只**调同 tier 内顺序**,**绝不跨 tier**(corpus 不入库)
4. 实施为 categorical filter(per-word lookup table),不是 per-entry list → 符合 RANKING-MODEL-INVARIANTS §2

## 区间(硬约束)

```
modern_freq 贡献 ∈ [0, 25_000]
```

- 同 tier 跨度 = 30_000(`score = 500_000 - tier × 30_000`)
- 25k 上限留 5k 安全 margin → 绝不让 corpus 高频把 tier 4 词提到 tier 3 位置
- 这是「rank-only」的硬保证 — corpus 调同 tier 顺序,不动 tier 桶

## 映射(percentile rank,linear)

```python
# 准备(ingest 时一次):
jieba_sorted = sorted(jieba_dict.items(), key=lambda x: x[1], reverse=True)
N = len(jieba_sorted)  # ≈ 350,000

# 给每个 v2 word/char 算 score:
for w in v2_words ∪ v2_chars:
    if w in jieba:
        rank = jieba_rank[w]                    # 1..N
        percentile = 1 - (rank - 1) / N         # rank 1 → 1.0, rank N → 0
        score = round(percentile * 25_000)      # u16
    else:
        score = 0                                # 不在现代 corpus = 古汉语/罕用,自然 demote
```

**为什么 percentile rank 不用 log(freq)**:
- jieba 原始 freq 跨 5-6 个数量级(的:3M, 测试:2k, 侧室:27)
- 直接拿 freq 会让 top-10 词碾压其它一切
- percentile rank 对 outlier 鲁棒,自然分布
- 换 corpus(将来从 jieba 切 Sogou)只要重跑 ingest,排序结构稳

**为什么 score 0 给不在 jieba 的词,而不是中位**:
- "不在现代 corpus" = "古汉语/罕用" = **legitimate signal to demote**
- 默认中位会埋掉这个信号

## 校准预期

| 词 | 类型 | 预估 jieba rank / freq | 预估 score | tier 内排位影响 |
|---|---|---|---|---|
| 的 | 助词 | rank 1 (freq 3.3M) | 25000 | tier 1 top |
| 你好 | HSK 1 | rank ~500 | ~24965 | tier 1 top |
| 测试 | 现代技术 | rank ~3000 (freq 2444) | ~24786 | **tier 4 提至同 tier 顶部** |
| 研究 | HSK 5 | rank ~2000 (freq 35029) | ~24857 | tier 3 顶部 |
| 烟酒 | 日常 | rank ~30000 (freq 83) | ~22857 | tier 3 中部 |
| 流程 | 现代办公 | rank ~5000 (freq 523) | ~24643 | tier 4 顶部 |
| 工期 | 项目管理 | rank ~25000 (freq 73) | ~23000 | **tier 4 中上** |
| 侧室 | 文言 | rank 250000 或缺 (freq 27) | ~7143 或 0 | **tier 4 底部** |
| 柳橙 | 台湾水果名 | rank ~lib bottom (freq 5) | ~低 | tier 内最底 |
| 共栖 | 生物学 | rank ~lib bottom (freq 3) | ~低 | tier 内最底 |
| 怜恤 | 古汉语 | 缺 | 0 | tier 内最底 |

**关键 case verify**(实际 dict 数值):
- 工期 73 vs 共栖 3 → percentile 差距显著 → 工期 应 ≫ 共栖
- 测试 2444 vs 侧室 27 → 测试 ≫ 侧室
- 跨 tier:tier 3 烟酒 (modern_freq ~22857) + 410k = 432857 vs tier 4 测试 (modern_freq ~24786) + 380k = 404786 → 烟酒 仍在前 ✓ tier 不破

## v2 query 排序顺序(新)

每个 path 排序 key,**插入 modern_freq DESC 为第二维**(tier 之后,其它之前):

```
1. tier asc                           (HSK 桶,不动)
2. modern_freq DESC  ← 新插入         (现代用词高的胜)
3. char_prominence (单字时,原有)
4. code length asc / word length      (path-specific)
5. codepoint asc                      (stable tail)
```

影响所有 6 path:
- Path 1a exact word
- Path 1b single char
- Path 1c initials reverse-lookup
- Path 3 prefix completion
- Path 5 compose
- quickfix override(quickfix freq 已是显式 override → modern_freq 仅作平局 tiebreak)

## 实施产物

| 文件 | 估行 | 说明 |
|---|---|---|
| `tools/v2-ingest/build-modern-freq.py` | ~70 | jieba dict + v2 words/chars → TSV |
| `core/crates/inputx-pinyin-v2/data/modern_freq.tsv` | ~95k 行,~1.5MB | `<word>\t<score>` per row |
| `core/crates/inputx-pinyin-v2/src/data.rs` `modern_freq()` lazy loader | +20 行 | `HashMap<String, u16>` |
| `core/crates/inputx-pinyin-v2/src/lib.rs` 各 path sort key | +6 sites × +1 行 | sort by `(tier, -modern_freq, ...)` |

## 副作用 / risk

| 类型 | 预期 |
|---|---|
| **老 quickfix 多数自然冗余** | `ceshi 测试 50k` 等 modern-freq 上线后变冗余(MAX 语义,无害,可 retire) |
| **少数老 quickfix 冲突** | 某 quickfix 把 X 推到 #0,但 corpus 给 Y 算更高 → 需 quickfix 再 boost 才压住(不影响,quickfix 是 MAX) |
| **baseline 357 测试** | 大概率全过(测的是常用词);少数边界 case fail → 看是 polish 老期望 outdated 还是 modern-freq 误判 |
| **冷门词 visibility 不变** | corpus 不影响"出不出",只影响"排第几"。古汉语词还在 dict |
| **多音字** | 一个词只一个 jieba freq,跨 reading 不区分 → 暂可接受(后续若发现 1 个词 多 code 共享 freq 不合理再改) |

## 不要做的(明确 anti-pattern)

- ❌ **修改 tier 划分**(让 corpus 高频词升 tier)→ 破坏 tier-based 干净度
- ❌ **从 jieba 加新词到 v2 dict**(用 jieba 频率作为补 dict 信号)→ corpus 入库,污染
- ❌ **per-entry list**(`const MODERN: &[(&str, u32)] = &[...]`)→ 违反 §2
- ❌ **将 modern_freq 作为 path-1 vs path-3 路径选择标准** → modern_freq 是同 tier 内 tiebreak,不是路径决定因素

## 测试锚点(Phase 1 验证 checklist)

跑完 Phase 1 之后,这些 buffer 应该**自动**翻转,**无需新加 quickfix**:

| buffer | 现状 | 期 | 来源 |
|---|---|---|---|
| gongqi | 共栖 > 工期 | 工期 > 共栖 | 用户报告 2026-06-30 |
| ceshi | 侧室 > 测试 | 测试 > 侧室 | 用户报告 2026-06-30 |
| yanjiu | 烟酒 > 研究 | 研究 > 烟酒 | 用户报告 2026-06-30(已有 quickfix,验证 modern-freq 也能独自做到 = retire quickfix 候选) |
| liucheng | 柳橙 > 流程 | 流程 > 柳橙 | 用户报告 2026-06-30(同上 retire 候选) |
| lianxu | 怜恤 > 连续 | 连续 > 怜恤 | 用户报告 2026-06-30(同上) |
| huluobo | (JP 在前) | 胡萝卜在 JP 前 | 用户报告 2026-06-30(跨引擎,modern_freq 可能不够,留 55k quickfix) |
| ceshi 测试 vs 侧室 | 测试 #1 (现已 quickfix) | 测试 #1 (无 quickfix 也成立) | 测自然 |

跑完后:
- 若所有"现已 quickfix"的 case 都能没 quickfix 自然成立 → mark 这些 quickfix 为 retire 候选,Phase 6 清理
- 若 跨引擎(huluobo)case 仍需 quickfix → 保留(那是不同问题,modern_freq 不能解决跨引擎 wx>px>nx)
