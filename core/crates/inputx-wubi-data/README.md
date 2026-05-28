# inputx-wubi-data

Embedded Wubi 86 IDFv1 dict blob + IdfReader OnceLock + Layer-from-
EntryFlags helper for the
[`inputx-wubi`](https://crates.io/crates/inputx-wubi) engine,
packaged as a publishable stone.

```toml
[dependencies]
inputx-wubi-data = "1.6"
```

Successor to [`inputx-wubi-cement`](https://crates.io/crates/inputx-wubi-cement)
under the v1.5 D11 taxonomy correction (2026-05): **cement =
application source code, not a published crate**. The historical
`-cement`-suffix crate is deprecated and re-exports from this crate
for backward compatibility.

## What's in the box

- **`EMBEDDED_WUBI_IDF`** — IDFv1 binary dict blob (~4.4 MB,
  135,822 entries). Each entry's `EntryFlags::engine_tag()` carries
  the wubi `Layer` enum index (v1.4.7 sub-phase A4 step 2 schema
  bump), so cement-side fills can reconstruct
  `(word, layer, raw_freq)` without re-reading the
  `inputx_wubi::WubiDict` table.
- **`wubi_idf_reader()`** — process-global `OnceLock<IdfReader>`;
  the 4 MB parse + sha256 verify amortizes once across the process,
  subsequent `lookup(code)` calls are O(|code|) FST walks with zero
  per-query allocation.
- **`layer_from_idf_tag(u8) -> inputx_wubi::Layer`** — reverse of
  `Layer::as_u8`; falls back to `Layer::Auto` on out-of-range bytes.
- **Table helpers** (`lookup`, `lookup_with_scores`,
  `lookup_with_layer`, `lookup_with_freq_layer`,
  `prefix_predictions`, `record_pick`) — process-global stateful
  `WubiDict` cache + per-code lookups, with rare-CJK filter
  (`set_show_rare` / `show_rare`).
- **L0 helpers** (`export_l0`, `import_l0`) + `warmup` + the
  `inputx_wubi::L0Snapshot` re-export.

## What's NOT here

- **Stateful `WubiEngine`** (buffer / `handle_letter` /
  `AutoCommitPolicy` / `commit_index` / L0 pin state machine) — per
  the v1.5 D11 correction, that classifies as **application
  cement** (your own `wubi.rs` / `engine.rs`). The Inputx monorepo's
  reference implementation lives at
  [`inputx-core/src/wubi/engine.rs`](https://github.com/goliajp/inputx/blob/develop/core/crates/inputx-core/src/wubi/engine.rs)
  (370 LOC, MIT/Apache-2.0; copy into your app).

## Usage

```rust
use inputx_wubi_data::{wubi_idf_reader, layer_from_idf_tag};

let r = wubi_idf_reader();
for entry in r.lookup(b"g") {
    let layer = layer_from_idf_tag(entry.flags.engine_tag());
    println!("{} layer={layer:?} raw_freq={}", entry.word, entry.raw_freq);
}
// 一 layer=Jianma1 raw_freq=N
```

## API stability

- **`EMBEDDED_WUBI_IDF` byte slice** — stable module path across 1.x;
  underlying bytes rebuild with each release as the upstream wubi
  data refreshes.
- **`wubi_idf_reader` / `layer_from_idf_tag` / table helpers** —
  signatures stable across 1.x.

## License

Dual-licensed under MIT OR Apache-2.0.
