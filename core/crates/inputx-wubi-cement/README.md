# inputx-wubi-cement

> **⚠ DEPRECATED at v2.0 (2026-05).** The v1.5 cycle's D11 taxonomy
> correction reclassified "cement" as **application source code, not
> a published crate**. v1.5.2 carved the `WubiEngine` state machine +
> `AutoCommitPolicy` out of this crate into the Inputx monorepo's
> `inputx-core/src/wubi/engine.rs`. v2.0.0 publishes the post-carve
> state (engine types removed; only data + lookup helpers remain).
>
> **Migration path**:
>
> - **Need WubiEngine** (state machine, handle_letter, auto-commit,
>   commit_index): copy [`inputx-core/src/wubi/engine.rs`](https://github.com/goliajp/inputx/blob/develop/core/crates/inputx-core/src/wubi/engine.rs)
>   into your application. It's a self-contained 370 LOC state
>   machine over this crate's stone-style helpers; copy it because
>   per the new taxonomy it IS your application's wubi cement.
> - **Just need data + lookup**: keep this crate for the
>   `EMBEDDED_WUBI_IDF` const, `wubi_idf_reader()` `OnceLock` reader,
>   `layer_from_idf_tag` helper, and the `lookup` / `lookup_with_*`
>   / `prefix_predictions` / `record_pick` table fns. A future
>   `inputx-wubi-data` stone (v1.6 backlog) will adopt these under
>   the correct taxonomy name; this crate is the holding pattern.

Wubi-specific data + lookup helpers for the [Inputx](https://github.com/goliajp/inputx)
IME. Post-v1.5.2 surface is stone-quality (no business glue, no
per-session state).

```toml
[dependencies]
inputx-wubi-cement = "1.4"
```

Pairs with the [`inputx-wubi`](https://crates.io/crates/inputx-wubi)
facade (codec / decomp / dict / jianma / layer / stroke / zigen
primitives) and the [`inputx-dict-format`](https://crates.io/crates/inputx-dict-format)
IDFv1 reader. Use the facade if you're writing your own Wubi IME and
want only the data + lookup primitives; use this cement if you want
the full stateful keystroke handler + IDF-backed lookup that Inputx
itself uses.

## What's in the box

- **`WubiEngine`** — stateful per-session driver: buffer state,
  `handle_letter` / `backspace` / `escape` / `commit_index` /
  `clear_all`, full simcode promote logic, L0 (per-user pin)
  snapshot helpers.
- **`AutoCommitPolicy`** — the host's policy switch for auto-commit
  behavior (`OnFourCodesIfUnique` / `Never` / per-buffer-length
  custom). Same enum the iOS / web Inputx UIs hand the host.
- **`EMBEDDED_WUBI_IDF`** (v1.4.7 sub-phase A4 step 2) — process-
  embedded IDFv1 wubi dict blob. **EntryFlags `engine_tag` bits
  carry the `inputx_wubi::Layer` enum index** (0=Auto, 1=Phrase,
  2=Zigen, 3=Jianma3, 4=Jianma2, 5=Jianma1), so cement-side fills
  can reconstruct `(word, layer, raw_freq)` losslessly without
  re-reading the facade dict.
- **`wubi_idf_reader()`** — process-global `IdfReader` `OnceLock`;
  9 MB parse + sha256 verify amortizes once across the process,
  subsequent lookups are O(|code|) FST walks.
- **`layer_from_idf_tag(u8) -> Layer`** — reverse of
  `Layer::as_index`; safely falls back to `Layer::Auto` on out-of-
  range bytes.
- **`lookup_with_freq_layer(code) -> Vec<(String, Layer, u64)>`**
  / **`prefix_predictions(prefix) -> Vec<(String, u64, usize)>`**
  — cement-level corpus lookups, both backed by `wubi_idf_reader()`
  (v1.4.7 A4 step 2 cutover from the facade dict path).
- **L0 export / import / `set_show_rare` / `show_rare` / `warmup`**
  — host-side persistence + rare-CJK glyph toggle.

## Quick start

```rust
use inputx_wubi_cement::{AutoCommitPolicy, WubiEngine};

let mut engine = WubiEngine::new();
engine.set_policy(AutoCommitPolicy::OnFourCodesIfUnique);

for b in b"shan" {
    if let Some(committed) = engine.handle_letter(*b) {
        println!("committed: {committed}");
    }
}
let cands = engine.candidates();
println!("preedit: {} candidates: {:?}", engine.buffer_str(), cands);

// Layer-aware cement-side lookup (IDF-backed).
let with_layer = engine.candidates_with_freq_layer();
for (word, layer, freq) in with_layer.iter().take(5) {
    println!("{word} layer={layer:?} freq={freq}");
}
```

Direct IDF reader access (skip the engine state machine):

```rust
use inputx_wubi_cement::{wubi_idf_reader, layer_from_idf_tag};

let r = wubi_idf_reader();
for entry in r.lookup(b"g") {
    let layer = layer_from_idf_tag(entry.flags.engine_tag());
    println!("{} layer={layer:?} raw_freq={}", entry.word, entry.raw_freq);
}
// 一 layer=Jianma1 raw_freq=N
```

## Architecture note

The stateful `WubiEngine` here owns the per-session buffer + auto-
commit policy + L0 pin state. Composite-engine glue (cross-engine
mode dispatcher, scoring constants, merge sort key) lives in
`inputx-core/composite/` — by [`PLAN-stones-extract.md` "Cement
catalog"](https://github.com/goliajp/inputx/blob/develop/.claude/PLAN-stones-extract.md)
terminology that's **composite root cement** (not per-language).
This crate contains only genuinely wubi-specific helpers — anything
a third-party Wubi IME consumer would re-implement the same way.

## API stability

The 1.x line follows semver:

- **`WubiEngine` / `AutoCommitPolicy` public surface** — no
  breaking changes within 1.x. New `AutoCommitPolicy` variants
  land via `#[non_exhaustive]` semantics.
- **`EMBEDDED_WUBI_IDF` schema** — IDFv1 with the v1.4.7
  `engine_tag` Layer encoding. Cement-side readers will continue to
  decode v1.4.6-era blobs (`engine_tag == 0` → `Layer::Auto`
  fallback) for the rest of the 1.x line.
- **`wubi_idf_reader()` / `layer_from_idf_tag` / `lookup_with_freq_
  layer` / `prefix_predictions` signatures** — stable.

## License

Dual-licensed under MIT OR Apache-2.0.
