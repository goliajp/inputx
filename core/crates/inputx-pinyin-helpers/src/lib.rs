//! `inputx-pinyin-helpers` — embedded Pinyin IDFv1 dict + NGMv1
//! bigram blob + stateless lookup helpers for the
//! [`inputx-pinyin`](https://crates.io/crates/inputx-pinyin) engine,
//! packaged as a publishable stone.
//!
//! Successor to [`inputx-pinyin-cement`](https://crates.io/crates/inputx-pinyin-cement)
//! under the v1.5 D11 taxonomy correction (2026-05): cement =
//! application source code, NOT a published crate. The old
//! `-cement`-suffix crate is deprecated and re-exports from this
//! crate for backward compat.
//!
//! ## What's in the box
//!
//! - [`EMBEDDED_PINYIN_IDF`] — IDFv1 binary blob (~9 MB, 237k
//!   entries) with prior_correction Q4 boosts baked into
//!   `log_prior_q4` (v1.4.7 sub-phase A5). Composite hot path loads
//!   this in place of the facade `PinyinDict::lookup_*` family.
//! - [`EMBEDDED_BIGRAMS_NGM`] — NGMv1 binary blob (~595 KB, 64k
//!   triplets); same Q4 log-prob data as the facade's bundled
//!   `bigrams.fsa`, but packaged for `inputx_ngram::NgramTable`
//!   consumption.
//! - [`pinyin_idf_reader`] — process-global `OnceLock<IdfReader>`.
//! - [`bigram_boost_from_ngm`] — runtime helper that reads a NGMv1
//!   table and returns a Q4 log-space bigram bonus compatible with
//!   the [`inputx_scoring`] schema.
//! - [`legacy_bigram_boost_from_ngm`] — v1.3-calibration bridge for
//!   the transitional sort-key window; capped at a fixed maximum,
//!   used by the composite-side cement for backward-compatible f64
//!   ordering on top of the Q4 sort.
//! - [`estimated_freq_from_log_prior`] — inverse of
//!   `inputx_scoring::log_prior_from_freq`. Recovers an estimated
//!   raw frequency from the Q4 log_prior. Round-trip drift ≤ 1% at
//!   typical bigram counts.
//!
//! ## What's NOT here
//!
//! - **Stateful pinyin engine** (`PinyinAdapter` state machine /
//!   handle_letter / commit_index) — that classifies as application
//!   cement per the v1.5 D11 correction and lives in the Inputx
//!   monorepo's [`inputx-core/src/composite/pinyin_adapter.rs`](https://github.com/goliajp/inputx/blob/develop/core/crates/inputx-core/src/composite/pinyin_adapter.rs).
//!   IME implementers copying this stone are expected to bring their
//!   own state machine.

pub mod freq;
pub mod ngram;

use std::borrow::Cow;
use std::sync::{Arc, OnceLock};

use arc_swap::ArcSwap;
use inputx_dict_format::IdfReader;

pub use freq::estimated_freq_from_log_prior;
pub use ngram::{bigram_boost_from_ngm, combined_bigram_log_prob_q4, legacy_bigram_boost_from_ngm};

/// Byte container for the process-global [`IdfReader`]: either a
/// borrow of the compile-time embedded blob or an owned `Vec<u8>`
/// loaded from disk after a hot-reload. `Cow` unifies the two under
/// one `AsRef<[u8]>` impl so callers of [`pinyin_idf_reader`] don't
/// see the storage difference.
pub type IdfBytes = Cow<'static, [u8]>;

/// Embedded NGMv1 bigram blob for the pinyin engine, sourced from
/// `inputx-pinyin-helpers/data/bigrams.ngm` at compile time. Carries
/// intra-token char-pair counts (pairs WITHIN a single dict word) —
/// the v1.3 cement layer's primary bigram signal.
pub const EMBEDDED_BIGRAMS_NGM: &[u8] = include_bytes!("../data/bigrams.ngm");

/// Embedded NGMv1 inter-token bigram blob, sourced from
/// `inputx-pinyin-helpers/data/bigrams_inter.ngm` at compile time.
/// Carries token-pair counts ACROSS dict-word boundaries (corpus-level
/// sentence adjacency). v1.14 K-best 3-segment chain gate consults
/// this in addition to [`EMBEDDED_BIGRAMS_NGM`] so an adjacency like
/// `(用, 不)` (yongbuliao → 用不了, real Chinese) is recognized even
/// when neither pair appears as an intra-word bigram. Built by
/// `cargo run --bin build-inter-bigrams-ngm`.
pub const EMBEDDED_INTER_BIGRAMS_NGM: &[u8] = include_bytes!("../data/bigrams_inter.ngm");

/// Embedded IDFv1 pinyin dict blob, sourced from
/// `inputx-pinyin-helpers/data/words.idf` at compile time. Carries
/// prior_correction Q4 boosts baked into `log_prior_q4` (v1.4.7
/// sub-phase A5) and a populated FST code index (sub-phase C1) so
/// lookup is O(|code|).
pub const EMBEDDED_PINYIN_IDF: &[u8] = include_bytes!("../data/words.idf");

/// Process-global [`IdfReader`] over an `IdfBytes` (embedded blob at
/// startup, replaceable by [`set_pinyin_idf_bytes`] after a hot-reload
/// so a running IME picks up freshly-baked polish data without exit).
/// Parses the 9 MB header / FST / entry-table sections once per swap
/// and amortizes the ~few-ms cost over the whole process lifetime;
/// subsequent `pinyin_idf_reader().lookup(code)` calls are O(|code|)
/// FST walks with zero allocation per query.
///
/// The returned `Arc<IdfReader<IdfBytes>>` derefs to `&IdfReader<…>`,
/// so existing call sites like `pinyin_idf_reader().lookup(b"jixu")`
/// keep working. Hold the `Arc` in a local for the duration of a
/// keystroke to guarantee a consistent snapshot even if
/// [`set_pinyin_idf_bytes`] swaps concurrently.
pub fn pinyin_idf_reader() -> Arc<IdfReader<IdfBytes>> {
    idf_reader_slot().load_full()
}

fn idf_reader_slot() -> &'static ArcSwap<IdfReader<IdfBytes>> {
    static SLOT: OnceLock<ArcSwap<IdfReader<IdfBytes>>> = OnceLock::new();
    SLOT.get_or_init(|| {
        let reader = IdfReader::from_bytes(Cow::Borrowed(EMBEDDED_PINYIN_IDF))
            .expect("inputx-pinyin-helpers EMBEDDED_PINYIN_IDF must be a valid IDFv1 blob");
        ArcSwap::from_pointee(reader)
    })
}

/// Replace the process-global `IdfReader` with one parsed from
/// `bytes` (owned). Returns the previous entry count vs. the new one
/// so the caller can log a reload delta.
///
/// The old `Arc<IdfReader<…>>` returned by any earlier
/// [`pinyin_idf_reader`] call remains valid — `ArcSwap` only flips
/// the pointer new callers see; existing references drop when the
/// last holder finishes its work. That's why keystroke-scoped code
/// must snapshot once at entry, so a mid-lookup swap can't produce a
/// half-old / half-new answer.
///
/// Returns [`IdfReloadError`] on parse failure. The slot is left
/// untouched on error — old dict stays in place.
pub fn set_pinyin_idf_bytes(bytes: Vec<u8>) -> Result<IdfReloadReport, IdfReloadError> {
    let reader = IdfReader::from_bytes(Cow::Owned(bytes)).map_err(IdfReloadError::Parse)?;
    let new_count = reader.entry_count();
    let old = idf_reader_slot().swap(Arc::new(reader));
    // Invalidate the memoized corpus-total; the sum depends on the
    // new entry set. Best-effort: OnceLock has no reset, so we tuck
    // the total behind an ArcSwap<Option<u64>> that swaps on reload.
    corpus_total_slot().store(Arc::new(None));
    Ok(IdfReloadReport {
        old_entry_count: old.entry_count(),
        new_entry_count: new_count,
    })
}

/// Outcome of [`set_pinyin_idf_bytes`]: before/after entry count so
/// callers can log a one-line summary without holding either reader.
#[derive(Debug, Clone, Copy)]
pub struct IdfReloadReport {
    pub old_entry_count: u32,
    pub new_entry_count: u32,
}

/// Failure modes for [`set_pinyin_idf_bytes`]. `Parse` wraps the
/// underlying `inputx-dict-format` error verbatim so the caller can
/// surface the exact byte-level reason (bad magic, truncated FST, …).
#[derive(Debug)]
pub enum IdfReloadError {
    Parse(inputx_dict_format::reader::OpenError),
}

impl std::fmt::Display for IdfReloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "IdfReader parse failed: {e:?}"),
        }
    }
}

impl std::error::Error for IdfReloadError {}

/// Sum of `raw_freq` across all entries in the CURRENT process-global
/// IDF. Cheap after first call (memoized via [`ArcSwap`]); recomputed
/// on the next call after [`set_pinyin_idf_bytes`] invalidates the
/// slot. The corpus-total denominator for
/// [`inputx_scoring::log_prob_corpus_from_freq`] on the pinyin engine.
///
/// MUST agree with the total `idf_from_pinyin_dict.rs` uses when
/// baking `log_prior_q4` — both compute `Σ raw_freq` over the same set
/// of entries (post-exclusions + additions, the actual .idf rows).
pub fn pinyin_corpus_total() -> u64 {
    let slot = corpus_total_slot();
    if let Some(total) = **slot.load() {
        return total;
    }
    let computed: u64 = pinyin_idf_reader()
        .entries()
        .map(|e| e.raw_freq as u64)
        .sum();
    slot.store(Arc::new(Some(computed)));
    computed
}

fn corpus_total_slot() -> &'static ArcSwap<Option<u64>> {
    static SLOT: OnceLock<ArcSwap<Option<u64>>> = OnceLock::new();
    SLOT.get_or_init(|| ArcSwap::from_pointee(None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinyin_idf_reader_parses_and_supports_exact_lookup() {
        let r = pinyin_idf_reader();
        assert!(r.entry_count() > 100_000);
        let hits = r.lookup(b"jixu");
        assert!(!hits.is_empty(), "jixu must have at least one reading");
        let words: Vec<&str> = hits.iter().map(|e| e.word).collect();
        assert!(
            words.contains(&"继续"),
            "jixu → 继续 expected, got {words:?}"
        );
    }

    #[test]
    fn pinyin_idf_reader_prefix_for_each_streams_in_order() {
        let r = pinyin_idf_reader();
        let mut seen_jixu = false;
        let mut count: usize = 0;
        r.prefix_for_each_entry(b"jix", |e| {
            count += 1;
            if e.code == "jixu" {
                seen_jixu = true;
            }
        });
        assert!(count > 0);
        assert!(seen_jixu);
    }

    /// v1.15 hot-reload: feeding the process-global slot the SAME
    /// embedded bytes must yield a reader with the same entry count
    /// and same top-1 word for `jixu`. Also verifies the returned
    /// `Arc` from a pre-reload call keeps producing valid entries
    /// after the swap (the ArcSwap contract).
    #[test]
    fn set_pinyin_idf_bytes_round_trip_preserves_lookup() {
        let pre = pinyin_idf_reader();
        let pre_count = pre.entry_count();
        let pre_jixu: Vec<String> = pre
            .lookup(b"jixu")
            .iter()
            .map(|e| e.word.to_string())
            .collect();
        assert!(!pre_jixu.is_empty());

        let report = set_pinyin_idf_bytes(EMBEDDED_PINYIN_IDF.to_vec())
            .expect("re-parsing the embedded bytes must succeed");
        assert_eq!(report.old_entry_count, pre_count);
        assert_eq!(report.new_entry_count, pre_count);

        // Old Arc still valid — ArcSwap only flips the pointer new
        // callers see; the old snapshot lives until its last holder
        // drops. This is what makes mid-keystroke reload safe.
        let post_jixu_on_old_arc: Vec<String> = pre
            .lookup(b"jixu")
            .iter()
            .map(|e| e.word.to_string())
            .collect();
        assert_eq!(post_jixu_on_old_arc, pre_jixu);

        // New callers see the new snapshot (same content, different
        // allocation).
        let post = pinyin_idf_reader();
        let post_jixu: Vec<String> = post
            .lookup(b"jixu")
            .iter()
            .map(|e| e.word.to_string())
            .collect();
        assert_eq!(post_jixu, pre_jixu);
    }
}
