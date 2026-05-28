# inputx-nihongo-data-jukugo

Embedded Japanese jukugo (熟語) IDFv1 dict blob + IdfReader OnceLock
for the [`inputx-nihongo`](https://crates.io/crates/inputx-nihongo)
engine.

```toml
[dependencies]
inputx-nihongo-data-jukugo = "1.6"
```

Successor to [`inputx-nihongo-cement`](https://crates.io/crates/inputx-nihongo-cement)'s
jukugo half under the v1.5 D11 taxonomy correction (2026-05):
**cement = application source code, not a published crate**. The
historical `-cement`-suffix crate is deprecated and re-exports from
this crate (and the sibling `inputx-nihongo-data-kanji`) for
backward compat.

Pure data + stateless lookup helper. No application glue, no
per-session state.

## What's in the box

- **`EMBEDDED_NIHONGO_JUKUGO_IDF`** — IDFv1 binary blob (~1.1 MB,
  27,380 entries). Byte-equivalent to the facade's `JUKUGO_TABLE`
  const table (`idf-from-nihongo-jukugo` sources both from the same
  data).
- **`nihongo_jukugo_idf_reader()`** — process-global
  `OnceLock<IdfReader>`; the ~1 MB parse + sha256 verify amortizes
  once across the process lifetime.

## Usage

```rust
use inputx_nihongo_data_jukugo::nihongo_jukugo_idf_reader;

let r = nihongo_jukugo_idf_reader();
for entry in r.lookup(b"shinjuku") {
    println!("{} freq={}", entry.word, entry.raw_freq);
}
// 新宿 freq=N
```

## API stability

- **`EMBEDDED_NIHONGO_JUKUGO_IDF` / `nihongo_jukugo_idf_reader`** —
  module path stable across 1.x; underlying bytes rebuild with each
  release as the upstream jukugo table refreshes.

## See also

- Sibling: [`inputx-nihongo-data-kanji`](../inputx-nihongo-data-kanji/)
- Facade: [`inputx-nihongo`](../inputx-nihongo/)

## License

Dual-licensed under MIT OR Apache-2.0.
