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

use std::borrow::Cow;
use std::sync::{Arc, OnceLock};

use arc_swap::ArcSwap;
use inputx_dict_format::IdfReader;

/// Re-export of the wubi L0 snapshot type so hosts can build /
/// destructure it without depending on the `inputx-wubi` crate
/// directly.
pub use inputx_wubi::L0Snapshot;
pub use table::{
    export_l0, import_l0, is_displayable, lookup, lookup_with_freq_layer, lookup_with_layer,
    lookup_with_scores, pinned_word, prefix_predictions, record_pick, set_show_rare,
    set_wubi_dict_bytes, show_rare, warmup,
};

/// Embedded IDFv1 wubi dict blob, sourced from
/// `inputx-wubi-data/data/words.idf` at compile time. Each entry's
/// `EntryFlags::engine_tag()` carries the wubi `Layer` enum index
/// (v1.4.7 sub-phase A4 step 2 schema bump), so cement-side fills
/// can reconstruct `(word, layer, raw_freq)` without re-reading the
/// `inputx_wubi::WubiDict` table.
pub const EMBEDDED_WUBI_IDF: &[u8] = include_bytes!("../data/words.idf");

/// Byte container for the process-global reader: borrowed from
/// [`EMBEDDED_WUBI_IDF`] at startup, owned after a hot-reload.
pub type IdfBytes = Cow<'static, [u8]>;

/// Process-global [`IdfReader`] over an [`IdfBytes`] (the embedded blob
/// at startup, replaceable by [`set_wubi_idf_bytes`] after a hot-reload
/// so a running IME picks up freshly-baked polish without exiting).
/// Parses the 4 MB header / FST / entry-table sections once per swap
/// and amortizes the ~few-ms cost over the process lifetime;
/// subsequent `wubi_idf_reader().lookup(code)` calls are O(|code|) FST
/// walks with zero allocation per query.
///
/// The returned `Arc<IdfReader<…>>` derefs to `&IdfReader<…>`, so
/// existing call sites keep working unchanged. Hold the `Arc` in a
/// local for the duration of a keystroke to guarantee a consistent
/// snapshot even if [`set_wubi_idf_bytes`] swaps concurrently.
pub fn wubi_idf_reader() -> Arc<IdfReader<IdfBytes>> {
    idf_reader_slot().load_full()
}

fn idf_reader_slot() -> &'static ArcSwap<IdfReader<IdfBytes>> {
    static SLOT: OnceLock<ArcSwap<IdfReader<IdfBytes>>> = OnceLock::new();
    SLOT.get_or_init(|| {
        let reader = IdfReader::from_bytes(Cow::Borrowed(EMBEDDED_WUBI_IDF))
            .expect("inputx-wubi-data EMBEDDED_WUBI_IDF must be a valid IDFv1 blob");
        ArcSwap::from_pointee(reader)
    })
}

/// Replace the process-global wubi `IdfReader` with one parsed from
/// `bytes` (owned). Returns the previous vs. new entry count so the
/// caller can log a reload delta.
///
/// The old `Arc` handed out by an earlier [`wubi_idf_reader`] call
/// stays valid — `ArcSwap` only flips the pointer new callers see, and
/// existing references drop when their last holder finishes. That is
/// what makes a mid-keystroke reload safe.
///
/// On parse failure the slot is left untouched and the old dict stays
/// in place. Mirrors `inputx_pinyin_helpers::set_pinyin_idf_bytes`.
pub fn set_wubi_idf_bytes(bytes: Vec<u8>) -> Result<IdfReloadReport, IdfReloadError> {
    let reader = IdfReader::from_bytes(Cow::Owned(bytes)).map_err(IdfReloadError::Parse)?;
    let new_count = reader.entry_count();
    let old = idf_reader_slot().swap(Arc::new(reader));
    // The corpus total is a sum over the entry set, so it has to die
    // with the old reader.
    corpus_total_slot().store(Arc::new(None));
    Ok(IdfReloadReport {
        old_entry_count: old.entry_count(),
        new_entry_count: new_count,
    })
}

/// Outcome of [`set_wubi_idf_bytes`]: before/after entry count, so a
/// caller can log a one-line summary without holding either reader.
#[derive(Debug, Clone, Copy)]
pub struct IdfReloadReport {
    pub old_entry_count: u32,
    pub new_entry_count: u32,
}

/// Failure modes for [`set_wubi_idf_bytes`]. `Parse` wraps the
/// underlying `inputx-dict-format` error verbatim so the caller can
/// surface the exact byte-level reason (bad magic, truncated FST, …).
#[derive(Debug)]
pub enum IdfReloadError {
    Parse(inputx_dict_format::reader::OpenError),
}

impl std::fmt::Display for IdfReloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "wubi IdfReader parse failed: {e:?}"),
        }
    }
}

impl std::error::Error for IdfReloadError {}

/// Sum of `raw_freq` across all entries in the CURRENT process-global
/// wubi IDF. The corpus-total denominator for
/// [`inputx_scoring::log_prob_corpus_from_freq`] on the wubi engine.
/// Cheap after the first call (memoized via [`ArcSwap`]); recomputed on
/// the next call after [`set_wubi_idf_bytes`] invalidates the slot.
///
/// This MUST match the total the builder bin used when baking the
/// `log_prior_q4` field: see `idf_from_wubi_tables.rs`. Both compute
/// `Σ raw_freq` over the same set of entries (the .idf is built from
/// the embedded wubi dict with no filtering), so they agree by
/// construction.
pub fn wubi_corpus_total() -> u64 {
    let slot = corpus_total_slot();
    if let Some(total) = **slot.load() {
        return total;
    }
    let computed: u64 = wubi_idf_reader().entries().map(|e| e.raw_freq as u64).sum();
    slot.store(Arc::new(Some(computed)));
    computed
}

fn corpus_total_slot() -> &'static ArcSwap<Option<u64>> {
    static SLOT: OnceLock<ArcSwap<Option<u64>>> = OnceLock::new();
    SLOT.get_or_init(|| ArcSwap::from_pointee(None))
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
        assert!(
            yi.is_some(),
            "g → 一 expected; got readings {:?}",
            hits.iter().map(|e| e.word).collect::<Vec<_>>()
        );
        let yi = yi.unwrap();
        assert_eq!(
            layer_from_idf_tag(yi.flags.engine_tag()),
            inputx_wubi::Layer::Jianma1,
        );
    }

    #[test]
    fn set_wubi_idf_bytes_round_trip_preserves_lookup_and_layer() {
        let pre = wubi_idf_reader();
        let pre_count = pre.entry_count();
        let pre_g: Vec<(String, u8)> = pre
            .lookup(b"g")
            .iter()
            .map(|e| (e.word.into(), e.flags.engine_tag()))
            .collect();
        assert!(!pre_g.is_empty());

        let report = set_wubi_idf_bytes(EMBEDDED_WUBI_IDF.to_vec())
            .expect("re-parsing the embedded bytes must succeed");
        assert_eq!(report.old_entry_count, pre_count);
        assert_eq!(report.new_entry_count, pre_count);

        // The pre-swap Arc still answers — ArcSwap only flips what NEW
        // callers see, which is what makes a mid-keystroke reload safe.
        let on_old: Vec<(String, u8)> = pre
            .lookup(b"g")
            .iter()
            .map(|e| (e.word.into(), e.flags.engine_tag()))
            .collect();
        assert_eq!(on_old, pre_g);

        // Layer tags survive the swap, not just the words — the engine_tag
        // bits are what `layer_from_idf_tag` reads for ranking.
        let post: Vec<(String, u8)> = wubi_idf_reader()
            .lookup(b"g")
            .iter()
            .map(|e| (e.word.into(), e.flags.engine_tag()))
            .collect();
        assert_eq!(post, pre_g);
    }

    #[test]
    fn set_wubi_idf_bytes_rejects_garbage_and_keeps_old_reader() {
        let pre_count = wubi_idf_reader().entry_count();
        assert!(set_wubi_idf_bytes(b"not an idf blob".to_vec()).is_err());
        assert_eq!(
            wubi_idf_reader().entry_count(),
            pre_count,
            "a rejected reload must leave the previous dict in place"
        );
    }

    #[test]
    fn corpus_total_recomputes_after_reload() {
        let before = wubi_corpus_total();
        assert!(before > 0);
        set_wubi_idf_bytes(EMBEDDED_WUBI_IDF.to_vec()).expect("round-trip reload");
        // Same bytes → same sum, but it must have been RE-derived rather
        // than served from a slot the swap forgot to invalidate. A stale
        // total silently skews every log_prob_corpus_from_freq call.
        assert_eq!(wubi_corpus_total(), before);
    }
}
