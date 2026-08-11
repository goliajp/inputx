# inputx-pinyin-data-core

Embedded core Mandarin Pinyin dictionary blob for the
[`inputx-pinyin`](https://crates.io/crates/inputx-pinyin) engine.

```toml
[dependencies]
inputx-pinyin-data-core = "1.4"
```

Pure data crate: a single `pub const EMBEDDED_PINYIN_DICT: &[u8]`
loaded via `include_bytes!`. Zero dependencies, `#![no_std]` clean.
Split out of `inputx-pinyin` in v1.4.7 sub-phase B (Strategy C —
3 `inputx-pinyin-data-*` stones + facade umbrella) so the facade
crate publishes light and consumers can opt out of the heavier
`bigrams` / `trigrams` data stones without dragging this required
core dict along.

## What's in the box

- ~217k pinyin → word entries in the
  [`inputx-fsa::Dict`](https://crates.io/crates/inputx-fsa) binary
  format. Two-level structure (code → many `(item, value)` pairs)
  keeps the unique item bytes outside the FSA, materially smaller
  than a flat `code\0item → value` graph.
- Single-character readings sourced from Unihan
  (`kHanyuPinlu` + `kMandarin`), multi-character phrase readings
  from pypinyin's canonical 47k-phrase override plus a Unihan
  cartesian-product long tail.
- See the [`inputx-pinyin`](https://crates.io/crates/inputx-pinyin)
  README for the full data lineage + license breakdown.

## Usage

You almost never depend on this crate directly. The
[`inputx-pinyin`](https://crates.io/crates/inputx-pinyin) facade
re-loads it under the hood for `PinyinDict::embedded`. Reach for it
yourself only when you're rolling a custom Pinyin runtime that
wants the same shipped dict without the rest of the facade engine:

```rust
use inputx_pinyin_data_core::EMBEDDED_PINYIN_DICT;
use inputx_fsa::Dict;

let dict = Dict::new(EMBEDDED_PINYIN_DICT).expect("valid dict bytes");
dict.get_for_each(b"zhongguo", |word, value| {
    println!("{} (value={value})", std::str::from_utf8(word).unwrap());
});
```

## API stability

- **`EMBEDDED_PINYIN_DICT` byte slice** — its existence and module
  path is stable for the 1.x line. The underlying bytes rebuild
  with each release as the upstream corpus / weight pipeline
  refreshes; consumers should treat it as opaque data and route
  through `inputx_fsa::Dict` for decoding.
- **No public API beyond the const** — by design.

## License

Dual-licensed under MIT OR Apache-2.0. Bundled dict bytes derive
from permissively-licensed sources (Unihan / jieba / pypinyin /
Leipzig Corpora / SUBTLEX-CH-WF); see
[`inputx-pinyin`](https://crates.io/crates/inputx-pinyin) for the
attribution chain.
