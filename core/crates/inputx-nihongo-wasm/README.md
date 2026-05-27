# @goliapkg/nihongo (WASM)

Browser / Node bindings for [`inputx-nihongo`](../inputx-nihongo/) —
self-contained Japanese input engine: Hepburn (with kunrei alternates)
romaji → hiragana/katakana, on-yomi single-kanji lookup, 27k jukugo
(熟語) compound lookup, mechanical `compose_sentence` particle/copula
glue. Pre-built into a ~1.5 MB WebAssembly module (JUKUGO_TABLE +
KANJI_TABLE embedded).

The web surface of the [Inputx IME](https://github.com/goliajp/inputx).

**License:** MIT OR Apache-2.0.

## Install

```sh
npm install @goliapkg/nihongo
```

## Usage

```js
import init, { JapaneseEngine, KanaKind } from "@goliapkg/nihongo";

await init();                              // load + instantiate the .wasm
const eng = new JapaneseEngine();

// Drive the engine one ASCII byte at a time. `-` is also accepted as
// chōonpu (ー / long-vowel mark) when the buffer is non-empty so
// long-vowel words like コーヒー stay reachable.
for (const c of "shinjuku") {
    eng.handleLetter(c.charCodeAt(0));
}

console.log(eng.preedit());                // "shinjuku"
console.log(eng.candidates());
// [
//   { word: "新宿",          kind: KanaKind.Kanji,    freq: N,
//     composed: false, proximityMilli: 1000 },
//   { word: "しんじゅく",     kind: KanaKind.Hiragana, freq: 0,
//     composed: false, proximityMilli: 1000 },
//   { word: "シンジュク",     kind: KanaKind.Katakana, freq: 0,
//     composed: false, proximityMilli: 1000 },
// ]

// Commit one — engine clears its buffer ready for the next word.
const committed = eng.commitIndex(0);       // "新宿"

// Backspace pops one byte; escape clears the entire buffer.
for (const c of "tabe") eng.handleLetter(c.charCodeAt(0));
eng.backspace();                            // pops 'e'
eng.escape();                               // clears entirely
```

## Candidate fields

Each `candidates()` entry carries:

| Field | Type | Meaning |
|---|---|---|
| `word` | `string` | The displayed text (kanji / hiragana / katakana) |
| `kind` | `KanaKind` | Enum (`0=Hiragana, 1=Katakana, 2=Kanji`) for the host UI's optional kana/kanji hint |
| `freq` | `number` | 0–100 frequency score. Higher = more common in modern JP. `0` for kana fallback (mechanical romaji → kana, not freq-tuned) |
| `composed` | `boolean` | `true` for `compose_sentence` products (particle/copula glue like 私は). LOW confidence — host should rank these below real dict words |
| `proximityMilli` | `number` | `1000` = the buffer is the full reading (exact match). `< 1000` = prefix-prediction candidate whose reading the buffer is only a prefix of (`shinjuk → 新宿` at `875`) |

## What ships

- `inputx_nihongo_wasm_bg.wasm` — engine + 27,380 jukugo + 1,666
  kanji-reading pairs (~1.5 MB)
- `inputx_nihongo_wasm.js` — ES-module wrapper
- `inputx_nihongo_wasm.d.ts` — TypeScript types

## Build from source

```sh
git clone https://github.com/goliajp/inputx
wasm-pack build core/crates/inputx-nihongo-wasm --target web --release
# output in core/crates/inputx-nihongo-wasm/pkg/
```

For Node-target bundles: `wasm-pack build --target nodejs`.

## API stability

The 1.x line follows semver:

- **`JapaneseEngine` constructor + method set** — no breaking changes
  within 1.x. New methods may be added as minor bumps.
- **`KanaKind` enum discriminants** — fixed (0=Hiragana, 1=Katakana,
  2=Kanji); matches the native facade's `KanaKind` enum.
- **`candidates()` object shape** — the 5 fields above are stable
  across 1.x; additional fields may be added as minor bumps.
- **`commitIndex` return** — string for in-range, `null` for
  out-of-range index; out-of-range never mutates state.

## See also

- Native Rust crate: [`inputx-nihongo`](../inputx-nihongo/)
- Parent IME repo: [Inputx](https://github.com/goliajp/inputx)
- Sibling engines: [`@goliapkg/wubi`](../inputx-wubi-wasm/) ·
  [`@goliapkg/pinyin`](../inputx-pinyin-wasm/)
