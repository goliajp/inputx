# inputx-pinyin-data-trigrams

Embedded word-trigram dict for the
[`inputx-pinyin`](https://crates.io/crates/inputx-pinyin) engine.

```toml
[dependencies]
inputx-pinyin-data-trigrams = "1.4"
```

Pure data crate: a single `pub const EMBEDDED_TRIGRAMS: &[u8]` via
`include_bytes!`, zero dependencies, `#![no_std]` clean. Split out
of `inputx-pinyin` in v1.4.7 sub-phase B (Strategy C — 3
`inputx-pinyin-data-*` stones + facade umbrella) so the facade
publishes light and consumers who only need exact-syllable + bigram
lookup can opt out via the facade's `trigrams` feature.

## What's in the box

- ~13 MB inter-token word-trigram dict in the
  [`inputx-fsa::Dict`](https://crates.io/crates/inputx-fsa) binary
  format. Keys are `<a>\0<b>\0<c>` where all three are distinct
  jieba tokens adjacent in the corpus. Two-level Dict layout —
  `(a\0b, *)` scans are the only access pattern — so unique items
  live outside the code FSA, materially smaller than a flat
  `a\0b\0c → count` graph.

## Usage

Almost always indirect — `inputx-pinyin`'s default-on `trigrams`
feature pulls this in and wires it through
`PinyinDict::predict_next_words_context` (the "联想 v1.0" sentence-
level coherent next-word prediction API). Direct use is for
custom runtimes:

```rust
use inputx_pinyin_data_trigrams::EMBEDDED_TRIGRAMS;
use inputx_fsa::Dict;

let trigrams = Dict::new(EMBEDDED_TRIGRAMS).expect("valid trigram dict");
// Walk every (c, count) for the (a, b) context "我们\0一起":
let key = b"\xe6\x88\x91\xe4\xbb\xac\x00\xe4\xb8\x80\xe8\xb5\xb7";
trigrams.get_for_each(key, |c_bytes, count| {
    let c = std::str::from_utf8(c_bytes).unwrap();
    println!("(我们, 一起, {c}) count={count}");
});
```

## API stability

- **`EMBEDDED_TRIGRAMS` byte slice** — module path stable for the
  1.x line. Underlying bytes rebuild with each release as the
  upstream corpus refreshes.
- **No public API beyond the const** — by design.

## License

Dual-licensed under MIT OR Apache-2.0. Trigram counts derive from
permissively-licensed corpora (Leipzig Corpora / SUBTLEX-CH-WF);
see [`inputx-pinyin`](https://crates.io/crates/inputx-pinyin) for
the attribution chain.
