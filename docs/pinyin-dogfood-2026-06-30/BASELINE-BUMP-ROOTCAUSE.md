# Baseline single-char bump 根因分析(2026-07-06)

## 症状

本 dogfood session 反复触发 baseline 回归:

- `ya → 呀` 期望 top1,实测 `ya → 亚`
- `er → 而` 期望 top1,实测 `er → 二`

累计触发 8+ 次(K396,K414,K424,K426,K436,K481,K485,K487,K489,K506)。

## 根因

`tools/scoring/data/polish/modern_vocab_v1.tsv` 里加入含 `亚`/`二` 的复合词条时,`idf-from-pinyin-dict.rs` 在生成 `words.idf` 时会**间接抬高**这些字的单字权重(通过 jieba corpus 词表的字级 freq 聚合路径)。

- 单字 `呀` / `而` 的语料 freq 相对稳定
- 每加一条含 `亚` 的复合词(如 `亚太区`、`周亚宁`、`叙利亚人`),累积字级抬升 ~1 个数量级不明显
- 但**累计** 138 条 `亚`-entries + 141 条 `二`-entries 后,`亚` / `二` 的聚合 freq 已经**濒临**翻超 `呀` / `而`
- 单条边缘 add(如 `第二层`、`玉置浩二`、`亚马尔`)就会 tip the scale

## 表现模式

每次触发时,失败测试是同一个:`composite::baseline_quality_test::tests::baseline_pinyin_only_single_syllable`。
top10 排列本身没大变(`ya` 都是 [亚,呀,压,雅,牙,鸭,崖,押,哑,芽]),只是 top1 flip。

## 已采取措施

**本 session 已 revert 的 `亚`/`二` entries**:
- K396: 瓦伦西亚, 迪亚斯, 索菲亚(3 词)
- K414: 周亚卫
- K424: 博尔基亚, 鲁尼亚
- K426: 亚马尔
- K436: 瓦伦西亚,迪亚斯,索菲亚(article title itself)
- K481: 叙利亚人 (delayed detection — broke K482/K483)
- K485: 周亚宁 (pre-blocked)
- K487: 玉置浩二 (延迟检测)
- K489: 二强 (immediate)
- K506: 第二层 (延迟检测,K507 才发现)

## 结构层预防

1. **强制 pre-push gate**: 每次 `commit + push` 前必须跑 `cargo test -p inputx-core --lib baseline --release`。本 session 跳过了~30 次,是 delayed-detection 的直接原因。
2. **Polish 工具加校验**: `tools/strict-dogfood/process_one.py` 的 `audit_and_fix()` 里,对含 `亚`/`二`/`马` 等 protected-char 的 candidate 显式提示 baseline 风险;或直接 hard-skip(次好方案 — 会漏真词)。
3. **Add-then-verify 循环**: audit 循环里,应该:add → build → run baseline → 若挂,自动 revert 最近 batch。
4. **升级 baseline test 到 pre-commit hook**:项目已有 CI,但本地 pre-commit 缺,导致 push 后才发现。

## 已知的 "protected" 单字 pinyin (top1 稳定性关键位)

- `ya` → 呀 (dominant challenger: 亚)
- `er` → 而 (dominant challenger: 二)
- 其他可能类似的:`ma` (呀/嘛/妈 vs 马)、`la` (啦/拉)、`ba` (吧/巴)、`na` (呐/那) — 未测,建议加入监测。

## 应用规则(operational)

作为 dogfood polish 工作的 hard rule:

- **每篇文章 push 前必须跑一次 baseline test**。若失败:
  - 先 identify 本 batch 里的 `亚`/`二` adds
  - Revert 该词,split 到 segments TSV
  - Re-test,直到 baseline 绿
  - 再 commit + push

- **禁止连续 push 多篇文章不跑 baseline**。这次 delayed-detection 的原因就是 K481 → K487 push 之间没跑,直到 K488 才发现 5 篇的累积漂移。
