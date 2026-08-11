# inputx-pinyin-helpers

Pinyin-specific stateless helpers + embedded IDFv1 dict (with
prior_correction baked) + embedded NGMv1 bigram blob, for the
[`inputx-pinyin`](https://crates.io/crates/inputx-pinyin) engine.

```toml
[dependencies]
inputx-pinyin-helpers = "1.6"
```

Successor to [`inputx-pinyin-cement`](https://crates.io/crates/inputx-pinyin-cement)
under the v1.5 D11 taxonomy correction (2026-05): **cement =
application source code, not a published crate**. The historical
`-cement`-suffix crate is deprecated and re-exports from this crate
for backward compat.

## What's in the box

- **`EMBEDDED_PINYIN_IDF`** — IDFv1 binary blob (~9 MB, 237k
  entries) with `prior_correction` Q4 boosts baked into
  `log_prior_q4` at build time (v1.4.7 sub-phase A5).
- **`EMBEDDED_BIGRAMS_NGM`** — NGMv1 binary blob (~595 KB, 64k
  triplets). Same Q4 log-prob data as the facade's bundled
  `bigrams.fsa`, just packaged for `inputx_ngram::NgramTable`
  consumption.
- **`pinyin_idf_reader()`** — process-global
  `OnceLock<IdfReader>`.
- **`bigram_boost_from_ngm(table, prev, next) -> i16`** — Q4 log-
  space bigram bonus, NGMv1-backed.
- **`legacy_bigram_boost_from_ngm(table, prev, next) -> f64`** —
  v1.3-calibration bridge for transitional sort-key windows.
- **`estimated_freq_from_log_prior(i16) -> u64`** — inverse of
  `inputx_scoring::log_prior_from_freq`. Round-trip drift ≤ 1% at
  typical bigram counts.

## What's NOT here

- **Stateful `PinyinAdapter`** (handle_letter / state machine /
  fuzzy-variant fill / prefix prediction L0 pin) — that classifies
  as application cement per the v1.5 D11 correction. The Inputx
  monorepo's reference implementation lives at
  [`inputx-core/src/composite/pinyin_adapter.rs`](https://github.com/goliajp/inputx/blob/develop/core/crates/inputx-core/src/composite/pinyin_adapter.rs).

## Usage

```rust
use inputx_pinyin_helpers::{pinyin_idf_reader, bigram_boost_from_ngm, EMBEDDED_BIGRAMS_NGM};
use inputx_ngram::NgramTable;

let reader = pinyin_idf_reader();
for entry in reader.lookup(b"jixu") {
    println!("{} log_prior_q4={} raw_freq={}", entry.word, entry.log_prior, entry.raw_freq);
}
// 继续 log_prior_q4=N (baked +17 correction) raw_freq=74652

let ngm = NgramTable::from_bytes(EMBEDDED_BIGRAMS_NGM)
    .expect("EMBEDDED_BIGRAMS_NGM is a valid NGMv1 blob");
let boost: i16 = bigram_boost_from_ngm(&ngm, Some("今天"), "是");
```

## API stability

- **`pinyin_idf_reader` / `bigram_boost_from_ngm` /
  `legacy_bigram_boost_from_ngm` / `estimated_freq_from_log_prior`
  signatures** — stable across 1.x.
- **`EMBEDDED_PINYIN_IDF` / `EMBEDDED_BIGRAMS_NGM` blobs** — module
  paths stable; underlying bytes rebuild with each release.

## License

Dual-licensed under MIT OR Apache-2.0.
