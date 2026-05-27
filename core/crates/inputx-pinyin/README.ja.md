# inputx-pinyin

[![Crates.io](https://img.shields.io/crates/v/inputx-pinyin.svg)](https://crates.io/crates/inputx-pinyin)
[![npm](https://img.shields.io/npm/v/@goliapkg/pinyin.svg)](https://www.npmjs.com/package/@goliapkg/pinyin)
[![docs.rs](https://docs.rs/inputx-pinyin/badge.svg)](https://docs.rs/inputx-pinyin)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#ライセンス)

Rust で書かれた普通話拼音入力エンジン。音節セグメンタ、ファジー音節、
FST バックエンドの辞書、L0 / L1+ ランキング、WebAssembly を
ファーストクラスでサポート (姉妹 crate
[`inputx-pinyin-wasm`](../inputx-pinyin-wasm/))。

**[Inputx](https://github.com/goliajp/inputx) IME** の拼音エンジン
として動作する一方、独立した再利用可能ライブラリでもあり、crates.io /
npm に単独公開している。寛容なライセンスの普通話拼音スタックを必要と
する他プロジェクトもそのまま利用できる。

**言語**: [English](https://github.com/goliajp/inputx/blob/develop/core/crates/inputx-pinyin/README.md) · [简体中文](https://github.com/goliajp/inputx/blob/develop/core/crates/inputx-pinyin/README.zh-CN.md) · 日本語

## 内容

- **414,325 件の FST エントリ** (`data/pinyin.fst`、約 9 MB):
  - 44,357 件の単漢字読み (Unihan `kHanyuPinlu` + `kMandarin` 由来)
  - 369,968 件の多漢字フレーズ読み — そのうち約 4.3 万件は pypinyin の
    正規化された読みでオーバーライド、残りのロングテールは Unihan の
    直積で生成
  - 105 件の手作業で校正された多音字 collapsing ルール
    (`银行 → yinhang`、`重新 → chongxin`、`着陆 → zhuolu` ……)
- **セグメンタ** — 拼音バッファのすべての合法分割を DP で列挙
- **ファジー音節** — 9 組の切替可能な子音/母音の許容 (`z⇄zh`、`n⇄l`、
  `en⇄eng` など)
- **L0 ユーザー学習** — 同じ `(input, word)` を 3 回選ぶと自動 pin、
  JSON シリアライズ可能なスナップショットでセッション横断永続化
- **ストリーミング前方一致スキャン** (`prefix_for_each`) — 与えた
  prefix にマッチするすべての FST エントリをアロケーションなしで
  visitor 経由で渡す。Inputx の IME 中で keystroke 毎の partial-input
  補完 (`zho → 中国`、`zhong → 中国 / 中华 / 中央` ……) に使用

## インストール

### Rust

```toml
[dependencies]
inputx-pinyin = "1.0"
```

### JavaScript / TypeScript

```sh
npm install @goliapkg/pinyin
```

## 使い方

### Rust

```rust
use inputx_pinyin::{PinyinEngine, PinyinDict};

let eng = PinyinEngine::new();
let dict = eng.dict();

// 完全一致音節ルックアップ (FST バックエンド)。
let cands = dict.lookup("zhongguo");
// → ["中国", "中过", ...]

// ストリーミング前方一致スキャン — 拼音が与えた prefix で始まる
// すべてのエントリを Vec のアロケーションなしで visitor に渡す。
// Inputx の IME ホットループで補完候補生成に使われている。
dict.prefix_for_each("zho", |pinyin, word, freq| {
    println!("{pinyin} {word} (freq={freq})");
});

// ユーザーが候補を選んだことをエンジンに通知。同じ (input, word) が
// 3 回選ばれると、その input の L0 ピンに自動昇格する。
dict.record_pick("zhongguo", "中国");
```

## 性能

姉妹プロジェクトの [Inputx IME](https://github.com/goliajp/inputx) では
[perfgate ユニットテスト](https://github.com/goliajp/inputx/blob/develop/core/crates/inputx-core/src/composite/pinyin_adapter.rs)
で「keystroke ごとに 1 フレーム 16 ms (60 Hz) を超えない」を
ハード制約とし、候補リフレッシュのフルパイプライン (完全一致、
简拼、prefix scan、フィルタ) を計測している。

エンジン単体の性能 (Apple Silicon、release ビルド; `cargo bench -p inputx-pinyin`):

| 操作 | レイテンシ |
|---|---|
| `dict.lookup("zhongguo")` 完全一致・多候補 | ~530 ns |
| `dict.lookup_into("zhongguo", &mut buf)` バッファ再利用 | ~510 ns |
| `dict.lookup("ni")` 単音節・多候補 | ~7.3 µs |
| `dict.lookup("xxxxxx")` ミス | ~160 ns |
| `dict.prefix_for_each("zhong", _)` 4 文字 prefix | ~770 µs |
| `dict.prefix_for_each("z", _)` 最悪ケース (約 5 万エントリ) | ~4.8 ms |
| `dict.prefix_for_each_raw` (`_for_each` 比) | ~10% 高速 |
| `dict.prefix_exists("zhong")` 早期終了 bool | < 100 ns |
| `dict.record_pick(input, word)` | < 1 µs |
| `segment("zhongguorenmin")` 4 音節入力 | < 2 µs |
| `encode::char_to_pinyin('中')` 逆引きキャッシュ | < 50 ns |

## ツール (`--features tools`)

メンテナ専用のバイナリ。上流データソースから FST を再生成する場合に
のみ使用。ライブラリ利用者は不要:

```sh
cargo run --features tools --release --bin unihan-extract-readings
cargo run --features tools --release --bin compose-phrase-readings
cargo run --features tools --release --bin pinyin-fetch-corpus
cargo run --features tools --release --bin pinyin-build-weights
cargo run --features tools --release --bin pinyin-build-fst
```

## ライセンス

エンジンコードは [MIT](LICENSE-MIT) と [Apache-2.0](LICENSE-APACHE)
のデュアルライセンス © 2026 GOLIA K.K.

### 同梱データとそのライセンス

| ソース | ライセンス | 提供内容 |
|---|---|---|
| **Unihan データベース** | [Unicode License v3](LICENSE-UNICODE) | 単漢字の拼音読み (`kHanyuPinlu`、`kMandarin`) |
| **jieba** (`fxsjy/jieba`) | [MIT](LICENSE-JIEBA) | 約 34.9 万件の普通話フレーズ辞書 (フレーズ読み生成の seed) |
| **pypinyin** (`mozillazg/python-pinyin`) | [MIT](LICENSE-PYPINYIN) | 約 4.7 万件の手作業校正済み正規フレーズ読み |
| **Leipzig Corpora Collection** | CC-BY 4.0 | `zho_wikipedia_2018_1M`、`zho_news_2020_100K` — 頻度ウェイト |
| **SUBTLEX-CH-WF** | CC-BY 4.0 | 映画字幕由来の口語レジスタの頻度ウェイト |

配布物は派生 FST と整数頻度スコアのみを含み、ソースコーパスの
テキストは再配布されない。
