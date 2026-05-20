# @goliapkg/pinyin (WASM)

Browser / Node bindings for [`inputx-pinyin`](../inputx-pinyin/) — a
self-developed Mandarin Pinyin input engine with segmenter, fuzzy
syllables, FST dict, and L0 user-learning. Pre-built into a ~100 KB
WebAssembly module (bootstrap dict embedded by default; full 9 MB dict
available behind a build feature).

The web surface of the [Inputx IME](https://github.com/goliajp/inputx).

**License:** MIT OR Apache-2.0.

> Read this in [简体中文](README.zh-CN.md) · [日本語](README.ja.md).

## Install

```sh
npm install @goliapkg/pinyin
```

## Usage

```js
import init, { PinyinEngine } from "@goliapkg/pinyin";

await init();                              // load + instantiate the .wasm
const eng = new PinyinEngine();

// Exact-syllable lookup
console.log(eng.lookup("zhongguo"));       // ["中国", "中过", ...]

// Prefix completion (used for partial-input IME suggestions)
const hits = eng.prefix("zho");
hits.slice(0, 5).forEach(h => console.log(h.pinyin, h.word));

// User picks a candidate → engine learns. 3 picks → auto-pin to L0.
eng.recordPick("zhongguo", "中国");

// Char → pinyin reverse lookup
console.log(eng.encode("中"));             // "zhong"

// Persistence — caller chooses storage
const state = eng.exportL0();
localStorage.setItem("pinyin-l0", JSON.stringify(state));
// later:
eng.importL0(JSON.parse(localStorage.getItem("pinyin-l0") ?? "{}"));
```

## What ships

- `golia_pinyin_wasm_bg.wasm` — engine + bootstrap FST (~100 KB)
- `golia_pinyin_wasm.js` — ES-module wrapper
- `golia_pinyin_wasm.d.ts` — TypeScript types

For the full 414K-entry dict (9 MB), rebuild without the
`bootstrap_only` feature — see Build below.

## Build from source

Default (small bootstrap dict, ~100 KB total bundle):

```sh
git clone https://github.com/goliajp/inputx
wasm-pack build core/crates/inputx-pinyin-wasm --target web --release
# output in core/crates/inputx-pinyin-wasm/pkg/
```

Full dict (~9 MB, all 414K entries):

```sh
wasm-pack build core/crates/inputx-pinyin-wasm \
    --no-default-features --target web --release
```

## See also

- Native Rust crate: [`inputx-pinyin`](../inputx-pinyin/)
- Parent IME repo: [Inputx](https://github.com/goliajp/inputx)
- Sibling wubi engine: [`@goliapkg/wubi`](../inputx-wubi-wasm/)
