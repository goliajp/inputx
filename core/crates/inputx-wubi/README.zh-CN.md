# inputx-wubi

[![Crates.io](https://img.shields.io/crates/v/inputx-wubi.svg)](https://crates.io/crates/inputx-wubi)
[![npm](https://img.shields.io/npm/v/@goliapkg/wubi.svg)](https://www.npmjs.com/package/@goliapkg/wubi)
[![docs.rs](https://docs.rs/inputx-wubi/badge.svg)](https://docs.rs/inputx-wubi)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#许可)

用 Rust 编写的五笔字型（Wubi 86）中文输入法引擎，原生支持 WebAssembly。
编码器、内嵌字典和用户偏好学习层组合在一个自完备的库中。

驱动 **[Inputx](https://github.com/goliajp/inputx) IME** 的五笔引擎；
本 crate 是独立可复用的 Wubi 引擎，对外也单独发布到 crates.io / npm，
方便其它项目使用一份干净、许可宽松的五笔栈。

**语言**：[English](README.md) · 简体中文 · [日本語](README.ja.md)

## 特性

- 完整的五笔 86 编码器，覆盖四种规范分解规则
- 135,822 条字典通过有限状态转换器 (FST) 内嵌进二进制
- 两层排序——基于语料的逐条频率 (L1+) 加上用户可变覆盖层 (L0)，
  带 3-pick 自动升级规则
- Layer prefs——宿主可调的层级权重乘子
- 可重现的权重生成管线，CI 字节级 diff 校验
- 纯 Rust，库代码零 `unsafe`，`no_std + alloc` 兼容
- WebAssembly 绑定向浏览器和 Node 暴露相同 API

## 安装

### Rust

```toml
[dependencies]
inputx-wubi = "1.0"
```

### JavaScript / TypeScript

```sh
npm install @goliapkg/wubi
```

## 使用

### Rust

```rust
use wubi::WubiDict;

let dict = WubiDict::embedded();

let candidates = dict.lookup("khlg");
// ["中国", "跨国", "跑车", ...]

// 通知字典：用户选了某个候选。同一 (code, word) 被选 3 次后，自动升为 L0 默认。
dict.record_pick("khlg", "跑车");
```

> crates.io 上的包名是 `inputx-wubi`，但 lib 名保留为 `wubi`，
> 所以代码里直接 `use wubi::...` 即可。

### JavaScript

```js
import init, { WubiEngine, Layer } from "@goliapkg/wubi";

await init();
const eng = new WubiEngine();

eng.lookup("khlg");                        // ["中国", "跨国", "跑车", ...]
eng.recordPick("khlg", "跑车");
eng.setLayerPref(Layer.Phrase, 1.5);

const state = eng.exportL0();
localStorage.setItem("wubi-l0", JSON.stringify(state));
```

## 排序模型

每个候选词的展示分数：

```
displayed_score = LAYER_BASE[layer] × layer_prefs[layer] + freq_score
```

每个 code 的候选返回顺序：

1. 若 L0 有该 code 的 pin，对应词放到位置 0
2. 其余候选按 `displayed_score` 降序排列

**层级**（优先级升序）：Auto、Phrase、Zigen、Jianma3、Jianma2、Jianma1。

**L0 升级规则**：`record_pick(code, word)` 递增 `(code, word)` 计数器。
达到阈值（默认 3，可在编译时通过 `WUBI_PROMOTE_THRESHOLD` 覆盖）时，
该词成为该 code 的 L0 pin，并清空该 code 下所有计数器——后续若有
不同的词想要取代，需要重新攒满 3 次。

## 性能

Apple Silicon，release 构建：

| 操作 | 延迟 |
|---|---|
| `lookup_zigen('王')` | 7.2 ns |
| `lookup_jianma1(b'g')` | 6.6 ns |
| `encode_into(decomp)` | 8–11 ns |
| `dict.lookup("g")`（1 个候选） | 266 ns |
| `dict.lookup("gggg")`（6 个候选） | 597 ns |
| `dict.lookup("zzzz")`（未命中） | 142 ns |
| `dict.record_pick(code, word)` | 674 ns |
| `dict.export_l0()`（L0 为空时） | 71 ns |
| `dict.prefix("g")`（约 5,000 个匹配） | 1.45 ms |

## 许可

双许可：MIT（[`LICENSE-MIT`](LICENSE-MIT)）+ Apache 2.0
（[`LICENSE-APACHE`](LICENSE-APACHE)）© 2026 GOLIA K.K.

字典结构源自公开的五笔 86 标准（王永民，1986）。频率权重派生自：

- **Leipzig Corpora Collection** — `zho_wikipedia_2018_1M` 和
  `zho_news_2020_100K`（均为 CC-BY 4.0）。
- **SUBTLEX-CH-WF** — Cai, Q. & Brysbaert, M. (2010). *SUBTLEX-CH:
  Chinese Word Frequencies Based on Film Subtitles.* PLOS ONE 5(6),
  e10729. CC-BY 4.0.

发布物中只包含派生的数值分数，不再分发任何源语料文本。
