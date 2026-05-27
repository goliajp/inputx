# @goliapkg/wubi (WASM)

Browser / Node bindings for [`inputx-wubi`](../inputx-wubi/) — a
self-developed Wubi 86 (五笔字型) encoder + dictionary with built-in
**L0 / L1+ ranking** and per-user auto-learning. Pre-built into a ~3 MB
WebAssembly module (135K-entry dictionary embedded).

The web surface of the [Inputx IME](https://github.com/goliajp/inputx).

**License:** MIT OR Apache-2.0.

> Read this in [简体中文](https://github.com/goliajp/inputx/blob/develop/core/crates/inputx-wubi-wasm/README.zh-CN.md) · [日本語](https://github.com/goliajp/inputx/blob/develop/core/crates/inputx-wubi-wasm/README.ja.md).

## Install

```sh
npm install @goliapkg/wubi
```

## Usage

```js
import init, { WubiEngine, Layer } from "@goliapkg/wubi";

await init();                              // load + instantiate the .wasm
const eng = new WubiEngine();              // ~µs, header-parse only

// L0/L1-ranked lookup
console.log(eng.lookup("khlg"));           // ["中国", "跑车", "跨国", ...]
console.log(eng.lookup("ipbf"));           // ["学"]

// Prefix lookup — for IME predictive candidates
const hits = eng.prefix("g");
hits.slice(0, 5).forEach(h => console.log(h.code, h.word));

// User picks a candidate → dict learns. After 3 picks of the same
// (code, word) the dict auto-pins it to L0.
eng.recordPick("khlg", "跑车");

// Layer prefs (advanced; default Auto = 0.7, others = 1.0)
eng.setLayerPref(Layer.Phrase, 1.5);

// Persistence — caller chooses storage
const state = eng.exportL0();
localStorage.setItem("wubi-l0", JSON.stringify(state));
// later:
eng.importL0(JSON.parse(localStorage.getItem("wubi-l0") ?? "{}"));
```

## What ships

- `inputx_wubi_wasm_bg.wasm` — packed FST + ranking + L0 logic (~3 MB)
- `inputx_wubi_wasm.js` — ES-module wrapper
- `inputx_wubi_wasm.d.ts` — TypeScript types
- 135,822 dictionary entries (字根 / 简码 / phrases / auto-decomposed CJK)

## Build from source

```sh
git clone https://github.com/goliajp/inputx
wasm-pack build core/crates/inputx-wubi-wasm --target web --release
# output in core/crates/inputx-wubi-wasm/pkg/
```

For Node-target bundles: `wasm-pack build --target nodejs`.

## API stability

The 1.x line follows semver:

- **`WubiEngine` constructor + method set** — no breaking changes
  within 1.x. New methods may be added as minor bumps.
- **`Layer` enum discriminants** — fixed (0=Auto … 5=Jianma1);
  matches the native `inputx-wubi` `Layer::as_u8`.
- **`exportL0` / `importL0` JSON shape** —
  `{ pins: [[code, word]], pickCounts: [[code, word, n]], layerPrefs: [...6] }` —
  stable across 1.x; v1.5 carries L0 state forward unchanged from
  prior 1.x JSON.

## See also

- Native Rust crate: [`inputx-wubi`](../inputx-wubi/)
- Parent IME repo: [Inputx](https://github.com/goliajp/inputx)
- Sibling engines: [`@goliapkg/pinyin`](../inputx-pinyin-wasm/) ·
  [`@goliapkg/nihongo`](../inputx-nihongo-wasm/)
