# @goliapkg/wubi (WASM)

[`inputx-wubi`](../inputx-wubi/) のブラウザ / Node バインディング。
自作の五筆字型 (Wubi 86) エンコーダと辞書、**L0 / L1+ ランキング**と
ユーザー自動学習を内蔵。約 3 MB の WebAssembly モジュールに事前ビルド
(13.5 万件の辞書を組み込み済み)。

[Inputx IME](https://github.com/goliajp/inputx) のウェブ面。

**ライセンス:** MIT OR Apache-2.0。

**言語**: [English](https://github.com/goliajp/inputx/blob/develop/core/crates/inputx-wubi-wasm/README.md) · [简体中文](https://github.com/goliajp/inputx/blob/develop/core/crates/inputx-wubi-wasm/README.zh-CN.md) · 日本語

## インストール

```sh
npm install @goliapkg/wubi
```

## 使い方

```js
import init, { WubiEngine, Layer } from "@goliapkg/wubi";

await init();                              // .wasm をロード + インスタンス化
const eng = new WubiEngine();              // μs オーダ、ヘッダ解析のみ

// L0/L1 ランク付きルックアップ
console.log(eng.lookup("khlg"));           // ["中国", "跑车", "跨国", ...]
console.log(eng.lookup("ipbf"));           // ["学"]

// 前方一致ルックアップ — IME 候補補完用
const hits = eng.prefix("g");
hits.slice(0, 5).forEach(h => console.log(h.code, h.word));

// ユーザーが候補を選ぶと辞書が学習。同じ (code, word) が 3 回選ばれると
// 自動的に L0 にピンされる。
eng.recordPick("khlg", "跑车");

// Layer prefs (上級者向け; デフォルト Auto = 0.7、他 = 1.0)
eng.setLayerPref(Layer.Phrase, 1.5);

// 永続化 — 保存先は呼び出し側が選択
const state = eng.exportL0();
localStorage.setItem("wubi-l0", JSON.stringify(state));
// あとで:
eng.importL0(JSON.parse(localStorage.getItem("wubi-l0") ?? "{}"));
```

## 含まれるもの

- `wubi_wasm_bg.wasm` — FST + ランキング + L0 ロジック (約 3 MB)
- `wubi_wasm.js` — ES module ラッパー
- `wubi_wasm.d.ts` — TypeScript 型定義
- 135,822 件の辞書エントリ (字根 / 简码 / 词组 / 自動分解 CJK)

## ソースからビルド

```sh
git clone https://github.com/goliajp/inputx
wasm-pack build core/crates/inputx-wubi-wasm --target web --release
# 出力は core/crates/inputx-wubi-wasm/pkg/
```

Node ターゲット: `wasm-pack build --target nodejs`。

## 関連リンク

- ネイティブ Rust crate: [`inputx-wubi`](../inputx-wubi/)
- 親 IME プロジェクト: [Inputx](https://github.com/goliajp/inputx)
- 姉妹拼音エンジン: [`@goliapkg/pinyin`](../inputx-pinyin-wasm/)
