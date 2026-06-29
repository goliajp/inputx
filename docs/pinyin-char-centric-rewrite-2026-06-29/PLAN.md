# Pinyin char-centric rewrite — v2

立 2026-06-29。重写拼音模块为「字 + 读音 + 词」三表 char-centric 模型,跟语文学/汉语词典的正统模型对齐,取代当前 corpus-digest 单表 `library.tsv`。

**Scope 红线**: 重写**仅在 pinyin 引擎内部**;`composite/merge.rs` 三语融合层、10-tier × wx>px>nx 正交模型、`engine_weights.toml` 跨引擎 gap、ranking 公式全部**不动**。pinyin 引擎对外契约 (`(word, score, tier)` 候选列表) 不变,下游无感。

## 立项动机

- v1 数据全来自 corpus → digest 自动灌入,字字直拼/人名/文言/polyphone-dup 等噪音是系统性问题
- 日常 polish 一条接一条修症状(查 [[feedback-ningque-wulan]] 直接源头实证)
- 用户 2026-06-29 拍板:换正统模型 + 弃 corpus digest 入库路径

## 数据来源(全 authoritative + 公开 + 兼容授权)

| 层 | 源 | 量 | 授权 |
|---|---|---|---|
| chars | [通用规范汉字表 2013](https://github.com/cdtym/digital-table-of-general-standard-chinese-characters) (一/二/三级 3500/3000/1605) | 8,105 | 国家标准,公有领域 |
| readings 主读 | [mozillazg/pinyin-data `kMandarin_8105.txt`](https://github.com/mozillazg/pinyin-data) | 8,105 | MIT |
| readings 多音字 | [Unihan kHanyuPinyin / kXHC1983](https://www.unicode.org/reports/tr38/) | ~12k | Unicode 公开 |
| words | [CC-CEDICT](https://www.mdbg.net/chinese/dictionary?page=cedict) (筛繁体 / 筛词频) | ~70k(去筛后) | CC-BY-SA 4.0 |
| words 补充 | HSK 词表 6 级 | ~5k | 汉办,公开 |
| 词层 cross-check | [g0v/moedict-data 重編國語辭典](https://github.com/g0v/moedict-data) | ~16万 | CC-BY-ND 3.0 TW(本地用,不入 git) |

## Schema 草案

```
chars.tsv     <char>\t<unicode>\t<radical>\t<canonical_reading>\t<tier_from_table>\t<note>
              tier: 1=通用一级, 2=二级, 3=三级 (跟 10-tier 模型直接对齐)

readings.tsv  <char>\t<reading>\t<tone>\t<rank>\t<tier_offset>\t<note>
              rank: primary / secondary / literary / colloquial / archaic
              tier_offset: 主读 0, 副读 +1, 文读 +1, 古音 +3

words.tsv     <code>\t<word>\t<reading_path>\t<tier>\t<source>\t<note>
              reading_path: 用哪个 char 的哪个 reading 拼出来,如:
                还有     [还|hái][有|yǒu]
                还书     [还|huán][书|shū]
              tier: 从 HSK 表 + 字层平均 + 词长推算 (1-9)
              source: hsk / cedict / polish-A
```

**字字直拼噪音天然不可能**: words.tsv 行必须能解出 valid reading_path,corpus 灌的「五六」「正于」根本进不来。

**freq 不存数值,只存 tier**(用户 2026-06-29 共识)— corpus 在新模型里**只作 tier 内排序兜底**(过渡期),后续被 runtime personal-freq 接管。

## Wire 机制(Phase 0 已落地)

`core/crates/inputx-pinyin-v2/` 是 v2 crate,跟 v1 (`inputx-pinyin`) **并存**。切换通过环境变量:

```sh
INPUTX_PINYIN_VERSION=v2    # 走 v2 (Phase 0 stub: 返回空候选)
unset / =v1                  # 走 v1 (default)
```

Wire 点: `core/crates/inputx-core/src/composite/pinyin_adapter.rs::refresh_candidates()` 顶部 if `inputx_pinyin_v2::enabled()` 早返回。每次 baseline 必须双跑 (默认 + `INPUTX_PINYIN_VERSION=v2`),v1 不允许回退,v2 进展从 stub 逐步推进。

## Phases

| Phase | 内容 | 状态 |
|---|---|---|
| 0 | v2 crate 骨架 + wire 通路 + INPUTX_PINYIN_VERSION env 控制 | **DONE 2026-06-29** |
| 0.5 | 多路 config (CLI flag / env / 2 config file 路径) | **DONE 2026-06-29** |
| 1 | chars + readings 表 ingest 8105 字 / 12149 readings | **DONE 2026-06-29** |
| 2 | words 表 ingest 88,097 词 (CC-CEDICT + HSK 2.0 tier overlay) | **DONE 2026-06-29** |
| 3 | v2 Path-1 lookup (exact-code words + single-syllable chars) | **DONE 2026-06-29** |
| 4 | v2 path 1c initials reverse-lookup (wsm → 为什么) | **DONE 2026-06-29** |
| 5 | v2 path 5 multi-syllable composition (greedy longest-prefix) | **DONE 2026-06-29** |
| 6 | tier 内 ranking — HSK 字表 overlay (muscle memory) | **DONE 2026-06-29** |
| 6.5 | personal freq runtime overlay | (deferred) |
| 7 | v1 → v2 切换为 default,v1 转 deprecation | next |
| 8 | v1 crate 删除,corpus-digest pipeline 退役 | |

## 不在 scope

- `composite/merge.rs` / `composite/scoring.rs` / `engine_weights.toml` — 跨引擎 / 排序公式 / tier 编号语义,**不动**
- wubi / nihongo 引擎 — **不动**
- baseline test — 仅可加新 test 测 v2 路径,不可改 v1 测试预期
