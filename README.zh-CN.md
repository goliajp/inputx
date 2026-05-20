# Inputx

[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#许可)
[![inputx-wubi](https://img.shields.io/crates/v/inputx-wubi.svg?label=inputx-wubi)](https://crates.io/crates/inputx-wubi)
[![inputx-pinyin](https://img.shields.io/crates/v/inputx-pinyin.svg?label=inputx-pinyin)](https://crates.io/crates/inputx-pinyin)
[![@goliapkg/wubi](https://img.shields.io/npm/v/@goliapkg/wubi.svg?label=%40goliapkg%2Fwubi)](https://www.npmjs.com/package/@goliapkg/wubi)
[![@goliapkg/pinyin](https://img.shields.io/npm/v/@goliapkg/pinyin.svg?label=%40goliapkg%2Fpinyin)](https://www.npmjs.com/package/@goliapkg/pinyin)

> **语言**：[English](README.md) · 简体中文 · [日本語](README.ja.md)

注重隐私的 iOS 中文输入法，双引擎核心用 Rust 写成。

Inputx 像万能五笔 / 搜狗五笔那样输入中文 —— **五笔 86 为主引擎，拼音
自动 fallback**，所以你永远不用手动切换输入方案。完全本地运行：零联网、
零分析、不需要"允许完全访问"。

## 特性

- **双引擎，无切换** —— 五笔 86 候选先出，拼音（全拼 + 简拼 +
  partial-input 前缀联想）在五笔为空时补位。
- **本地学习层 (L0)** —— 同一 `(input → word)` 被选 3 次自动 pin 到
  最前。学习数据永不离开设备，用户可 JSON 导出 / 导入。
- **隐私构建** —— 零联网、零遥测、无键盘完全访问。唯一收集的数据是
  Apple 自带 crash reporter 抓的东西。
- **Perfgate 守护每个 keystroke** —— 每次候选刷新都有单元测试守
  16ms / 60Hz 一帧预算。输入卡顿被当成最差用户体验。
- **生僻字支持** —— 可选显示 Plane-2+ 字符，通过内嵌 cascade 字体
  （`Lab8CJKExtended`，Plangothic 子集，OFL）。
- **CJK 排版** —— 全角 / 半角切换、ASCII→CJK 标点映射、状态化的智能引号。
- **iOS 原生 UX** —— Dynamic Type、深色模式、VoiceOver、横屏重排、
  按键长按变体、候选来源指示。

## 开源引擎生态

两个语言引擎加 WASM 包装独立发布到 crates.io / npm。它们驱动这个 IME
（iOS 原生 + Web WASM），也按 MIT / Apache-2.0 协议供任何其它五笔 /
拼音项目自由使用：

| 组件 | crates.io | npm |
|---|---|---|
| 五笔 86 引擎 | [`inputx-wubi`](https://crates.io/crates/inputx-wubi) | — |
| 五笔 86 WASM | [`inputx-wubi-wasm`](https://crates.io/crates/inputx-wubi-wasm) | [`@goliapkg/wubi`](https://www.npmjs.com/package/@goliapkg/wubi) |
| 拼音引擎 | [`inputx-pinyin`](https://crates.io/crates/inputx-pinyin) | — |
| 拼音 WASM | [`inputx-pinyin-wasm`](https://crates.io/crates/inputx-pinyin-wasm) | [`@goliapkg/pinyin`](https://www.npmjs.com/package/@goliapkg/pinyin) |

每个组件都有三语 README 和 docs.rs 文档：

- **inputx-wubi** —— [README (en)](core/crates/inputx-wubi/README.md) ·
  [简体中文](core/crates/inputx-wubi/README.zh-CN.md) ·
  [日本語](core/crates/inputx-wubi/README.ja.md) ·
  [docs.rs](https://docs.rs/inputx-wubi)
- **inputx-pinyin** —— [README (en)](core/crates/inputx-pinyin/README.md) ·
  [简体中文](core/crates/inputx-pinyin/README.zh-CN.md) ·
  [日本語](core/crates/inputx-pinyin/README.ja.md) ·
  [docs.rs](https://docs.rs/inputx-pinyin)

## 仓库结构

```
┌─ ios/                   Swift 键盘扩展 + 容器 app
│                         （iOS ship 目标）。通过 cbindgen 生成的 C FFI
│                         调用 Rust core。
│
├─ core/crates/
│   ├─ inputx-core/       组合层：双引擎 router、L0 学习、locale 处理、
│   │                     Swift 消费的 C FFI。
│   ├─ inputx-wubi/       五笔 86 编码器 + 字典（PHF + FST）。
│   ├─ inputx-wubi-wasm/  inputx-wubi 的 wasm-bindgen 包装。
│   ├─ inputx-pinyin/     普通话拼音分段器 + FST 字典。
│   └─ inputx-pinyin-wasm/inputx-pinyin 的 wasm-bindgen 包装。
│
└─ mac/                   macOS shell —— FROZEN，直到 iOS v1 ship。
```

`inputx-core` **不**发布到 crates.io，它是 app 内部的组合层，把两个
引擎绑成 iOS 键盘形态。发布的引擎本身是独立可复用的。

## 构建

需要 Rust ≥ 1.85（edition 2024）、Xcode 16+、
[`xcodegen`](https://github.com/yonaskolb/XcodeGen)。

```sh
# Rust core —— 跑全套测试（253 tests + perfgate，5 个 crate）。
cd core && cargo test --release

# Bench（criterion）—— pinyin 和 composite 热路径。
cd core && cargo bench -p inputx-pinyin
cd core && cargo bench -p inputx-core

# iOS 模拟器构建。
cd ios && ./build_sim.sh

# 真机构建（需要 provisioning profile + 签名身份）。
cd ios && ./build_device.sh
```

C header（`core/include/inputx_core.h`）由 cbindgen 在 `cargo build`
时生成，已 `.gitignore`，所以新 clone 必须先构建 Rust core 才能 link
iOS target。构建脚本自动处理这个顺序。

## 状态

iOS v1，目标 App Store。Rust 引擎、双引擎路由、L0 持久化、locale
处理、iOS 键盘 / 设置 UI 全部实现并测过。剩余工作是真机验证、
性能 profiling、TestFlight、App Store 提交。

v1 范围外：macOS shell（冻结）、iCloud L0 同步、ja-JP / ko-KR 本地化。

## 许可

双协议 **MIT**（[`LICENSE-MIT`](LICENSE-MIT)）**OR Apache-2.0**
（[`LICENSE-APACHE`](LICENSE-APACHE)）© 2026 GOLIA K.K.，任你选。

内嵌第三方数据和资源保留各自的许可 —— 见 app 内的 OSS 致谢页和
`core/crates/inputx-pinyin/LICENSE-*` 看完整 attribution 链
（Unihan、jieba、pypinyin、Leipzig、SUBTLEX-CH、Plangothic / OFL）。
