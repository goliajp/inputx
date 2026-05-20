# @goliapkg/pinyin (WASM)

[`inputx-pinyin`](../inputx-pinyin/) のブラウザ / Node バインディング。
自作の普通話拼音入力エンジン (セグメンタ、ファジー音節、FST 辞書、
L0 ユーザー学習)。約 100 KB の WebAssembly モジュールに事前ビルド
(デフォルトは bootstrap 辞書を組み込み、完全な 9 MB 辞書は build
feature で切替可能)。

[Inputx IME](https://github.com/goliajp/inputx) のウェブ面。

**ライセンス:** MIT OR Apache-2.0。

**言語**: [English](README.md) · [简体中文](README.zh-CN.md) · 日本語

## インストール

```sh
npm install @goliapkg/pinyin
```

## 使い方

```js
import init, { PinyinEngine } from "@goliapkg/pinyin";

await init();                              // .wasm をロード + インスタンス化
const eng = new PinyinEngine();

// 完全一致音節ルックアップ
console.log(eng.lookup("zhongguo"));       // ["中国", "中过", ...]

// 前方一致補完 (partial-input IME 候補)
const hits = eng.prefix("zho");
hits.slice(0, 5).forEach(h => console.log(h.pinyin, h.word));

// ユーザーが候補を選択 → エンジンが学習。3 回で L0 自動ピン。
eng.recordPick("zhongguo", "中国");

// 漢字 → 拼音逆引き
console.log(eng.encode("中"));             // "zhong"

// 永続化 — 保存先は呼び出し側
const state = eng.exportL0();
localStorage.setItem("pinyin-l0", JSON.stringify(state));
// あとで:
eng.importL0(JSON.parse(localStorage.getItem("pinyin-l0") ?? "{}"));
```

## 含まれるもの

- `golia_pinyin_wasm_bg.wasm` — エンジン + bootstrap FST (約 100 KB)
- `golia_pinyin_wasm.js` — ES module ラッパー
- `golia_pinyin_wasm.d.ts` — TypeScript 型定義

完全な 41.4 万件辞書 (9 MB) は `bootstrap_only` feature を外して
リビルドする必要がある。下記の Build セクション参照。

## ソースからビルド

デフォルト (小さな bootstrap 辞書、バンドル全体で約 100 KB):

```sh
git clone https://github.com/goliajp/inputx
wasm-pack build core/crates/inputx-pinyin-wasm --target web --release
# 出力は core/crates/inputx-pinyin-wasm/pkg/
```

完全辞書 (約 9 MB、41.4 万エントリ):

```sh
wasm-pack build core/crates/inputx-pinyin-wasm \
    --no-default-features --target web --release
```

## 関連リンク

- ネイティブ Rust crate: [`inputx-pinyin`](../inputx-pinyin/)
- 親 IME プロジェクト: [Inputx](https://github.com/goliajp/inputx)
- 姉妹五筆エンジン: [`@goliapkg/wubi`](../inputx-wubi-wasm/)
