//! `inputx-pinyin-cement` — Pinyin-specific consumer-engine cement.
//!
//! Cement layer (see `.claude/PLAN-stones-extract.md` "Cement catalog")
//! built on top of the [`inputx_pinyin`] facade plus
//! [`inputx_dict_format`] (IDFv1 dict) + [`inputx_ngram`] (NGMv1
//! bigram). Consumers get:
//!
//! - `bigram_boost_from_ngm` — runtime helper that reads a NGMv1
//!   [`NgramTable`] and returns a Q4 log-space bigram bonus
//!   compatible with the [`inputx_scoring`] schema.
//! - `PinyinIdfLookup` (v1.4.6 sub-phase C work) — IdfReader-driven
//!   lookup adapter that replaces direct `PinyinDict::lookup_into`
//!   calls in the composite pinyin adapter.
//! - L0 JSON export/import for pinyin pins (carved from
//!   inputx-core/composite/l0_json.rs).
//!
//! The split: [`inputx_pinyin`] facade ships data + lookup primitives
//! (codec / dict / syllable / fuzzy / engine) and is publish-quality
//! for any IME implementer. `inputx-pinyin-cement` is Inputx-specific
//! plumbing — anything wiring the facade into a stateful IME engine
//! that any third-party Pinyin consumer probably wants to re-implement
//! to suit their own ergonomics.
//!
//! pinyin_adapter (the stateful state machine driven by composite
//! engine) lives in inputx-core/composite — it depends on cross-engine
//! rules + mode + scoring modules and is the **composite root cement**
//! per PLAN-stones-extract.md.

pub mod freq;
pub mod ngram;

use std::sync::OnceLock;

use inputx_dict_format::IdfReader;

pub use freq::estimated_freq_from_log_prior;
pub use ngram::{bigram_boost_from_ngm, legacy_bigram_boost_from_ngm};

/// Embedded NGMv1 bigram blob for the pinyin engine, sourced from
/// `data/private-dict/v0.0.1/pinyin/bigrams.ngm` at compile time.
///
/// Composite hot path (v1.4.6 sub-phase C2 onwards) loads this once
/// via `inputx_ngram::NgramTable::from_bytes(EMBEDDED_BIGRAMS_NGM)` to
/// replace the per-call `PinyinDict::bigram_boost` lookup against the
/// facade's bundled `bigrams.fsa`.
pub const EMBEDDED_BIGRAMS_NGM: &[u8] =
    include_bytes!("../data/bigrams.ngm");

/// Embedded IDFv1 pinyin dict blob, sourced from
/// `data/private-dict/v0.0.1/pinyin/words.idf` at compile time. Already
/// carries prior_correction multipliers baked into `log_prior_q4`
/// (v1.4.6 sub-phase B1) and a populated FST code index (sub-phase C1)
/// so lookup is O(|code|).
///
/// Composite hot path (v1.4.6 sub-phase C3 onwards) loads this once
/// via `inputx_dict_format::IdfReader::from_bytes(EMBEDDED_PINYIN_IDF)`
/// to replace the per-call `PinyinDict::lookup_with_scores_into`
/// against the facade's bundled `pinyin.dict` (which lacks the
/// prior_correction absorb and is also being phased out).
pub const EMBEDDED_PINYIN_IDF: &[u8] =
    include_bytes!("../data/words.idf");

/// Process-global [`IdfReader`] over [`EMBEDDED_PINYIN_IDF`]. Parses the
/// 9 MB header / FST / entry-table sections once and amortizes the
/// ~few-ms cost over the whole process lifetime; subsequent
/// `pinyin_idf_reader().lookup(code)` calls are O(|code|) FST walks
/// with zero allocation per query.
///
/// Composite hot path (v1.4.7 sub-phase A4): the pinyin adapter's
/// exact / fuzzy / prefix-prediction fills go through this reader in
/// place of `inputx_pinyin::PinyinDict::lookup_with_freq_into` /
/// `PinyinDict::lookup_into` / `PinyinDict::prefix_for_each_raw`. The
/// underlying `.idf` already carries
/// `prior_correction` Q4 boosts baked into `log_prior_q4` (sub-phase
/// B1 build-time absorb) so cement-side `prior_correction.rs`
/// recomputation can retire (sub-phase A5).
pub fn pinyin_idf_reader() -> &'static IdfReader<&'static [u8]> {
    static READER: OnceLock<IdfReader<&'static [u8]>> = OnceLock::new();
    READER.get_or_init(|| {
        IdfReader::from_bytes(EMBEDDED_PINYIN_IDF)
            .expect("inputx-pinyin-cement EMBEDDED_PINYIN_IDF must be a valid IDFv1 blob")
    })
}

#[cfg(test)]
mod cement_tests {
    use super::*;

    #[test]
    fn pinyin_idf_reader_parses_and_supports_exact_lookup() {
        let r = pinyin_idf_reader();
        // Smoke test that the embedded blob parses + carries reasonable
        // entry counts (~237k entries per the v0.0.1 manifest) + lookup
        // round-trips for a well-known seed word.
        assert!(r.entry_count() > 100_000);
        let hits = r.lookup(b"jixu");
        assert!(!hits.is_empty(), "jixu must have at least one reading");
        let words: Vec<&str> = hits.iter().map(|e| e.word).collect();
        // 继续 ships in the v0.0.1 pinyin .idf (canon polish-log entry).
        assert!(words.contains(&"继续"), "jixu → 继续 expected, got {words:?}");
    }

    #[test]
    fn pinyin_idf_reader_prefix_for_each_streams_in_order() {
        let r = pinyin_idf_reader();
        // Prefix `jix` is small enough to enumerate exhaustively; ensure
        // streaming visit reports at least one `jixu` entry.
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
}
