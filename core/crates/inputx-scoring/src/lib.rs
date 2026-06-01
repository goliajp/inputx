//! `inputx-scoring` — probability-native candidate scoring primitive.
//!
//! # The schema
//!
//! Every candidate carries two log-space scalars and a typed match
//! classification:
//!
//! ```text
//!     score(W | i) = log_prior(W) + log_likelihood(i | W)
//! ```
//!
//! This is the Bayesian decomposition `P(W|i) ∝ P(i|W) · P(W)` rendered
//! in log space — addition replaces multiplication, comparison stays
//! monotone, and the two factors can be assigned independently by
//! producers (dictionaries, n-grams, fuzzy matchers, …) without
//! coordinating a single global formula.
//!
//! Both terms are `i32` Q4 fixed-point. One log unit = [`Q4`] (= 16)
//! integer steps. The Q4 choice trades resolution for headroom: scores
//! fit comfortably in `i32` even for very rare or very common words
//! while staying precise enough that ranking-relevant gaps (~0.0625 in
//! log space) survive quantization.
//!
//! # Public surface
//!
//! - [`Source`]  — which engine produced the candidate
//! - [`MatchType`] — how the typed input maps to the candidate
//! - [`Candidate`] — a (word, log_prior, log_likelihood, match_type, source) tuple
//! - [`score`] — `log_prior + log_likelihood`, the sort key
//! - [`Q4`] — log-to-integer scale (= 16)
//!
//! Scoring policy lives in the consumer (IME engine cement); this
//! crate provides only the schema and the additive sort key.

#![cfg_attr(not(feature = "std"), no_std)]

/// Fixed-point scale for log-space scalars. `Q4 = 16` means every
/// integer step is 1/16 of a log unit (≈ 0.0625). At this resolution,
/// `i32` covers a dynamic range of ~ ±67 million log units — far more
/// than any realistic candidate score needs.
pub const Q4: i32 = 16;

/// Engine that produced a candidate. Numeric representation is stable
/// across versions so it can cross the FFI boundary as `u8` without
/// translation. Mirrors `inputx_core::composite::merge::Source`.
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    Wubi = 0,
    Pinyin = 1,
    Japanese = 2,
}

/// Classification of how the typed input `i` maps to a candidate `W`.
/// Carried alongside the (log_prior, log_likelihood) pair so the
/// downstream merger / probe / UI can render context without
/// re-deriving the match shape.
///
/// The numeric payloads (`u16` milli scales for proximity / cost,
/// `u8` link count for composed) are intentionally narrow — they're
/// classification metadata, not the score itself. Scoring lives in
/// `log_likelihood`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum MatchType {
    /// Typed input matches the candidate's full code exactly.
    Exact,
    /// Typed input is a prefix of the candidate's full code.
    /// Payload: `proximity_milli` = `typed_len * 1000 / full_len`,
    /// `1000` meaning "fully typed" (which collapses to `Exact` for
    /// producers that wish to distinguish).
    Prefix(u16),
    /// Typed input differs from the candidate by a phonetic edit
    /// distance. Payload: `edit_cost_milli` ∈ `[0, 1000]`, scaled
    /// against `inputx_phonetic_edit`'s max-cost convention.
    Fuzzy(u16),
    /// Typed input was decomposed into a chain of dict segments by a
    /// Viterbi / DP composer. Payload: `bigram_links` = `chain_len − 1`,
    /// the number of segment-to-segment bigram joins.
    Composed { bigram_links: u8 },
}

/// One scored candidate. Word lifetime is `'a` so consumers can pass
/// borrowed references through the merge pipeline; the merger clones
/// only the survivors.
///
/// Equality intentionally compares `word + source + match_type`, not
/// the score — score is a sort key, not part of identity. (Two
/// producers that emit the same word with the same `match_type` and
/// `source` are duplicates regardless of how they scored it.)
#[derive(Copy, Clone, Debug)]
pub struct Candidate<'a> {
    pub word: &'a str,
    /// `Q4 · log(prior probability)`. Producers derive from corpus
    /// frequency (`Q4 · log(1 + freq)` is the canonical baseline) plus
    /// any user-level boost (L0 pin, recency, etc.). Non-negative by
    /// convention but the schema accepts negative for explicit
    /// down-weights.
    pub log_prior: i32,
    /// `Q4 · log(match likelihood)`. Producers derive from the match
    /// shape: per-type base (e.g. `LIKELIHOOD_JP_JUKUGO_BASE`),
    /// proximity decay for prefix matches (`Q4 · K · log(proximity)`),
    /// edit-cost penalty for fuzzy matches, demote/promote factors
    /// (TC demote, full-match promote, …) folded in additively in log
    /// space.
    pub log_likelihood: i32,
    pub match_type: MatchType,
    pub source: Source,
}

impl<'a> PartialEq for Candidate<'a> {
    fn eq(&self, other: &Self) -> bool {
        self.word == other.word
            && self.source == other.source
            && self.match_type == other.match_type
    }
}

impl<'a> Eq for Candidate<'a> {}

/// Bayesian sort key: `log_prior + log_likelihood`. The single source
/// of truth for ranking. Higher = better.
///
/// Producers that wish to expose `score(c)` over the FFI boundary as a
/// `f64` may convert via `(score(c) as f64) / (Q4 as f64)` to recover
/// nats / bits in natural log space.
#[inline]
pub fn score(c: &Candidate<'_>) -> i32 {
    c.log_prior + c.log_likelihood
}

// ─── Derivation helpers (std-only) ─────────────────────────────────────
//
// The schema above is `no_std` clean — consumers in `no_std`
// environments may fill `log_prior` / `log_likelihood` from their own
// fixed-point log tables or precomputed values. The helpers below offer
// a default Q4 conversion for the common cases, gated behind `std` so
// they can use `f64::ln`.

/// Q4 log of natural priors derived from a corpus frequency. Returns
/// `Q4 · ln(1 + freq)` rounded to the nearest integer. Zero-freq
/// candidates map to `0`; freq-1 to ~`11`; freq-1000 to ~`110`.
///
/// This is the canonical baseline for the `log_prior` term. Producers
/// add user-level boosts (L0 pin, recency) on top by passing a larger
/// `freq` (e.g. `freq * 1000` for a pinned word) or by adding directly
/// to the returned value.
#[cfg(feature = "std")]
#[inline]
pub fn log_prior_from_freq(freq: u64) -> i32 {
    let ln = ((freq as f64) + 1.0).ln();
    (ln * (Q4 as f64)).round() as i32
}

/// Per-source linear shift applied to `log_prior_from_freq` to bring
/// pinyin / wubi / japanese log-priors into a common comparable
/// space. Today these are all zero — `log_prior_from_freq_with_source`
/// is byte-identical to `log_prior_from_freq` regardless of `source`
/// — so the framework is in place without changing any ranking. v1.7.3
/// / v1.8 will fit non-zero shifts from corpus log-frequency
/// distributions (per [[PLAN-v1.7]] D20 / D21 — linear shift + scale,
/// no ML, defaults preserved until explicit cutover).
///
/// **Why the framework lands now (v1.7.2) before the values change**:
/// existing callers (`idf-from-pinyin-dict` / `idf-from-wubi-tables` /
/// `idf-from-nihongo-jukugo` / `idf-from-nihongo-kanji`) bake the
/// `log_prior` into `.idf` snapshots at build time. To enable
/// source-aware fitting we need every caller to be able to receive
/// the source enum without having to widen its API again later. Ship
/// the API now, fit later.
///
/// Q4 fixed point: a shift of `+16` ≈ ×2.7 linear (`e^1`), `-16` ≈ ÷2.7.
#[cfg(feature = "std")]
const LOG_PRIOR_SHIFT_Q4: [i32; 3] = [
    0, // Source::Wubi (= 0)
    0, // Source::Pinyin (= 1)
    0, // Source::Japanese (= 2)
];

/// Source-aware Q4 log prior. Identical to `log_prior_from_freq(freq)`
/// when the per-source shift is 0 (today's default). Future fits
/// (v1.7.3+) tune the shifts so cross-source candidate comparison in
/// the composite engine becomes apples-to-apples.
///
/// Producers should migrate to this entry point; the source-less
/// `log_prior_from_freq` stays available indefinitely as the
/// no_std-friendly primitive.
#[cfg(feature = "std")]
#[inline]
pub fn log_prior_from_freq_with_source(freq: u64, source: Source) -> i32 {
    log_prior_from_freq(freq) + LOG_PRIOR_SHIFT_Q4[source as usize]
}

/// Q4 log-likelihood derived from a match-type classification.
///
/// `base_log_q4` is the producer-chosen per-engine / per-kind likelihood
/// floor (already in Q4 log-space) — e.g. `Q4·ln(LIKELIHOOD_JP_JUKUGO_BASE)`.
/// The match-type decay is applied on top:
///
/// - `Exact`              → `base_log_q4`
/// - `Prefix(prox_milli)` → `base_log_q4 + K · Q4·ln(prox/1000)` with `K=3`
/// - `Fuzzy(cost_milli)`  → `base_log_q4 + Q4·ln(1 − cost/1000)` (cost-capped at 999 to avoid `-inf`)
/// - `Composed { links }` → `base_log_q4 + (links − 1) · Q4·ln(0.7)` for `links ≥ 1`, else `base_log_q4`
///
/// All decays are non-positive (multiplicative factors ≤ 1.0 in linear
/// space), so the returned value is always `≤ base_log_q4`. This is the
/// log-space equivalent of the v1.3 multiplicative chain
/// `base · proximity^K · (1 − cost) · 0.7^(links − 1)`.
#[cfg(feature = "std")]
pub fn derive_log_likelihood(base_log_q4: i32, mt: MatchType) -> i32 {
    /// `LIKELIHOOD_PREDICT_PROXIMITY_K` from the legacy scoring module —
    /// pinyin / wubi / JP prefix-prediction all share `K = 3`.
    const K: f64 = 3.0;
    /// Per-extra-bigram-link composition decay (mirrors the linear-space
    /// `0.7` factor pinyin_adapter applies per additional Viterbi join).
    const LN_COMPOSED_PER_LINK: f64 = -0.356_674_943_938_732_4; // f64::ln(0.7)
    let q4 = Q4 as f64;
    match mt {
        MatchType::Exact => base_log_q4,
        MatchType::Prefix(prox_milli) => {
            // Producers that emit `Prefix(1000)` mean "fully typed";
            // the decay is 0 (ln 1 = 0), recovering the Exact case.
            let prox = (prox_milli.max(1) as f64) / 1000.0;
            let decay_q4 = (K * prox.ln() * q4).round() as i32;
            base_log_q4 + decay_q4
        }
        MatchType::Fuzzy(cost_milli) => {
            // Cap at 999 so the implied `1 − cost/1000` stays > 0 and
            // `ln` is finite; a cost of 1000 would mean "no match",
            // which producers should classify as a drop, not a fuzzy
            // hit.
            let cost = cost_milli.min(999) as f64 / 1000.0;
            let decay_q4 = ((1.0 - cost).ln() * q4).round() as i32;
            base_log_q4 + decay_q4
        }
        MatchType::Composed { bigram_links } => {
            let extra_links = bigram_links.saturating_sub(1) as f64;
            let decay_q4 = (extra_links * LN_COMPOSED_PER_LINK * q4).round() as i32;
            base_log_q4 + decay_q4
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn q4_anchored_to_16() {
        assert_eq!(Q4, 16);
    }

    #[test]
    fn source_repr_u8_stable() {
        assert_eq!(Source::Wubi as u8, 0);
        assert_eq!(Source::Pinyin as u8, 1);
        assert_eq!(Source::Japanese as u8, 2);
    }

    #[test]
    fn bayes_additive_in_log_space() {
        let cases: &[(i32, i32)] = &[
            (0, 0),
            (10 * Q4, 5 * Q4),
            (100 * Q4, -30 * Q4),
            (-50 * Q4, 100 * Q4),
            (i32::MAX / 4, i32::MIN / 4),
        ];
        for &(lp, ll) in cases {
            let c = Candidate {
                word: "x",
                log_prior: lp,
                log_likelihood: ll,
                match_type: MatchType::Exact,
                source: Source::Pinyin,
            };
            assert_eq!(score(&c), lp.saturating_add(ll));
        }
    }

    #[test]
    fn match_type_round_trip_variants() {
        let _exact = MatchType::Exact;
        let prefix = MatchType::Prefix(875);
        let fuzzy = MatchType::Fuzzy(300);
        let composed = MatchType::Composed { bigram_links: 2 };
        assert_eq!(prefix, MatchType::Prefix(875));
        assert_eq!(fuzzy, MatchType::Fuzzy(300));
        assert_eq!(composed, MatchType::Composed { bigram_links: 2 });
        assert_ne!(prefix, MatchType::Prefix(874));
        assert_ne!(composed, MatchType::Composed { bigram_links: 1 });
    }

    #[test]
    fn candidate_equality_ignores_score() {
        let a = Candidate {
            word: "继续",
            log_prior: 100 * Q4,
            log_likelihood: 50 * Q4,
            match_type: MatchType::Exact,
            source: Source::Pinyin,
        };
        let b = Candidate {
            word: "继续",
            log_prior: 999 * Q4,
            log_likelihood: -10 * Q4,
            match_type: MatchType::Exact,
            source: Source::Pinyin,
        };
        let c_word_diff = Candidate { word: "继续。", ..a };
        let c_source_diff = Candidate { source: Source::Wubi, ..a };
        let c_mt_diff = Candidate { match_type: MatchType::Prefix(500), ..a };
        assert_eq!(a, b, "score should not affect identity");
        assert_ne!(a, c_word_diff);
        assert_ne!(a, c_source_diff);
        assert_ne!(a, c_mt_diff);
    }

    #[cfg(feature = "std")]
    #[test]
    fn log_prior_from_freq_baseline_values() {
        assert_eq!(log_prior_from_freq(0), 0);
        let f1 = log_prior_from_freq(1);
        assert!((10..=12).contains(&f1), "ln(2)·16 ≈ 11; got {f1}");
        let f1000 = log_prior_from_freq(1000);
        assert!((110..=112).contains(&f1000), "ln(1001)·16 ≈ 110; got {f1000}");
        let f50000 = log_prior_from_freq(50_000);
        assert!(f50000 > log_prior_from_freq(1000), "monotone in freq");
    }

    /// v1.7.2 framework invariant: while `LOG_PRIOR_SHIFT_Q4` is all
    /// zeros, `log_prior_from_freq_with_source` must be byte-identical
    /// to `log_prior_from_freq` for every source. Any future change
    /// that flips a shift nonzero MUST flip this test together so the
    /// "default is identity" guarantee is auditable.
    #[cfg(feature = "std")]
    #[test]
    fn log_prior_from_freq_with_source_is_identity_at_zero_shift() {
        for source in [Source::Wubi, Source::Pinyin, Source::Japanese] {
            assert_eq!(LOG_PRIOR_SHIFT_Q4[source as usize], 0,
                "if you change LOG_PRIOR_SHIFT_Q4 update this test in the same commit");
            for freq in [0u64, 1, 100, 1_000, 50_000, 1_000_000] {
                assert_eq!(
                    log_prior_from_freq_with_source(freq, source),
                    log_prior_from_freq(freq),
                    "source={source:?} freq={freq}",
                );
            }
        }
    }

    #[cfg(feature = "std")]
    #[test]
    fn derive_log_likelihood_exact_is_base() {
        for base in [0, Q4, -10 * Q4, 100 * Q4] {
            assert_eq!(derive_log_likelihood(base, MatchType::Exact), base);
        }
    }

    #[cfg(feature = "std")]
    #[test]
    fn derive_log_likelihood_prefix_monotonic() {
        let base = 100 * Q4;
        let high = derive_log_likelihood(base, MatchType::Prefix(875));
        let mid = derive_log_likelihood(base, MatchType::Prefix(500));
        let low = derive_log_likelihood(base, MatchType::Prefix(100));
        let full = derive_log_likelihood(base, MatchType::Prefix(1000));
        assert_eq!(full, base, "prox=1000 collapses to base (ln 1 = 0)");
        assert!(high < base && mid < high && low < mid,
            "prefix decay must be monotone-decreasing in proximity; got full={full} high={high} mid={mid} low={low}");
    }

    #[cfg(feature = "std")]
    #[test]
    fn derive_log_likelihood_fuzzy_monotonic_in_cost() {
        let base = 100 * Q4;
        let cheap = derive_log_likelihood(base, MatchType::Fuzzy(100));
        let expensive = derive_log_likelihood(base, MatchType::Fuzzy(700));
        let zero = derive_log_likelihood(base, MatchType::Fuzzy(0));
        assert_eq!(zero, base, "cost=0 collapses to base (ln 1 = 0)");
        assert!(expensive < cheap && cheap < base,
            "fuzzy decay must be monotone-decreasing in cost; got zero={zero} cheap={cheap} expensive={expensive}");
    }

    #[cfg(feature = "std")]
    #[test]
    fn derive_log_likelihood_composed_per_link_decay() {
        let base = 200 * Q4;
        let one = derive_log_likelihood(base, MatchType::Composed { bigram_links: 1 });
        let two = derive_log_likelihood(base, MatchType::Composed { bigram_links: 2 });
        let three = derive_log_likelihood(base, MatchType::Composed { bigram_links: 3 });
        let zero = derive_log_likelihood(base, MatchType::Composed { bigram_links: 0 });
        assert_eq!(zero, base, "0 links — no chain — no decay");
        assert_eq!(one, base, "1 link is the first segment; (links − 1) = 0, no decay");
        assert!(two < one && three < two,
            "composed decay must drop per extra link; got one={one} two={two} three={three}");
    }

    #[cfg(feature = "std")]
    #[test]
    fn derive_log_likelihood_bayes_additivity_round_trip() {
        // The whole point: caller can compute (log_prior, log_likelihood)
        // independently and sum to a stable rank. Spot-check that
        // `score()` of a candidate built from `log_prior_from_freq` +
        // `derive_log_likelihood` recovers a sensible sort order across
        // a synthetic 4-tuple covering Exact / Prefix / Fuzzy / Composed.
        let base = 160 * Q4;
        let make = |freq: u64, mt: MatchType| Candidate {
            word: "x",
            log_prior: log_prior_from_freq(freq),
            log_likelihood: derive_log_likelihood(base, mt),
            match_type: mt,
            source: Source::Pinyin,
        };
        let exact_common = make(50_000, MatchType::Exact);
        let exact_rare = make(10, MatchType::Exact);
        let prefix_common = make(50_000, MatchType::Prefix(875));
        let fuzzy_common = make(50_000, MatchType::Fuzzy(300));
        // The common exact wins; common prefix beats rare exact (because
        // prior contribution dominates); common fuzzy sits between common
        // prefix and rare exact (rough chain, ranking-only check).
        assert!(score(&exact_common) > score(&prefix_common));
        assert!(score(&prefix_common) > score(&exact_rare),
            "common prefix prior wins rare exact; got pref={} rare={}",
            score(&prefix_common), score(&exact_rare));
        assert!(score(&exact_common) > score(&fuzzy_common));
    }

    #[test]
    fn score_orders_candidates_by_log_sum_desc() {
        let mk = |lp: i32, ll: i32| Candidate {
            word: "w",
            log_prior: lp,
            log_likelihood: ll,
            match_type: MatchType::Exact,
            source: Source::Pinyin,
        };
        let mut cands = [mk(10, 20), mk(50, -10), mk(0, 0), mk(100, 100)];
        cands.sort_by(|a, b| score(b).cmp(&score(a)));
        assert_eq!(score(&cands[0]), 200);
        assert_eq!(score(&cands[1]), 40);
        assert_eq!(score(&cands[2]), 30);
        assert_eq!(score(&cands[3]), 0);
    }
}
