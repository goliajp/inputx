//! `inputx-wubi-data` — embedded Wubi 86 IDFv1 dict + lookup helpers
//! for the [`inputx-wubi`](https://crates.io/crates/inputx-wubi)
//! engine, packaged as a publishable stone.
//!
//! Successor to [`inputx-wubi-cement`](https://crates.io/crates/inputx-wubi-cement)
//! under the v1.5 D11 taxonomy correction (2026-05): cement = an
//! application's source code (your own `wubi.rs` / `engine.rs`),
//! NOT a published crate. The historical `-cement`-suffix crate is
//! deprecated and re-exports from this crate for backward compat.
//!
//! ## What's in the box
//!
//! - [`EMBEDDED_WUBI_IDF`] — IDFv1 binary dict blob with the wubi
//!   `Layer` enum index encoded in `EntryFlags::engine_tag()`
//!   (v1.4.7 sub-phase A4 step 2).
//! - [`wubi_idf_reader`] — process-global `OnceLock<IdfReader>` over
//!   the embedded blob; amortizes the 4 MB parse + sha256 verify
//!   across the process lifetime.
//! - [`layer_from_idf_tag`] — reverse of `Layer::as_u8`; decodes an
//!   IDF entry's engine_tag back into the originating wubi `Layer`.
//! - `table` module — process-global stateful `WubiDict` cache +
//!   per-code lookup helpers (`lookup`, `lookup_with_scores`,
//!   `lookup_with_layer`, `lookup_with_freq_layer`,
//!   `prefix_predictions`, `record_pick`, `export_l0`, `import_l0`)
//!   + rare-CJK toggle (`set_show_rare` / `show_rare`) + warmup
//!   helper.
//!
//! ## What's NOT here
//!
//! - **Stateful `WubiEngine`** (buffer / `handle_letter` /
//!   auto-commit / commit_index / L0 pin state machine) — that
//!   classifies as application cement per the v1.5 D11 correction
//!   and now lives in the Inputx monorepo's
//!   [`inputx-core/src/wubi/engine.rs`](https://github.com/goliajp/inputx/blob/develop/core/crates/inputx-core/src/wubi/engine.rs).
//!   IME implementers copying this stone are expected to bring their
//!   own state machine matching their UI ergonomics.

mod table;

use std::sync::OnceLock;

use inputx_dict_format::IdfReader;

pub use table::{
    export_l0, import_l0, is_displayable, lookup, lookup_with_freq_layer,
    lookup_with_layer, lookup_with_scores, prefix_predictions, record_pick,
    set_show_rare, show_rare, warmup,
};
/// Re-export of the wubi L0 snapshot type so hosts can build /
/// destructure it without depending on the `inputx-wubi` crate
/// directly.
pub use inputx_wubi::L0Snapshot;

/// Embedded IDFv1 wubi dict blob, sourced from
/// `inputx-wubi-data/data/words.idf` at compile time. Each entry's
/// `EntryFlags::engine_tag()` carries the wubi `Layer` enum index
/// (v1.4.7 sub-phase A4 step 2 schema bump), so cement-side fills
/// can reconstruct `(word, layer, raw_freq)` without re-reading the
/// `inputx_wubi::WubiDict` table.
pub const EMBEDDED_WUBI_IDF: &[u8] =
    include_bytes!("../data/words.idf");

/// Process-global [`IdfReader`] over [`EMBEDDED_WUBI_IDF`]. Parses
/// the 4 MB header / FST / entry-table sections once and amortizes
/// the ~few-ms cost over the whole process lifetime; subsequent
/// `wubi_idf_reader().lookup(code)` calls are O(|code|) FST walks
/// with zero allocation per query.
pub fn wubi_idf_reader() -> &'static IdfReader<&'static [u8]> {
    static READER: OnceLock<IdfReader<&'static [u8]>> = OnceLock::new();
    READER.get_or_init(|| {
        IdfReader::from_bytes(EMBEDDED_WUBI_IDF)
            .expect("inputx-wubi-data EMBEDDED_WUBI_IDF must be a valid IDFv1 blob")
    })
}

/// Decode an IDF wubi entry's `EntryFlags::engine_tag()` back into
/// the originating `inputx_wubi::Layer` variant. Falls back to
/// `Layer::Auto` on out-of-range bytes (defensive — the writer only
/// emits 0..=5).
pub fn layer_from_idf_tag(tag: u8) -> inputx_wubi::Layer {
    inputx_wubi::Layer::from_u8(tag).unwrap_or(inputx_wubi::Layer::Auto)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wubi_idf_reader_parses_and_supports_exact_lookup_with_layer() {
        let r = wubi_idf_reader();
        assert!(r.entry_count() > 100_000);
        let hits = r.lookup(b"g");
        assert!(!hits.is_empty(), "g must have at least one Jianma1 entry");
        let yi = hits.iter().find(|e| e.word == "一");
        assert!(yi.is_some(), "g → 一 expected; got readings {:?}", hits.iter().map(|e| e.word).collect::<Vec<_>>());
        let yi = yi.unwrap();
        assert_eq!(
            layer_from_idf_tag(yi.flags.engine_tag()),
            inputx_wubi::Layer::Jianma1,
        );
    }
}
