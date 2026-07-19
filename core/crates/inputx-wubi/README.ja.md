# inputx-wubi

[![Crates.io](https://img.shields.io/crates/v/inputx-wubi.svg)](https://crates.io/crates/inputx-wubi)
[![npm](https://img.shields.io/npm/v/@goliapkg/wubi.svg)](https://www.npmjs.com/package/@goliapkg/wubi)
[![docs.rs](https://docs.rs/inputx-wubi/badge.svg)](https://docs.rs/inputx-wubi)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#ライセンス)

Rust で書かれた五筆字型 (Wubi 86) 中国語入力エンジン。WebAssembly を
ファーストクラスでサポート。エンコーダ・組み込み辞書・ユーザー学習層を
1 つのライブラリにまとめている。

**[Inputx](https://github.com/goliajp/inputx) IME** の五筆エンジン
として動作する一方、独立した再利用可能ライブラリでもあり、crates.io /
npm に単独公開している。寛容なライセンスの五筆スタックを必要とする
他プロジェクトもそのまま利用できる。

**言語**: [English](https://github.com/goliajp/inputx/blob/develop/core/crates/inputx-wubi/README.md) · [简体中文](https://github.com/goliajp/inputx/blob/develop/core/crates/inputx-wubi/README.zh-CN.md) · 日本語

## 特徴

- Wubi 86 エンコーダ — 4 つの正規分解ルールに対応
- 135,822 件の辞書を有限状態トランスデューサ (FST) としてバイナリに埋め込み
- 二層ランキング — コーパス由来のエントリ別頻度 (L1+) と、明示的な
  ピンのみで駆動するユーザーオーバーライド層 (L0)
- Layer prefs — ホスト側でレイヤごとの倍率を調整可能
- 再現性のあるウェイト生成パイプライン、CI でバイト差分を検証
- 純粋な Rust、ライブラリコードに `unsafe` なし、`no_std + alloc` 互換
- WebAssembly バインディングはブラウザと Node に同じ API を提供

## インストール

### Rust

```toml
[dependencies]
inputx-wubi = "1.0"
```

### JavaScript / TypeScript

```sh
npm install @goliapkg/wubi
```

## 使い方

### Rust

```rust
use inputx_wubi::WubiDict;

let dict = WubiDict::embedded();

let candidates = dict.lookup("khlg");
// ["中国", "跨国", "跑车", ...]

// ユーザーが候補を選んだことを辞書に通知する。利用カウンタが増えるだけで、
// 候補の並び順は変わらない。
dict.record_pick("khlg", "跑车");
```

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

## ランキングモデル

各候補のスコアは以下で計算される:

```
displayed_score = LAYER_BASE[layer] × layer_prefs[layer] + freq_score
```

各 code について、候補は次の順で返る:

1. L0 にその code のピンがあれば、ピンされた語が 0 番目
2. 残りの候補は `displayed_score` の降順で並ぶ

**レイヤ** (優先度の昇順): Auto、Phrase、Zigen、Jianma3、Jianma2、
Jianma1。

**L0 カウンタ**: `record_pick(code, word)` は `(code, word)` のカウンタ
を 1 ずつ増やす。カウンタは利用統計にすぎず、**並び順には影響しない**。
候補順は辞書の判断で決まり、明示的な `pin` だけがそれを変更できる。
(自動ピン留めは 2026-07-20 に削除。)

## 性能

Apple Silicon、release ビルド:

| 操作 | レイテンシ |
|---|---|
| `lookup_zigen('王')` | 7.2 ns |
| `lookup_jianma1(b'g')` | 6.6 ns |
| `encode_into(decomp)` | 8–11 ns |
| `dict.lookup("g")` (候補 1 件) | 266 ns |
| `dict.lookup("gggg")` (候補 6 件) | 597 ns |
| `dict.lookup("zzzz")` (ヒットなし) | 142 ns |
| `dict.record_pick(code, word)` | 674 ns |
| `dict.export_l0()` (L0 が空) | 71 ns |
| `dict.prefix("g")` (約 5,000 件マッチ) | 1.45 ms |

## ライセンス

[MIT](LICENSE-MIT) と [Apache 2.0](LICENSE-APACHE) のデュアルライセンス
© 2026 GOLIA K.K.

辞書構造は公開された Wubi 86 標準 (王永民, 1986) に由来する。
頻度ウェイトの派生元:

- **Leipzig Corpora Collection** — `zho_wikipedia_2018_1M` と
  `zho_news_2020_100K` (いずれも CC-BY 4.0)
- **SUBTLEX-CH-WF** — Cai, Q. & Brysbaert, M. (2010). *SUBTLEX-CH:
  Chinese Word Frequencies Based on Film Subtitles.* PLOS ONE 5(6),
  e10729. CC-BY 4.0.

配布物には派生した数値スコアのみが含まれ、ソースコーパスのテキストは
再配布されない。
