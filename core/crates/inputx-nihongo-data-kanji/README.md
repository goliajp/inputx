# inputx-nihongo-data-kanji

Embedded Japanese kanji multi-reading IDFv1 dict blob + IdfReader
OnceLock for the
[`inputx-nihongo`](https://crates.io/crates/inputx-nihongo) engine.

```toml
[dependencies]
inputx-nihongo-data-kanji = "1.6"
```

Successor to [`inputx-nihongo-cement`](https://crates.io/crates/inputx-nihongo-cement)'s
kanji half under the v1.5 D11 taxonomy correction.

Pure data + stateless lookup helper.

## What's in the box

- **`EMBEDDED_NIHONGO_KANJI_IDF`** — IDFv1 binary blob (~40 KB,
  1,666 `(reading, kanji)` pairs — multi-reading expansion of 813
  source kanji from `KANJI_TABLE`).
- **`nihongo_kanji_idf_reader()`** — process-global
  `OnceLock<IdfReader>`.

## Usage

```rust
use inputx_nihongo_data_kanji::nihongo_kanji_idf_reader;

let r = nihongo_kanji_idf_reader();
for entry in r.lookup(b"nichi") {
    println!("{} freq={}", entry.word, entry.raw_freq);
}
// 日 freq=N (and other nichi-readings)
```

## API stability

- **`EMBEDDED_NIHONGO_KANJI_IDF` / `nihongo_kanji_idf_reader`** —
  module path stable across 1.x.

## See also

- Sibling: [`inputx-nihongo-data-jukugo`](../inputx-nihongo-data-jukugo/)
- Facade: [`inputx-nihongo`](../inputx-nihongo/)

## License

Dual-licensed under MIT OR Apache-2.0.
