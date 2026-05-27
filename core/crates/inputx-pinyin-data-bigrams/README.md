# inputx-pinyin-data-bigrams

Embedded bigram FSAs for the
[`inputx-pinyin`](https://crates.io/crates/inputx-pinyin) engine.

```toml
[dependencies]
inputx-pinyin-data-bigrams = "1.4"
```

Pure data crate: two `pub const` byte slices via `include_bytes!`,
zero dependencies, `#![no_std]` clean. Split out of `inputx-pinyin`
in v1.4.7 sub-phase B (Strategy C — 3 `inputx-pinyin-data-*`
stones + facade umbrella) so the facade publishes light and
consumers who only need exact-syllable lookup can opt out via the
facade's `bigrams` feature.

## What's in the box

- **`EMBEDDED_BIGRAMS`** (~4.5 MB) — inter-token word bigram FSA:
  keys `<prev_word>\0<next_word>` where both ends are distinct
  jieba tokens adjacent in the source corpus. Sole input to the
  facade's `PinyinDict::bigram_boost` and next-word prediction
  paths.
- **`EMBEDDED_BIGRAMS_INTRA`** (~1.5 MB) — intra-token char bigram
  FSA: keys `<a>\0<b>` for adjacent characters *inside* one jieba
  token (e.g. `(你, 好)` captured from `你好`). Helps Viterbi
  composition prefer known phrases; never used for next-word
  prediction.

Both are in the
[`inputx-fsa::Fsa`](https://crates.io/crates/inputx-fsa) binary
format.

## Usage

Almost always indirect — `inputx-pinyin`'s default-on `bigrams`
feature pulls this in and wires it through
`PinyinDict::bigram_boost`. Direct use is for custom runtimes:

```rust
use inputx_pinyin_data_bigrams::{EMBEDDED_BIGRAMS, EMBEDDED_BIGRAMS_INTRA};
use inputx_fsa::Fsa;

let bigrams = Fsa::new(EMBEDDED_BIGRAMS).expect("valid FSA");
if let Some(count) = bigrams.get(b"\xe4\xbd\xa0\xe5\xa5\xbd\x00\xe5\x90\x97") {
    println!("(你好, 吗) bigram count = {count}");
}
```

## API stability

- **`EMBEDDED_BIGRAMS` / `EMBEDDED_BIGRAMS_INTRA`** — module path
  stable for the 1.x line. Underlying bytes rebuild with each
  release as the upstream corpus / weight pipeline refreshes.
- **No public API beyond the two consts** — by design.

## License

Dual-licensed under MIT OR Apache-2.0. Bigram counts derive from
permissively-licensed corpora (Leipzig Corpora / SUBTLEX-CH-WF);
see [`inputx-pinyin`](https://crates.io/crates/inputx-pinyin) for
the attribution chain.
