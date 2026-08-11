# Phase 7 blocker — v2 ↔ v1 feature gap (2026-06-29)

**Status**: Phase 7「切换 v2 为 default」**不能直接落地**。dry-run
(`INPUTX_PINYIN_VERSION=v2` + 完整 baseline) 显示 58/357 tests
fail (16.2%).

Phase 0-6 已落地的 v2 能力:
- Path 1a exact words.tsv code 匹配
- Path 1b 单字 readings 匹配
- Path 1c initials 反查
- Path 5 greedy 多音节 composition
- HSK 字表 tier-internal ordering

Phase 7 阻塞原因: v2 暂未实现的 v1 path / 行为:

| 缺口类别 | 影响 tests | 备注 |
|---|---|---|
| **L0 user pins** | polish_xuxian / polish_weixin / polish_chakong / polish_edu | v1 用 cement L0 pin 用户敲过的词;v2 无 session-state |
| **fuzzy/typo** | lue_alias_resolves_to_lve / pinyin_real_fallback_composition | v1 有南方音 z/zh + lue→lve alias;v2 严格 reading_path |
| **prefix-completion** | prefix_partial_zho / prefix_single_letter_yields | v1 path 3 prefix completion;v2 无 |
| **polish overlay (tier_overlay.tsv)** | tier_overlay_lifts_juti / tier_overlay_demotes_mi_qiao | v1 读 polish/tier_overlay.tsv;v2 不读 |
| **quickfix_boost.tsv polish** | polish_log_relative_ordering / polish_log_tuidao_order | v1 读 polish/quickfix_boost.tsv;v2 不读 |
| **bigram context** | session_wang_yields_pinyin_candidates | v1 用 corpus bigram boost;v2 无 |
| **wubi 跨引擎 tier 5 测试** | wubi_pollution_tier5_demoted_absent_from_mixed_top3 | tier_overlay 影响 wubi 也间接影响 pinyin layout |
| **stress** | stress_10k_ops_no_quadratic | v2 path 不同,需重测 perf 不变性 |
| **corpus-derived edge cases** | polish_jianma_noise_removed_from_candidates / polish_kongdang_order | 各种 corpus 特定 case |

**Spot-check 验证 v2 polish 实际对**:
- `tuidao` v2 → `[推倒, 推导]` ✓ (推导 #1)
- `queshi` v2 → `[确实, 却是, 确是, 缺失]` ✓ (确实 #0)
- `wuliu` v2 → `[物流]` ✓ (0 五六)
- `zhengyu` v2 → `[整域]` ✓ (0 正宇/正于/证于)

v2 在 polish 报告涉及的 buffer 上**实际工作正确**。test FAILED 是因为
test 也要求**其它特定候选存在** (如 `polish_log_tuidao_order` 要求 退到
出现 — v2 reading_path resolve 没收 退到 这条 cedict 词,所以 absent).

**两条出路**:

(A) **v2 补 v1 path** — 实现 fuzzy / prefix / polish overlay 读取 /
bigram. 工程量 ≈ 1-2 周, 复刻 v1 大部分代码到 v2.

(B) **v2 ship as 实验性 opt-in** — 不切 default. 文档里标记
"INPUTX_PINYIN_VERSION=v2 启用试验",baseline 默认还跑 v1.
未来 v2 path 逐步补全后再切.

我会选 (B). 理由:
- v2 已经在 polish 报告涉及的核心 buffer 上**比 v1 更干净** (无 corpus 噪音)
- 但 v1 还有用户已经习惯的 muscle-memory 操作 (L0 pin / fuzzy 容错)
- 强切 default 会让用户体验突然退化 (这些容错都没了)
- 走 (A) 复刻路线是把 v1 复杂度搬进 v2,违背重写初衷
- 正确路径: v2 逐步加 path,**每加一个 path 看哪些 v1 测试它能 cover**,
  慢慢 retire v1 测试 / 转 v2-only 测试,等覆盖足够再切

## Phase 7 重定义

| 子 phase | 内容 | 评估 |
|---|---|---|
| 7a | v2 加 `polish/tier_overlay.tsv` + `polish/quickfix_boost.tsv` 读 | 1 天, 立刻拿回所有 polish 测试 |
| 7b | v2 加 prefix completion (path 3) | 2-3 天 |
| 7c | v2 加 fuzzy (path 1b consonant-prefix) | 2-3 天 |
| 7d | v2 加 L0 user pin session state | 3-4 天 |
| 7e | v2 加 bigram personal context | 2 天 |
| 7f | baseline test 全 pass on v2 → 切 default | gate |
| 8 | v1 删除 + corpus-digest 退役 | 1 天 |

Recommended next: **Phase 7a** (polish overlay 读) 立刻拿回最多分.

## 当前状态总结

- v2 已 DONE: Phase 0-6 (6 个 commits today)
- v2 内 32/32 unit tests ✓
- 默认 (v1) baseline 357/0 ✓
- `INPUTX_PINYIN_VERSION=v2` 走 v2:
  - 跟 polish 报告吻合的 buffer 全干净 ✓
  - baseline 58/357 fail = v1-specific path 还没复刻
- v2 ship 状态: **opt-in only**, 不切 default
