//! `inputx-wubi-cement` — Wubi-specific engine glue.
//!
//! Cement layer (see `.claude/PLAN-stones-extract.md` "Cement catalog")
//! sitting on top of the `inputx-wubi` facade. Consumers get a
//! stateful `WubiEngine` ready to be driven keystroke-by-keystroke,
//! plus L0 (per-user pin) export/import helpers and the auto-commit
//! policy enum.
//!
//! The split: `inputx-wubi` ships data + primitives (codec, decomp,
//! dict, jianma, layer, stroke, zigen) and is publish-quality for any
//! IME implementer. `inputx-wubi-cement` is Inputx-specific business
//! logic — buffer state machine, simcode promote rules, auto-commit
//! policies, L0 snapshot model — that any third-party Wubi consumer
//! probably wants to re-implement to suit their own ergonomics.
//!
//! # Public surface (1:1 with the pre-v1.4.5 `inputx_core::wubi`
//! module so the carve-out is a pure mechanical rename for consumers).

mod table;

use std::sync::OnceLock;

use inputx_dict_format::IdfReader;

// v1.5.2 WU-λ carve-out: WubiEngine state machine + AutoCommitPolicy
// moved INTO the Inputx application (`inputx-core::wubi`) per the v1.5
// D11 taxonomy correction (cement = application source, NOT a
// published crate). This crate keeps only stone-quality data + lookup
// helpers — no per-session state, no auto-commit policy.
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
/// `data/private-dict/v0.0.1/wubi/words.idf` at compile time. Each
/// entry's `EntryFlags::engine_tag()` carries the wubi `Layer` enum
/// index (v1.4.7 sub-phase A4 step 2), so cement-side fills can
/// reconstruct `(word, layer, raw_freq)` without re-reading the
/// `inputx_wubi::WubiDict` table.
pub const EMBEDDED_WUBI_IDF: &[u8] =
    include_bytes!("../data/words.idf");

/// Process-global [`IdfReader`] over [`EMBEDDED_WUBI_IDF`]. Parses
/// the 4 MB header / FST / entry-table sections once and amortizes
/// the ~few-ms cost over the whole process lifetime; subsequent
/// `wubi_idf_reader().lookup(code)` calls are O(|code|) FST walks
/// with zero allocation per query.
///
/// Composite hot path (v1.4.7 sub-phase A4 step 2): cement-level
/// `table::lookup_with_freq_layer` and `table::prefix_predictions`
/// fills go through this reader in place of `WubiDict::lookup_with_
/// freq_layer_into` / `WubiDict::prefix_predictions`. The facade
/// `WubiDict` (and the underlying `inputx-wubi` data tables) is
/// still loaded inside `WubiEngine` because the state machine
/// (buffer / auto-commit / L0 pin) lives on the facade — but the
/// composite engine's corpus lookups no longer touch it.
pub fn wubi_idf_reader() -> &'static IdfReader<&'static [u8]> {
    static READER: OnceLock<IdfReader<&'static [u8]>> = OnceLock::new();
    READER.get_or_init(|| {
        IdfReader::from_bytes(EMBEDDED_WUBI_IDF)
            .expect("inputx-wubi-cement EMBEDDED_WUBI_IDF must be a valid IDFv1 blob")
    })
}

/// Decode an IDF wubi entry's `EntryFlags::engine_tag()` back into the
/// originating `inputx_wubi::Layer` variant. Falls back to
/// `Layer::Auto` on out-of-range bytes (defensive — the writer only
/// emits 0..=5).
pub fn layer_from_idf_tag(tag: u8) -> inputx_wubi::Layer {
    inputx_wubi::Layer::from_u8(tag).unwrap_or(inputx_wubi::Layer::Auto)
}

#[cfg(test)]
mod cement_tests {
    use super::*;

    #[test]
    fn wubi_idf_reader_parses_and_supports_exact_lookup_with_layer() {
        let r = wubi_idf_reader();
        // Embedded blob carries ~135k entries.
        assert!(r.entry_count() > 100_000);
        // `g` is a one-letter Jianma1 simcode in wubi 86; expect at
        // least one entry, and the layer tag should round-trip.
        let hits = r.lookup(b"g");
        assert!(!hits.is_empty(), "g must have at least one Jianma1 entry");
        // 一 ships in the v0.0.1 wubi dict as the canonical `g`
        // Jianma1 simcode (per inputx_wubi seed catalog).
        let yi = hits.iter().find(|e| e.word == "一");
        assert!(yi.is_some(), "g → 一 expected; got readings {:?}", hits.iter().map(|e| e.word).collect::<Vec<_>>());
        let yi = yi.unwrap();
        assert_eq!(
            layer_from_idf_tag(yi.flags.engine_tag()),
            inputx_wubi::Layer::Jianma1,
        );
    }
}
