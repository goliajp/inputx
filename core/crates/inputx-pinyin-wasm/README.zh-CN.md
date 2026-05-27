# @goliapkg/pinyin (WASM)

[`inputx-pinyin`](../inputx-pinyin/) 的浏览器 / Node 绑定 ——
自建的普通话拼音输入引擎，含分段器、模糊音、FST 字典和 L0 用户学习。
预编译为约 100 KB 的 WebAssembly 模块（默认嵌入 bootstrap 字典；
完整 9 MB 字典通过 build feature 启用）。

[Inputx IME](https://github.com/goliajp/inputx) 的 Web 形态。

**许可：** MIT OR Apache-2.0。

**语言**：[English](https://github.com/goliajp/inputx/blob/develop/core/crates/inputx-pinyin-wasm/README.md) · 简体中文 · [日本語](https://github.com/goliajp/inputx/blob/develop/core/crates/inputx-pinyin-wasm/README.ja.md)

## 安装

```sh
npm install @goliapkg/pinyin
```

## 使用

```js
import init, { PinyinEngine } from "@goliapkg/pinyin";

await init();                              // 加载并实例化 .wasm
const eng = new PinyinEngine();

// 精确音节查询
console.log(eng.lookup("zhongguo"));       // ["中国", "中过", ...]

// 前缀联想（partial-input IME 候选用）
const hits = eng.prefix("zho");
hits.slice(0, 5).forEach(h => console.log(h.pinyin, h.word));

// 用户选择候选 → 引擎学习。3 次后自动 pin 到 L0。
eng.recordPick("zhongguo", "中国");

// 字 → 拼音反查
console.log(eng.encode("中"));             // "zhong"

// 持久化 —— 调用方自己选存储
const state = eng.exportL0();
localStorage.setItem("pinyin-l0", JSON.stringify(state));
// 之后：
eng.importL0(JSON.parse(localStorage.getItem("pinyin-l0") ?? "{}"));
```

## 发布物

- `inputx_pinyin_wasm_bg.wasm` —— 引擎 + bootstrap FST（约 100 KB）
- `inputx_pinyin_wasm.js` —— ES module 包装
- `inputx_pinyin_wasm.d.ts` —— TypeScript 类型定义

完整 41.4 万词条字典（9 MB）需要去掉 `bootstrap_only` feature 重新构建，
见下方 Build 章节。

## 从源码构建

默认（小 bootstrap 字典，整包约 100 KB）：

```sh
git clone https://github.com/goliajp/inputx
wasm-pack build core/crates/inputx-pinyin-wasm --target web --release
# 输出在 core/crates/inputx-pinyin-wasm/pkg/
```

完整字典（约 9 MB，41.4 万条目）：

```sh
wasm-pack build core/crates/inputx-pinyin-wasm \
    --no-default-features --target web --release
```

## 相关链接

- 原生 Rust crate：[`inputx-pinyin`](../inputx-pinyin/)
- 母 IME 项目：[Inputx](https://github.com/goliajp/inputx)
- 姐妹五笔引擎：[`@goliapkg/wubi`](../inputx-wubi-wasm/)
