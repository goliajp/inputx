# Inputx

[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#ライセンス)
[![inputx-wubi](https://img.shields.io/crates/v/inputx-wubi.svg?label=inputx-wubi)](https://crates.io/crates/inputx-wubi)
[![inputx-pinyin](https://img.shields.io/crates/v/inputx-pinyin.svg?label=inputx-pinyin)](https://crates.io/crates/inputx-pinyin)
[![@goliapkg/wubi](https://img.shields.io/npm/v/@goliapkg/wubi.svg?label=%40goliapkg%2Fwubi)](https://www.npmjs.com/package/@goliapkg/wubi)
[![@goliapkg/pinyin](https://img.shields.io/npm/v/@goliapkg/pinyin.svg?label=%40goliapkg%2Fpinyin)](https://www.npmjs.com/package/@goliapkg/pinyin)

> **言語**: [English](README.md) · [简体中文](README.zh-CN.md) · 日本語

プライバシーファーストの iOS 向け中国語入力メソッド (IME)。Rust で
書かれたデュアルエンジンコアを持つ。

Inputx は万能五笔 / 搜狗五笔と同じ方式で中国語を入力する —
**Wubi 86 をプライマリエンジン、Pinyin を自動フォールバック** として
動作するため、入力スキームを手動切替する必要がない。完全にオンデバイス
で動作：ネットワーク通信なし、アナリティクスなし、"フルアクセスを
許可" を要求しない。

## ハイライト

- **デュアルエンジン、切替なし** — Wubi 86 候補が先、Pinyin (全拼 +
  簡拼 + 部分入力 prefix 補完) が Wubi が空のときに補う。
- **オンデバイス学習 (L0)** — 同じ `(input → word)` を 3 回選ぶと
  自動的に先頭にピンされる。学習データは端末から出ない。JSON で
  ユーザーがエクスポート / インポート可能。
- **設計レベルでのプライバシー** — ネットワーク通信ゼロ、テレメトリ
  ゼロ、キーボードフルアクセスなし。収集されるデータは Apple 標準の
  クラッシュレポートが拾うもののみ。
- **Perfgate でガードされたキーストローク** — 候補リフレッシュは
  ユニットテストで 16 ms / 60 Hz 1 フレーム予算を強制する。入力の
  レイテンシは IME UX で最も致命的な問題として扱われる。
- **稀少 CJK 対応** — Plane-2+ 文字の表示をオプションで有効化、
  同梱の cascade フォント (`InputxCJKExtended`、Plangothic サブセット、
  OFL) 経由。
- **CJK タイポグラフィ** — 全角 / 半角切替、ASCII→CJK 句読点変換、
  状態を持つスマートクォート。
- **iOS ネイティブ UX** — Dynamic Type、ダークモード、VoiceOver、
  横向き再レイアウト、キー長押しバリアント、候補のソース表示。

## オープンソースエンジンエコシステム

2 つの言語エンジンとその WASM ラッパーは crates.io と npm に独立
公開されている。Inputx の iOS (ネイティブ) と Web (WASM) を駆動する
一方、MIT または Apache-2.0 のもとで他の Wubi / Pinyin プロジェクト
が自由に利用できる：

| コンポーネント | crates.io | npm |
|---|---|---|
| Wubi 86 エンジン | [`inputx-wubi`](https://crates.io/crates/inputx-wubi) | — |
| Wubi 86 WASM | [`inputx-wubi-wasm`](https://crates.io/crates/inputx-wubi-wasm) | [`@goliapkg/wubi`](https://www.npmjs.com/package/@goliapkg/wubi) |
| Pinyin エンジン | [`inputx-pinyin`](https://crates.io/crates/inputx-pinyin) | — |
| Pinyin WASM | [`inputx-pinyin-wasm`](https://crates.io/crates/inputx-pinyin-wasm) | [`@goliapkg/pinyin`](https://www.npmjs.com/package/@goliapkg/pinyin) |

各コンポーネントは 3 言語の README + docs.rs ドキュメントを持つ：

- **inputx-wubi** — [README (en)](core/crates/inputx-wubi/README.md) ·
  [简体中文](core/crates/inputx-wubi/README.zh-CN.md) ·
  [日本語](core/crates/inputx-wubi/README.ja.md) ·
  [docs.rs](https://docs.rs/inputx-wubi)
- **inputx-pinyin** — [README (en)](core/crates/inputx-pinyin/README.md) ·
  [简体中文](core/crates/inputx-pinyin/README.zh-CN.md) ·
  [日本語](core/crates/inputx-pinyin/README.ja.md) ·
  [docs.rs](https://docs.rs/inputx-pinyin)

## レポジトリ構成

```
┌─ ios/                   Swift キーボード拡張 + コンテナアプリ
│                         (iOS リリースターゲット)。cbindgen が生成する
│                         C FFI 経由で Rust コアを呼び出す。
│
├─ core/crates/
│   ├─ inputx-core/       コンポジット層: デュアルエンジンルーター、
│   │                     L0 学習、locale 処理、Swift が消費する C FFI。
│   ├─ inputx-wubi/       Wubi 86 エンコーダ + 辞書 (PHF + FST)。
│   ├─ inputx-wubi-wasm/  inputx-wubi の wasm-bindgen ラッパー。
│   ├─ inputx-pinyin/     普通話拼音セグメンタ + FST 辞書。
│   └─ inputx-pinyin-wasm/inputx-pinyin の wasm-bindgen ラッパー。
│
└─ mac/                   macOS シェル — iOS v1 出荷まで FROZEN。
```

`inputx-core` は crates.io に公開**されていない** — これはアプリ内部の
コンポジット層で、2 つのエンジンを iOS キーボード形態に束ねるための
もの。公開されているエンジン本体は単独で再利用可能。

## ビルド

要件: Rust ≥ 1.85 (edition 2024)、Xcode 16+、
[`xcodegen`](https://github.com/yonaskolb/XcodeGen)。

```sh
# Rust core — 全テストスイート実行 (253 tests + perfgate、5 crate)。
cd core && cargo test --release

# Bench (criterion) — pinyin と composite のホットパス。
cd core && cargo bench -p inputx-pinyin
cd core && cargo bench -p inputx-core

# iOS シミュレータビルド。
cd ios && ./build_sim.sh

# 実機ビルド (プロビジョニングプロファイル + 署名 identity が必要)。
cd ios && ./build_device.sh
```

C ヘッダ (`core/include/inputx_core.h`) は `cargo build` 中に
cbindgen が生成する。`.gitignore` 済みなので、新規 clone は iOS
ターゲットがリンクする前にまず Rust コアをビルドする必要がある。
ビルドスクリプトがこの順序を自動で処理する。

## ステータス

iOS v1、App Store 公開を目標。Rust エンジン、デュアルエンジン
ルーティング、L0 永続化、locale 処理、iOS キーボード / 設定 UI は
すべて実装済み + テスト済み。残作業は実機検証、性能プロファイリング、
TestFlight、App Store 申請。

v1 スコープ外: macOS シェル (凍結)、iCloud での L0 同期、
ja-JP / ko-KR ローカライゼーション。

## ライセンス

デュアルライセンス **MIT** ([`LICENSE-MIT`](LICENSE-MIT)) **OR
Apache-2.0** ([`LICENSE-APACHE`](LICENSE-APACHE)) © 2026 GOLIA K.K.、
お好みで選択可能。

同梱の第三者データとリソースはそれぞれのライセンスを保持する —
アプリ内の OSS 謝辞画面と `core/crates/inputx-pinyin/LICENSE-*` を
参照、完全な attribution チェーン (Unihan、jieba、pypinyin、Leipzig、
SUBTLEX-CH、Plangothic / OFL) が確認できる。
