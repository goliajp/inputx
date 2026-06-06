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

use std::sync::OnceLock;

use inputx_dict_format::IdfReader;

pub use freq::estimated_freq_from_log_prior;
pub use ngram::{bigram_boost_from_ngm, combined_bigram_log_prob_q4, legacy_bigram_boost_from_ngm};

/// Embedded NGMv1 bigram blob for the pinyin engine, sourced from
/// `inputx-pinyin-helpers/data/bigrams.ngm` at compile time. Carries
/// intra-token char-pair counts (pairs WITHIN a single dict word) —
/// the v1.3 cement layer's primary bigram signal.
pub const EMBEDDED_BIGRAMS_NGM: &[u8] =
    include_bytes!("../data/bigrams.ngm");

/// Embedded NGMv1 inter-token bigram blob, sourced from
/// `inputx-pinyin-helpers/data/bigrams_inter.ngm` at compile time.
/// Carries token-pair counts ACROSS dict-word boundaries (corpus-level
/// sentence adjacency). v1.14 K-best 3-segment chain gate consults
/// this in addition to [`EMBEDDED_BIGRAMS_NGM`] so an adjacency like
/// `(用, 不)` (yongbuliao → 用不了, real Chinese) is recognized even
/// when neither pair appears as an intra-word bigram. Built by
/// `cargo run --bin build-inter-bigrams-ngm`.
pub const EMBEDDED_INTER_BIGRAMS_NGM: &[u8] =
    include_bytes!("../data/bigrams_inter.ngm");

/// Embedded IDFv1 pinyin dict blob, sourced from
/// `inputx-pinyin-helpers/data/words.idf` at compile time. Carries
/// prior_correction Q4 boosts baked into `log_prior_q4` (v1.4.7
/// sub-phase A5) and a populated FST code index (sub-phase C1) so
/// lookup is O(|code|).
pub const EMBEDDED_PINYIN_IDF: &[u8] =
    include_bytes!("../data/words.idf");

/// Process-global [`IdfReader`] over [`EMBEDDED_PINYIN_IDF`]. Parses
/// the 9 MB header / FST / entry-table sections once and amortizes
/// the ~few-ms cost over the whole process lifetime; subsequent
/// `pinyin_idf_reader().lookup(code)` calls are O(|code|) FST walks
/// with zero allocation per query.
pub fn pinyin_idf_reader() -> &'static IdfReader<&'static [u8]> {
    static READER: OnceLock<IdfReader<&'static [u8]>> = OnceLock::new();
    READER.get_or_init(|| {
        IdfReader::from_bytes(EMBEDDED_PINYIN_IDF)
            .expect("inputx-pinyin-helpers EMBEDDED_PINYIN_IDF must be a valid IDFv1 blob")
    })
}

/// Sum of `raw_freq` across all entries in [`EMBEDDED_PINYIN_IDF`].
/// The corpus-total denominator for
/// [`inputx_scoring::log_prob_corpus_from_freq`] on the pinyin engine.
/// Process-global `OnceLock` — computed once via a linear scan of the
/// .idf, then memoized.
///
/// MUST agree with the total `idf_from_pinyin_dict.rs` uses when
/// baking `log_prior_q4` — both compute `Σ raw_freq` over the same set
/// of entries (post-exclusions + additions, the actual .idf rows).
pub fn pinyin_corpus_total() -> u64 {
    static TOTAL: OnceLock<u64> = OnceLock::new();
    *TOTAL.get_or_init(|| {
        pinyin_idf_reader()
            .entries()
            .map(|e| e.raw_freq as u64)
            .sum()
    })
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
        assert!(words.contains(&"继续"), "jixu → 继续 expected, got {words:?}");
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
}
