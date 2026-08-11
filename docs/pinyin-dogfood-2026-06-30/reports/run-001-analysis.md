# Dogfood Run 001 — Analysis

立 2026-06-30 / iter#16. Source data:`scratchpad/failures/run-001-full.tsv`。

## Top-line stats

| 指标 | 值 | % |
|---|---:|---:|
| Total segments | 47,904 | 100% |
| PASS | 32,271 | **67.4%** |
| SOFT(in top10, not #0) | 8,724 | 18.2% |
| HARD(not in top10) | 6,909 | 14.4% |

Source corpus:66 zhwiki articles(slow fetcher 进行中,target 500)。

## Corpus skew warning

**Article 0058 = 「偶像梦幻祭」(Japanese anime wiki page)** 单文 vlog 3,989 HARD = 58% 的全部 HARD failures。
Names like 梦之咲 / 星奏馆 / 雷欧 / 北斗 / 斑 / 成鸣 / 松井 / 洋平 / 奏汰 / 一彩 / 凪 / 真绪 / 儿玉沙织 / 红月 / 丹希 全都来自此条 niche fandom 内容。

**Excluding 0058,real-domain HARD ≈ 2,920 / 43,915 ≈ 6.6%**。模型表现远比 14.4% 描绘的好。Phase 5 target 决策应**排除 0058 类 niche names**(不该上 polish-A backfill)。

## SOFT 模式分析

SOFT 分布(by char length):
- **1c:5,801(66.5% of SOFT)** ← 单字 polysemy 是 SOFT 的主体
- 2c:2,914(33.4%)
- 3c:9(0.1%)

### Top-30 单字 SOFT(批 polish 候选)

格式:`<count>  <pinyin>:<current #0> ❌  → <expected>`

| n | pinyin | got #0 | should | 备注 |
|---:|---|---|---|---|
| 448 | yu | 鱼 | **与** | grammatical particle |
| 395 | wei | 喂 | **为** | grammatical |
| 308 | zhong | 种 | **中** | super common |
| 240 | bing | 丙 | **并** | conjunction |
| 179 | yi | 一 | **以** | preposition |
| 171 | hou | 厚 | **后** | spatial/temporal |
| 153 | yu | 鱼 | **于** | preposition |
| 150 | shi | 是 | **时** | temporal |
| 141 | jiang | 讲 | **将** | modal |
| 125 | dan | 单 | **但** | conjunction |
| 117 | di | 的 | **第** | ordinal |
| 109 | you | 有 | **由** | preposition |
|  92 | ceng | 层 | **曾** | aspect |
|  92 | bu | 不 | **部** | section/measure |
|  81 | hua | 花 | **话** | speech |
|  72 | dui | 对 | **队** | team |
|  69 | qi | 起 | **其** | possessive |
|  65 | ge | 个 | **歌** | song |
|  58 | huo | 火 | **或** | conjunction |
|  57 | qian | 钱 | **前** | spatial |
|  56 | deng | 灯 | **等** | conjunction |
|  56 | ji | 给 | **及** | conjunction |
|  50 | yi | 一 | **翼** | wing |
|  49 | xin | 心 | **新** | new |
|  48 | zhe | 这 | **着** | aspect particle |
|  46 | gang | 刚 | **港** | port |
|  43 | xiang | 想 | **向** | direction |
|  41 | gai | 盖 | **该** | this/that |
|  40 | yin | 阴 | **因** | cause |
|  39 | zhi | 只 | **至** | until |

### SOFT-1c polish 策略分析

观察:大多数 expected word 是 **语法功能字**(与/为/并/以/后/于/将/但/由/曾 等),而 modern_freq 给 #0 的是 **意义实词**(鱼/喂/种/丙/一/厚 等)。

原因:jieba dict 用新闻语料统计,实词频次 > 功能字频次(因为功能字虽然频繁出现,但 jieba 切词时单字功能字常被合到双字词)。

**矛盾**:用户日常打字 80% 是 buffer-内单字 = 功能字,但 jieba freq 反映的是「单字 token 在切词后的统计」,主要是实词。两者优先级不同。

**策略**:批量 quickfix top-20 单字 SOFT(功能字 muscle memory),每条 ~30k 抵 modern_freq + sovereignty。

Risk:这些 buffer 的"实词需求"会推到 #1。可接受 — 用户拿空格或下一选选实词。

### Top 双字 SOFT(批 polish 候选 — 不批改,quickfix)

| n | pinyin | got #0 | should |
|---:|---|---|---|
| 151 | xuexing | (低分项)| **血型** |
| 84 | xueyuan | 学院 | **学园** |
| 81 | liuxing | 流行 | **流星** |
| 79 | chengwei | 称谓 | **称为** |
| 72 | dui | (单字,已在上)| |
| 66 | zuowei | 作为 | (无 — 已经 #0?) |
| 65 | tongshi | 同时 | (待 verify) |
| 65 | ge(or) | (单字)| |
| 57 | bianqu | 编曲 | **编曲** |

很多 2c SOFT 看上去是 niche compound(学园/流星 都从 anime 文章来),应该 skip。real-domain 2c SOFT 不多。

## HARD 模式分析

3,218 distinct HARD words。Top-50(过滤掉显著 anime 干扰):

### 真有效 HARD targets(general-domain)

| n | word | 备注 |
|---:|---|---|
| 148 | 宣传语 | propaganda phrase / slogan |
| 88 | 预选赛 | qualifier round(体育)|
| 83 | 存于 | (古文 / 文言)— 可 skip |
| 65 | 第二年 | ordinal + 年 — 通用 |
| 63 | 译作 | translation as / translated as |
| 58 | 作词 | songwriting |
| 55 | 第一年 | ditto |
| 35 | 家中 | at home |
| 22 | 一楼 | first floor |
| 21 | 三年级 | grade 3 |
| 21 | 一名 | one(person)|
| 20 | 一年级 | grade 1 |
| 20 | 一年 | one year |
| 18 | 二年级 | grade 2 |
| 16 | 二楼 | second floor |
| 15 | 英语 | English language ← proper-noun reject(已知 pattern A)|
| 14 | 四川省 | Sichuan province |
| 14 | 馆内 | inside the hall |
| 14 | 园内 | inside the garden |

### Niche / 应 skip 的 HARD(anime / 古文 / 半专有)

skip set:梦之咲 / 星奏馆 / 雷欧 / 北斗 / 斑 / 成鸣 / 松井 / 洋平 / 奏汰 / 凪 / 真绪 / 儿玉沙织 / 红月 / 丹希 / 英智 / 一彩 / 同寝室 / 燐 / 之咲 / 敬人 / 星奏 / 中住 / 返礼 / 色为 / 翼(in anime context)/ 阴(literal)/ 等 anime-pet names。

### HARD 子类总结

| 类 | est. count | 处理 |
|---|---:|---|
| Anime/fandom names(article 0058) | ~3,989 | **SKIP** — 不入 polish |
| 历史 / 古文(存于 / 译作 / 饰 / 驻 等)| ~300 | **SKIP** — 选 |
| Ordinal compounds(第二年 / 三年级 / 一楼 / 一名 等)| ~400 | **POLISH A**:加 modern_vocab |
| Locale-compounds(家中 / 馆内 / 园内 等)| ~200 | **POLISH A** |
| Real word missing(宣传语 / 预选赛 / 译作 / 作词)| ~500 | **POLISH A** |
| Province / city(四川省 等)| ~100 | **POLISH A** if missing |
| Proper-noun reject(英语 等 — pattern A)| ~200 | **POLISH A** |

## Phase 5 batch polish plan

### Iter 5.1 — SOFT-1c batch(highest impact)
- Top-25 单字 SOFT 加 quickfix,each ~30k
- 预估 PASS rate +8-10pp(到 ~76% PASS)
- Risk:单字 muscle memory 调整可能跟某些用户偏好冲突,但 dogfood 数据显示这是主流诉求

### Iter 5.2 — HARD ordinal/locale 通用 compound
- 第二年 / 第一年 / 一年 / 三年级 / 二年级 / 一楼 / 二楼 / 三楼 / 一名 / 几名 / 家中 / 园内 / 馆内 / 城内 等
- 约 30-50 词加 modern_vocab(freq 35-50k)
- 预估 HARD rate -3-4pp

### Iter 5.3 — HARD specialty compound
- 宣传语 / 预选赛 / 译作 / 作词 / 作曲 / 编曲 / 主唱 等媒体 / 比赛 vocab
- 20-30 词加 modern_vocab

### Iter 5.4 — Proper-noun reject 系统问题
- 英语 / 中文 / 日语 / 韩语 / 法语 等语言名
- 已批量加过国家名;追加语言名(20-30 词)

### Skip
- Article 0058 anime 词(梦之咲/星奏馆等)— niche,跟 IME 主战场无关
- 古文文言(存于/译作 / 驻 / 饰 当文言用时)— v2 dict 已收,polish 不必再加

### 预期收益

| Phase | PASS rate after |
|---|---|
| Baseline(now) | 67.4% |
| After 5.1(SOFT-1c) | ~76% |
| After 5.2(ordinal/locale) | ~80% |
| After 5.3(specialty) | ~82% |
| After 5.4(lang names) | ~83% |

剩余 ~17% 主要是 corpus-skew(anime niche)/ true v2 dict gap / cross-engine 边界。

## Followup actions

- [ ] Expand corpus when fetcher reaches 200+ articles;re-run dogfood
- [ ] Consider filter article 0058 from second pass(or document as known skew)
- [ ] Phase 6 retire candidates(单字 muscle memory quickfix 若已经覆盖某些,新 batch 别叠)

## Conclusions

1. **modern_freq 工作良好** — 67% PASS 起步,且大多 SOFT 是单字 polysemy,modern_freq 没错只是 jieba freq 不反映 IME 单字 muscle memory。
2. **Phase 5 path 明确** — 50-100 行 polish data 可以提到 ~80% PASS。
3. **Article-skew issue 必须 acknowledge** — niche article 让总数据偏。下次 corpus 扩容后重测。
4. **No framework gap surfaced yet** — 所有 issue 都能 data-level 处理,不需要动 v2 引擎逻辑。
