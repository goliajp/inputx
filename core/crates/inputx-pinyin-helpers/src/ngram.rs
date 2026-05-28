//! NGMv1 bigram-boost wire helper for the pinyin composite path.
//!
//! Replaces direct `inputx_pinyin::PinyinDict::bigram_boost(prev,
//! next)` (which reads an embedded `bigrams.fsa`) with a `NgramTable`-
//! backed lookup (reads `data/private-dict/v0.0.1/pinyin/bigrams.ngm`).
//!
//! The output value is in **Q4 log-space** ([`inputx_scoring::Q4`] =
//! 16) — the cement layer that adds this to a candidate's
//! `log_likelihood` is doing strict log-space accumulation, NOT the
//! legacy f64 `bigram_boost = MAX × ln(1+count) / ln(1+REF)` linear-
//! space additive bonus.
//!
//! For the v1.3 → v1.4.6 cutover, callers wrap this in a small adapter
//! that emits a comparable scalar in the legacy units the composite
//! merge currently sorts on. The clean cement-layer-only call site
//! will appear post-v1.4.6.

use inputx_ngram::{NgramTable, Q4};

/// Bigram bonus (Q4 log-space) for the `(prev, next)` pair, looked up
/// in the supplied [`NgramTable`]. Returns `0` if:
/// - `prev` is `None` (cold session)
/// - `prev` or `next` is empty
/// - the bigram isn't in the table (rare pair)
///
/// The table stores `log_prob_q4 = Q4 · ln(count)`; we return that
/// scalar directly. To convert to the legacy f64
/// `BIGRAM_BOOST_MAX × ln(1+count) / ln(1+BIGRAM_REF)` calibration, use
/// [`legacy_bigram_boost_from_ngm`] below.
pub fn bigram_boost_from_ngm<B: AsRef<[u8]>>(
    table: &NgramTable<B>,
    prev: Option<&str>,
    next: &str,
) -> i16 {
    let Some(prev) = prev else { return 0 };
    if prev.is_empty() || next.is_empty() {
        return 0;
    }
    table.log_prob(&[prev], next).unwrap_or(0)
}

/// Legacy-units bigram bonus (f64, additive on the v1.3 cross-engine
/// score scale). Bridges the new NGMv1 backing to the existing pinyin
/// composite merge formula. Replicates the v1.3 calibration:
/// `BIGRAM_BOOST_MAX (= 50_000) × ln(1 + count) / ln(1 + BIGRAM_REF
/// (= 1000))` — caps at MAX, log-scaled so count=1 and count=100_000
/// don't differ by 100_000×.
///
/// Inverts the Q4 log_prob in the table back to a raw count estimate:
/// `count ≈ exp(log_prob_q4 / Q4)`. Acceptable round-trip error vs
/// the original count: ±1 count at the bottom of the curve, ±100 at
/// count=100_000 (where the score has saturated anyway).
pub fn legacy_bigram_boost_from_ngm<B: AsRef<[u8]>>(
    table: &NgramTable<B>,
    prev: Option<&str>,
    next: &str,
) -> f64 {
    const BIGRAM_BOOST_MAX: f64 = 50_000.0;
    const BIGRAM_REF: f64 = 1000.0;
    let log_prob_q4 = bigram_boost_from_ngm(table, prev, next);
    if log_prob_q4 == 0 {
        return 0.0;
    }
    let ln_count = log_prob_q4 as f64 / Q4 as f64;
    let count = ln_count.exp();
    let scaled = BIGRAM_BOOST_MAX * (1.0 + count).ln() / (1.0 + BIGRAM_REF).ln();
    scaled.min(BIGRAM_BOOST_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use inputx_ngram::NgramBuilder;
    use tempfile::tempdir;

    fn build_table_with(entries: &[(&str, &str, i16)]) -> Vec<u8> {
        let dir = tempdir().unwrap();
        let p = dir.path().join("t.ngm");
        let mut b = NgramBuilder::new(2);
        for (prev, next, log_prob) in entries {
            b.add(&[prev], next, *log_prob);
        }
        b.build(&p).unwrap();
        std::fs::read(&p).unwrap()
    }

    #[test]
    fn boost_returns_zero_for_unknown_or_empty() {
        let bytes = build_table_with(&[("今天", "是", 100)]);
        let t = NgramTable::from_bytes(bytes).unwrap();
        assert_eq!(bigram_boost_from_ngm(&t, None, "是"), 0);
        assert_eq!(bigram_boost_from_ngm(&t, Some(""), "是"), 0);
        assert_eq!(bigram_boost_from_ngm(&t, Some("今天"), ""), 0);
        assert_eq!(bigram_boost_from_ngm(&t, Some("unknown"), "x"), 0);
    }

    #[test]
    fn boost_returns_table_log_prob_q4() {
        let bytes = build_table_with(&[
            ("今天", "是", 250),
            ("我们", "的", 300),
        ]);
        let t = NgramTable::from_bytes(bytes).unwrap();
        assert_eq!(bigram_boost_from_ngm(&t, Some("今天"), "是"), 250);
        assert_eq!(bigram_boost_from_ngm(&t, Some("我们"), "的"), 300);
    }

    #[test]
    fn legacy_boost_monotone_in_count() {
        let bytes = build_table_with(&[
            // log_prob_q4 = Q4 · ln(count). Q4=16.
            // count=10 → ln(10)·16 = 36.84 → 37
            // count=1000 → ln(1000)·16 = 110.5 → 111
            // count=100000 → ln(1e5)·16 = 184.1 → 184
            ("a", "b", 37),
            ("c", "d", 111),
            ("e", "f", 184),
        ]);
        let t = NgramTable::from_bytes(bytes).unwrap();
        let low = legacy_bigram_boost_from_ngm(&t, Some("a"), "b");
        let mid = legacy_bigram_boost_from_ngm(&t, Some("c"), "d");
        let high = legacy_bigram_boost_from_ngm(&t, Some("e"), "f");
        // Monotone non-decreasing; cap saturates at MAX past count ≈ 1000.
        // count=10 lands well under the cap (~17k); count≥1000 saturates
        // (matches v1.3 behaviour where log scaling pins anything past
        // REF=1000 to the same ~50k bonus — bigram strength saturates
        // once it's "clearly a common pair").
        assert!(low > 0.0 && low < mid && mid <= high && high <= 50_000.0,
            "low={low} mid={mid} high={high}");
    }
}
