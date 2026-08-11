# inputx-pinyin

[![Crates.io](https://img.shields.io/crates/v/inputx-pinyin.svg)](https://crates.io/crates/inputx-pinyin)
[![npm](https://img.shields.io/npm/v/@goliapkg/pinyin.svg)](https://www.npmjs.com/package/@goliapkg/pinyin)
[![docs.rs](https://docs.rs/inputx-pinyin/badge.svg)](https://docs.rs/inputx-pinyin)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#许可)

用 Rust 编写的普通话拼音输入法引擎——音节分段器、模糊音、FST 字典、
L0 / L1+ 排序、WebAssembly 原生支持
（[`inputx-pinyin-wasm`](../inputx-pinyin-wasm/)）。

驱动 **[Inputx](https://github.com/goliajp/inputx) IME** 的拼音引擎；
本 crate 是独立可复用的拼音引擎，对外也单独发布到 crates.io / npm，
方便其它项目使用一份干净、许可宽松的拼音栈。

**语言**：[English](https://github.com/goliajp/inputx/blob/develop/core/crates/inputx-pinyin/README.md) · 简体中文 · [日本語](https://github.com/goliajp/inputx/blob/develop/core/crates/inputx-pinyin/README.ja.md)

## 内容

- **414,325 条 FST 词条**（`data/pinyin.fst`，约 9 MB）：
  - 44,357 条单字读音，来自 Unihan `kHanyuPinlu` + `kMandarin`
  - 369,968 条多字短语读音 —— 其中约 4.3 万条来自 pypinyin 的规范读音覆盖，
    余下长尾来自 Unihan 笛卡尔积
  - 105 条手工校对的多音字 collapsing 规则
    （`银行 → yinhang`、`重新 → chongxin`、`着陆 → zhuolu` 等）
- **分段器**——DP 枚举拼音缓冲区的所有合法切分
- **模糊音**——9 对可切换的辅音/元音容错（`z⇄zh`、`n⇄l`、`en⇄eng` 等）
- **L0 用户覆盖**——显式的 `(input, word)` pin，JSON
  序列化的快照支持跨会话持久化
- **流式前缀扫描**（`prefix_for_each`）——零分配 visitor 遍历 FST 中匹配
  前缀的所有条目，Inputx 用它在每次 keystroke 上做 partial-input
  联想（`zho → 中国`、`zhong → 中国 / 中华 / 中央` ……）

## 安装

### Rust

```toml
[dependencies]
inputx-pinyin = "1.0"
```

### JavaScript / TypeScript

```sh
npm install @goliapkg/pinyin
```

## 使用

### Rust

```rust
use inputx_pinyin::{PinyinEngine, PinyinDict};

let eng = PinyinEngine::new();
let dict = eng.dict();

// 精确音节查询（基于 FST）。
let cands = dict.lookup("zhongguo");
// → ["中国", "中过", ...]

// 流式前缀扫描——visitor 看到所有拼音以给定前缀开头的条目，
// 前期不分配 Vec。Inputx 在 IME 热路径上用它做联想候选。
dict.prefix_for_each("zho", |pinyin, word, freq| {
    println!("{pinyin} {word} (freq={freq})");
});

// 通知引擎用户选了某个词。只累加使用计数，不改变候选次序。
dict.record_pick("zhongguo", "中国");
```

## 性能

配套的 [Inputx IME](https://github.com/goliajp/inputx) 在
[perfgate 单元测试](https://github.com/goliajp/inputx/blob/develop/core/crates/inputx-core/src/composite/pinyin_adapter.rs)
里以「每次 keystroke 不超过一帧 16 ms (60 Hz)」为硬约束，
覆盖完整的候选刷新流水线（精确查、简拼、前缀扫描、过滤）。

引擎裸性能（Apple Silicon，release；跑 `cargo bench -p inputx-pinyin`）：

| 操作 | 延迟 |
|---|---|
| `dict.lookup("zhongguo")` 精确多候选 | ~530 ns |
| `dict.lookup_into("zhongguo", &mut buf)` 复用 buf | ~510 ns |
| `dict.lookup("ni")` 单音节大扇出 | ~7.3 µs |
| `dict.lookup("xxxxxx")` 未命中 | ~160 ns |
| `dict.prefix_for_each("zhong", _)` 4 字母前缀 | ~770 µs |
| `dict.prefix_for_each("z", _)` worst case（约 5 万条目） | ~4.8 ms |
| `dict.prefix_for_each_raw` 对比 `_for_each` | ~10% 更快 |
| `dict.prefix_exists("zhong")` 早 termination bool | < 100 ns |
| `dict.record_pick(input, word)` | < 1 µs |
| `segment("zhongguorenmin")` 4 音节 | < 2 µs |
| `encode::char_to_pinyin('中')` 反查 cache | < 50 ns |

## 工具（`--features tools`）

仅维护者用的二进制，从上游数据源重新生成 FST。库消费者不需要：

```sh
cargo run --features tools --release --bin unihan-extract-readings
cargo run --features tools --release --bin compose-phrase-readings
cargo run --features tools --release --bin pinyin-fetch-corpus
cargo run --features tools --release --bin pinyin-build-weights
cargo run --features tools --release --bin pinyin-build-fst
```

## 许可

引擎代码双许可：[MIT](LICENSE-MIT) **OR** [Apache-2.0](LICENSE-APACHE)
© 2026 GOLIA K.K.

### 内嵌数据及其许可

| 来源 | 许可 | 贡献 |
|---|---|---|
| **Unihan 数据库** | [Unicode 许可 v3](LICENSE-UNICODE) | 单字拼音读音（`kHanyuPinlu`、`kMandarin`） |
| **jieba** (`fxsjy/jieba`) | [MIT](LICENSE-JIEBA) | 约 34.9 万条普通话短语词典，作为短语读音 seed |
| **pypinyin** (`mozillazg/python-pinyin`) | [MIT](LICENSE-PYPINYIN) | 约 4.7 万条手工校对的规范短语读音 |
| **Leipzig Corpora Collection** | CC-BY 4.0 | `zho_wikipedia_2018_1M`、`zho_news_2020_100K`，提供频率权重 |
| **SUBTLEX-CH-WF** | CC-BY 4.0 | 来自电影字幕的口语 register 频率权重 |

发布物只包含派生出来的 FST 和整数频率分数，不再分发任何源语料文本。
