# @goliapkg/wubi (WASM)

[`inputx-wubi`](../inputx-wubi/) 的浏览器 / Node 绑定 ——
自建的五笔字型 (Wubi 86) 编码器和字典，内置 **L0 / L1+ 排序** 和
用户自动学习。预编译为约 3 MB 的 WebAssembly 模块（13.5 万词条字典已嵌入）。

[Inputx IME](https://github.com/goliajp/inputx) 的 Web 形态。

**许可：** MIT OR Apache-2.0。

**语言**：[English](https://github.com/goliajp/inputx/blob/develop/core/crates/inputx-wubi-wasm/README.md) · 简体中文 · [日本語](https://github.com/goliajp/inputx/blob/develop/core/crates/inputx-wubi-wasm/README.ja.md)

## 安装

```sh
npm install @goliapkg/wubi
```

## 使用

```js
import init, { WubiEngine, Layer } from "@goliapkg/wubi";

await init();                              // 加载并实例化 .wasm
const eng = new WubiEngine();              // 微秒级，仅 header 解析

// L0/L1 排序查询
console.log(eng.lookup("khlg"));           // ["中国", "跑车", "跨国", ...]
console.log(eng.lookup("ipbf"));           // ["学"]

// 前缀查询 —— 用于 IME 联想候选
const hits = eng.prefix("g");
hits.slice(0, 5).forEach(h => console.log(h.code, h.word));

// 用户选择候选 → 字典学习。同一 (code, word) 被选 3 次后自动 pin 到 L0。
eng.recordPick("khlg", "跑车");

// Layer prefs（进阶；默认 Auto = 0.7，其余 = 1.0）
eng.setLayerPref(Layer.Phrase, 1.5);

// 持久化 —— 调用方自己选存储
const state = eng.exportL0();
localStorage.setItem("wubi-l0", JSON.stringify(state));
// 之后：
eng.importL0(JSON.parse(localStorage.getItem("wubi-l0") ?? "{}"));
```

## 发布物

- `wubi_wasm_bg.wasm` —— 打包的 FST + 排序 + L0 逻辑（约 3 MB）
- `wubi_wasm.js` —— ES module 包装
- `wubi_wasm.d.ts` —— TypeScript 类型定义
- 135,822 条字典（字根 / 简码 / 词组 / 自动分解 CJK）

## 从源码构建

```sh
git clone https://github.com/goliajp/inputx
wasm-pack build core/crates/inputx-wubi-wasm --target web --release
# 输出在 core/crates/inputx-wubi-wasm/pkg/
```

Node 目标：`wasm-pack build --target nodejs`。

## 相关链接

- 原生 Rust crate：[`inputx-wubi`](../inputx-wubi/)
- 母 IME 项目：[Inputx](https://github.com/goliajp/inputx)
- 姐妹拼音引擎：[`@goliapkg/pinyin`](../inputx-pinyin-wasm/)
