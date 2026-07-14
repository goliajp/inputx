# v2 non-idiom backfill — 2026-07-14

v2 的 words.tsv 来自 CC-CEDICT + HSK ingest，**从未吸收 v1 语料词库**。
成语那一块已在 `docs/pinyin-v2-chengyu-2026-07-14/` 补完；这次补非成语词。

## Scope（不设阈值）
v1 library 里 v2 缺失的**全部非四字词**：135,579 条
- 2 字 73,044 / 3 字 58,076 / 5+ 字 4,459
- freq 100–47,067，低频冷僻一并过

## 方法
1. 272 个 reviewer agent × 500 行，**逐行** RECALL/KEEP（rubric 见 RUBRIC.md）→ 15,185 条提案，覆盖 135,579/135,579
2. **lead 逐行终审全部 15,185 条提案** → 拒 104（rejects.tsv）
3. (code,word) 去重取 max freq → **入库 15,081 条** @ 15000（tier 4）

## Lead 终审拒收的 4 类硬伤（agent 一审漏网）
1. **儿化 r 码**（`dager`/`weir`/`yidianr`…）— v2 reading_path 每字一音节，「儿」只能是 `er`，r 形对不齐。有 er 形的保留 er 形；无 er 形的整条不补（v1 仍可打）。
2. **多音字错码** — 藏(zang地名/cang收藏)：藏獒/藏区/川藏线/青藏线/西藏自治区/雅鲁藏布江；长(chang/zhang)：长春电影制片厂/长江口/长途车；血(xue医学词/xie口语)：胃出血/血常规；另有 蚌埠/贝勒/北朝鲜/羹匙/牛仔(zai)。
3. **歧义切分码** — `diao`(迪奥 di-ao)、`guai`(骨癌 gu-ai)、`piao`(皮袄 pi-ao)：零声母音节连写会被切成另一个合法音节，撞 掉/拐/票。
4. **同码重复的非规范形** — 奥德萨/布莱梅/俄克拉荷马/谍中谍/牡羊座/函式（台湾用语）。

## 与既有 polish 的冲突（已处理）
backfill 引入的 **汴京**、**发疹** 挤掉了用户此前拍板的排序。用户拍板的顺序是硬约束 →
`quickfix_boost.tsv` chain-boost 钉回：`bianjing` 边境#0/辩经#1（汴京退#2）、`fazhen` 法阵#0（发疹退#1）。
新词保留，只是排在后面。

## 验证
- `make polish-rebuild`：scoring 21/21、baseline 114/114、lib 399/0
- 回归 test：`v2_nonidiom_backfill_representatives`（脑洞/潮汕/澡堂子/美男子 + 冲突 guard）
