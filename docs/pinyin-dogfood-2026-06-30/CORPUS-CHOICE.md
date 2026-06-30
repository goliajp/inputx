# Corpus 选源决定

立 2026-06-30 / iter#8.

## 选 THUCNews

| 维度 | 值 |
|---|---|
| 名称 | THUCNews(清华大学中文文本分类数据集) |
| 来源 | 新浪新闻(2005-2011) |
| 规模 | 836k 篇,~2.19GB raw / ~700MB zip |
| 分类 | 14 类:财经 / 彩票 / 房产 / 股票 / 家居 / 教育 / 科技 / 社会 / 时尚 / 时政 / 体育 / 星座 / 游戏 / 娱乐 |
| 授权 | 学术使用,公开下载,无 auth |
| 官方 | http://thuctc.thunlp.org/source/THUCNews.zip |

## 为什么不选别的

| 备选 | 不选原因 |
|---|---|
| HuggingFace dataset | `pip install datasets` 没装,且网络下载 dataset 各种 zip 风险高 |
| Wikipedia 中文 | `pip install wikipedia` 没装;且 wiki 是百科非新闻 |
| Common Crawl | 太大,filter Chinese subset 复杂 |
| Web scrape 最新新闻 | 法务灰、爬虫脆、source 多变 |

## 时效性疑虑 + 化解

THUCNews 2005-2011 看似不"最新",但:
1. **中文核心词汇 95%+ 自 2005 起未变**(jieba dict 也是同期采集,跟 modern_freq 配套)
2. **新词覆盖弱**(微信 / 小程序 / 区块链 / 元宇宙 等 2015+ 词)是真问题但**有 fallback**:
   - polish overlay 已批量加现代词(国家 / 城市 / shijiebei 等)
   - dogfood failures 跑出来自然会暴露缺词 → Phase 5 polish 批补
3. **新闻文本天然偏正式**,适合考核词库 baseline。日常口语用法是另一个独立 axis,本次不主测。

## 抽样策略(用于 2.3)

1000 篇均衡:
- 每大类 ~71 篇(14 × 71 ≈ 994,凑 1000)
- 每类内随机选(seed 固定,可复现)
- 过滤:文章长度 ≥ 200 中文字 + 编码合规

## 下载 plan(2.2 用)

```sh
cd docs/pinyin-dogfood-2026-06-30/scratchpad/corpus
curl -L -o THUCNews.zip http://thuctc.thunlp.org/source/THUCNews.zip
# ~700MB. 5-10 分钟视网速.
unzip -q THUCNews.zip
# 出 14 个目录,每目录 X篇 .txt
```

Risk:download 5-10 分钟,可能超 /loop 3min interval。下个 fire 见到 [WIP] 同 item > 30 min ago → mark BLOCKED stuck。所以 2.2 我会 run download in foreground 一气完成,不分多个 fire。
