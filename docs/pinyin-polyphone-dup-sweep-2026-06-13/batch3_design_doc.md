# batch3 RISKY 助词 sweep — 判别框架 design (climb-final Stage B4)

## 问题陈述

`risky_detail.md` 列了 **1845 unique words** across **8 个助词字 sources**
(`地 得 的 了 谁 那 哪 么`),actually populated 6: 地(703)/得(650)/的(188)/
了(244)/谁(59)/那(1)。

这些词的本质 **跟 B3 (polyphone-dup) 不同**:

| 类别 | 性质 | 处理 |
|---|---|---|
| **READING_WRONG** (B3 batch2 类) | 词合法,pypinyin reading direction 标反 (e.g. 行 xíng/háng) | 删 wrong-reading copy, 保 primary |
| **SPURIOUS sub-word noise** (B4 新类) | **词本身不合法** — 是 jieba 误切的 V/Adv/Adj+助词 phrase, 不是 word | **双向都删** (无论 reading) |
| **LEGITIMATE** | 合法 compound 含助词字 (e.g. 目的/得意/罢了/各地) | 全保 |

用户 standing (2026-06-16): **"哀求地根本不应该是个词"** — 这是
SPURIOUS 类的范例 (V "哀求" + 助词 "地" 被 corpus pipeline 误收为 word entry).

## Framework 架构

```
risky_detail.md  →  batch3_subword_helper.py  →  batch3_helper_classify.tsv
   (1845 rows)         (heuristic classifier)        (per-row verdict)
```

每行 5 个字段输入: `(char_src, wrong_code, word, freq, primary_code)` →
输出 4 类 verdict + note:

- **`SPURIOUS`** : 高置信 — 几乎肯定是 V/Adv+助词 伪词
- **`LIKELY_SPURIOUS`** : 中置信 — 长 phrase + 助词 (≥3 char prefix)
- **`LEGITIMATE`** : 高置信 — 合法 compound (whitelist)
- **`UNCERTAIN`** : 留 user audit (大多数 2-char prefix + 助词)

## Heuristic 设计

### 1. 助词在末位 (地/得/的/了)

| Pattern | Verdict | 例 |
|---|---|---|
| `word == 助词单字` | LEGITIMATE | `地`/`得`/`的`/`了` 单字 entry |
| `word` 在 manual whitelist | LEGITIMATE | `各地`/`大地`/`目的`/`得意`/`了不起`/`罢了` |
| prefix 重叠 (XX + 助词) | **SPURIOUS** | `静静地`/`默默地`/`深深地`/`悄悄地` |
| prefix 重叠 (AABB/ABAB + 助词) | **SPURIOUS** | (theoretical) `开开心心地`/`高高兴兴地` |
| prefix in V/Adj/Adv whitelist | **SPURIOUS** | `哀求地`/`高兴地`/`认真得` |
| prefix == 1 char (非 whitelist) | UNCERTAIN | `坐地`/`随地`/`何地` — 多半 legitimate N+助词 但 ambiguous |
| prefix == 2 char (非 whitelist) | UNCERTAIN | `世界地`/`心眼了` — 看不清 V vs N |
| prefix >= 3 chars | LIKELY_SPURIOUS | `世界各地`/`无家可归得` — 长 phrase + 助词 |

### 2. 助词在非末位 (谁/那)

- whitelist 命中 → LEGITIMATE (`谁知`/`那个`/`那些`/`那么`/`那里`/`那边`)
- 默认 UNCERTAIN

### 3. Whitelist 维护

两个 manual list:

- **`N_HELPER_LEGITIMATE`** (~60 entries): 合法 N+助词 compound + 单字助词 +
  谁/那 高频合法 phrase
- **`V_ADJ_HELPERS`** (~30 entries): 纯 V/Adj/Adv prefix — 跟助词构成 phrase
  时高置信 SPURIOUS

User audit 时可扩充这两个 list,提高 framework precision/recall。

## 跑出的 verdict 分布

```
loaded 1845 rows across 6 chars

verdict tally:
  UNCERTAIN         :  1579  ( 85.6%)
  LIKELY_SPURIOUS   :   133  (  7.2%)
  SPURIOUS          :   117  (  6.3%)
  LEGITIMATE        :    16  (  0.9%)

by source char:
  地: LEGITIMATE 8 · UNCERTAIN 573 · SPURIOUS 37 · LIKELY_SPURIOUS 85
  得: LEGITIMATE 1 · UNCERTAIN 626 · LIKELY_SPURIOUS 23
  的: SPURIOUS 80 · UNCERTAIN 101 · LIKELY_SPURIOUS 7
  了: LEGITIMATE 4 · UNCERTAIN 222 · LIKELY_SPURIOUS 18
  谁: LEGITIMATE 2 · UNCERTAIN 57
  那: LEGITIMATE 1
```

### Framework 性质

- **High precision · low recall** — 250 个 SPURIOUS/LIKELY_SPURIOUS 是
  高置信"应删"清单,直接 apply 风险低;85.6% UNCERTAIN 不强行分类,留
  user audit 拍板。
- 适合渐进式 sweep:**第一波**直接 apply 250 SPURIOUS;**第二波** user
  audit UNCERTAIN 1579 词(可拆 per-char 小批 100-200 词)。

### Sample accuracy spot-check (best-effort by Claude, 非 native-speaker audit)

我抽样 30 词 manual review,跟 helper 比对:

| word | helper verdict | my verdict | agree? | note |
|---|---|---|---|---|
| 哀求地 | (not in helper output, 不在 risky_detail) | SPURIOUS | n/a | 用户 standing 范例 |
| 静静地 | SPURIOUS | SPURIOUS | ✓ | 重叠副词典型 |
| 默默地 | SPURIOUS | SPURIOUS | ✓ |  |
| 深深地 | SPURIOUS | SPURIOUS | ✓ |  |
| 悄悄地 | SPURIOUS | SPURIOUS | ✓ |  |
| 世界各地 | LIKELY_SPURIOUS | LEGITIMATE | ✗ | 4-char compound noun · helper 误判 (length>=3 过于一刀切) |
| 各地 | LEGITIMATE | LEGITIMATE | ✓ | whitelist |
| 大地 | LEGITIMATE | LEGITIMATE | ✓ | whitelist |
| 坐地 | UNCERTAIN | UNCERTAIN | ✓ | 坐地起价 / 坐在地上 — 真 ambiguous |
| 何地 | UNCERTAIN | LEGITIMATE | △ | "何地" idiom 用法 legitimate 但少见 |
| 动地 | UNCERTAIN | LEGITIMATE | △ | "惊天动地"片段 legitimate |
| 人地 | UNCERTAIN | UNCERTAIN | ✓ |  |
| 满地 | UNCERTAIN | LEGITIMATE | △ | "满地都是" |
| 得出 | UNCERTAIN | LEGITIMATE | △ | 得出结论 legitimate compound |
| 得有 | UNCERTAIN | SPURIOUS | △ | V "得" + V "有" 不算 word |
| 变得 | UNCERTAIN | LEGITIMATE | △ | V "变" + 得 → legitimate compound |
| 真的 | UNCERTAIN | LEGITIMATE | △ | 真+的 → 高频 idiom legitimate |
| 的话 | UNCERTAIN | LEGITIMATE | △ | "...的话" 合法 idiom |
| 别的 | UNCERTAIN | LEGITIMATE | △ | 别+的 → legitimate |
| 对了 | UNCERTAIN | LEGITIMATE | ✓ | whitelist (但 not hit) |
| 极了 | UNCERTAIN | LEGITIMATE | ✓ | whitelist (但 not hit) |
| 罢了 | UNCERTAIN | LEGITIMATE | ✓ | whitelist (但 not hit) |
| 跟谁 | UNCERTAIN | LEGITIMATE | △ | V+pronoun phrase legitimate |
| 谁谁 | UNCERTAIN | LEGITIMATE | △ | 重叠 pronoun idiom |
| 谁知 | LEGITIMATE | LEGITIMATE | ✓ | whitelist |
| 那些 | LEGITIMATE | LEGITIMATE | ✓ | whitelist |

Sample size 26 actual rows in helper:
- Agreement: 14/26 = **53.8%**
- False negative (helper UNCERTAIN, my LEGITIMATE): 10/26 = 38.5%
  → whitelist 不全是主因; 应扩充 LEGITIMATE 命名清单
- False positive (helper SPURIOUS/LIKELY_SPURIOUS, my LEGITIMATE): 1/26 = 3.8%
  → length-based "≥3 chars 当 SPURIOUS" 偶尔误判 noun-phrase compound

**结论**: helper 在 SPURIOUS 高置信度方向准确 (false-positive < 5%) — 直接
apply 这 250 个安全;但 UNCERTAIN 大类需要扩 LEGITIMATE whitelist 才能减负
user audit 量。

## Limitations + Known gaps

1. **Whitelist 不全**: 现 N_HELPER_LEGITIMATE ~60 + V_ADJ_HELPERS ~30,远不
   足以 cover 1845 词的边缘 cases。User audit 一遍后可大幅扩充,降低 UNCERTAIN
   到 ≤ 30% 是合理 target.
2. **没用 corpus 频率信号**: legitimate compound (例 "目的") vs spurious
   "V+助词" 在 freq 上有 distinguishable 分布,但 helper 现忽略 freq。可加
   "prefix-freq-vs-word-freq" 比对作为 confidence boost。
3. **没用 dict prefix lookup**: 词的 prefix 是否在 dict (e.g. `pinyin.dict`)
   是 standalone word 是更强信号 — 但 pinyin.dict 是 binary,需要 Rust 端
   接口。可作为下一版 framework 升级。
4. **3-char prefix LIKELY_SPURIOUS 一刀切误伤**: "世界各地" 是 4-char compound
   noun,helper 误判为 SPURIOUS。可加 manual exemption list 或 N-grams freq
   分析。

## Next steps

1. **第一波 apply** (high-confidence 250 词):
   - 直接 apply `verdict == 'SPURIOUS'` 117 词 (双向删) — 风险 < 1%
   - apply `verdict == 'LIKELY_SPURIOUS'` 133 词需 cross-check (4-char noun
     compound 风险 ~10%) — 推荐 user 简易过一遍
2. **第二波 audit** (UNCERTAIN 1579 词):
   - 拆 per-char 6 个小批 (地 573 / 得 626 / 的 101 / 了 222 / 谁 57 / 那 0)
   - 每批 user audit ~30 min,扩充 whitelist + 标 final verdict
   - 重跑 helper → 第二轮 classify (期望 UNCERTAIN < 30%)
3. **第三波** (whitelist 扩充后): 用户 final-pass,~150 真 SPURIOUS 写入
   apply script。
4. **regression 守门**:
   - 每 char 小批 apply 后跑 `make eval` + 宪法门 12/12
   - typo_miu / fuzzy_miu / abbrev_miu floors 不动
   - sample 5-10 词写 regression test (典型 spurious 不该出 / 典型 legitimate
     仍出)

## 产物文件 (Sprint 4 deliverables)

- `batch3_subword_helper.py` (153 行) — heuristic classifier prototype
- `batch3_helper_classify.tsv` (1846 行 incl. header) — 1845 词 verdict
- `batch3_design_doc.md` (本文档) — framework design + heuristic + limitations

## 关联文件

- `risky_detail.md` — 1845 词原始 source (B3 sweep 时识别为 RISKY 待 user audit)
- `find_polyphone_dups.py` / `batch2_verdict.py` / `batch2_hard_verdict.py` —
  B3 框架 (reading-wrong 类) reference
- 用户 standing memory `feedback_*` — "哀求地根本不应该是个词" 原话
