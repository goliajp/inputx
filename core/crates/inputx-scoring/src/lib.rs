//! `inputx-scoring` — probability-native candidate scoring primitive.
//!
//! # The three-layer scoring model (v1.7.2)
//!
//! The ranking pipeline is intentionally factored into three layers,
//! each with a separate type, separate ownership, and a separate
//! lifecycle:
//!
//! ```text
//!     ┌───────────────────┐     ┌──────────────────┐
//!     │ static data       │     │ runtime config   │
//!     │ (per candidate,   │     │ (per session /   │
//!     │  baked into .idf  │     │  per user prefs) │
//!     │  at build time)   │     │                  │
//!     │                   │     │ EngineWeights {  │
//!     │ CandidateData {   │     │  engine_boost_q4 │
//!     │  log_prob_corpus  │     │  simcode_boost   │
//!     │  source / layer   │     │  bootstrap_floor │
//!     │  is_bootstrap     │     │  ...             │
//!     │  is_simcode       │     │ }                │
//!     │  match_type       │     │                  │
//!     │  log_likelihood   │     │                  │
//!     │ }                 │     │                  │
//!     └─────────┬─────────┘     └────────┬─────────┘
//!               │                        │
//!               └──────── compute_score(data, weights) ──→ i32
//!                                                          ▲
//!                                  this is the only value  │
//!                                  comparison ever looks at┘
//! ```
//!
//! **Why this matters**: candidates carry **data assets** (intrinsic,
//! durable, stored on disk). Sessions carry **weights** (dynamic,
//! configurable, user-tunable). Ranking compares **computed scores**
//! — the output of folding the two together — not raw data and not
//! raw weights. Mixing the three (e.g. baking engine preferences into
//! the .idf log_prior) loses configurability; ignoring weights at
//! compare time loses the user-tuning surface; comparing data directly
//! across engines without normalization (the legacy `log_prior_from_freq`
//! path) loses cross-engine fairness.
//!
//! # Static data primitives
//!
//! - [`log_prob_corpus_from_freq`] — `Q4·ln((1+freq)/(1+total))`,
//!   a real log probability in [-∞, 0]. This is the **data primitive**:
//!   producers bake it into `.idf` at build time so consumers don't
//!   need the corpus total at runtime.
//!
//! # Dynamic weights
//!
//! - [`EngineWeights`] — per-session knobs. `neutral()` = no-op
//!   (compute_score reproduces the legacy additive `score()`).
//!   `inputx_default()` = production defaults (五笔 simcode boost,
//!   bootstrap floor for 字根 entries, etc.).
//!
//! # Compute
//!
//! - [`compute_score`] — folds [`CandidateData`] + [`EngineWeights`]
//!   into the i32 sort key. Pure function, no I/O.
//! - [`score`] — legacy shortcut: `c.log_prior + c.log_likelihood`,
//!   equivalent to `compute_score(c.into(), &EngineWeights::neutral())`
//!   minus the source/layer pathways.
//!
//! # Legacy data primitive (kept transitional)
//!
//! - [`log_prior_from_freq`] — `Q4·ln(1 + freq)`. **Not** a log
//!   probability (missing the `- ln(total)` term). The name is
//!   honored for transitional compatibility; new callers should use
//!   [`log_prob_corpus_from_freq`] with the engine's known total.
//!
//! Both terms are `i32` Q4 fixed-point. One log unit = [`Q4`] (= 16)
//! integer steps. The Q4 choice trades resolution for headroom: scores
//! fit comfortably in `i32` even for very rare or very common words
//! while staying precise enough that ranking-relevant gaps (~0.0625 in
//! log space) survive quantization.
//!
//! Scoring policy lives in the consumer (IME engine cement); this
//! crate provides only the schema, the data primitives, the weight
//! struct, and the compose function. **No production-grade weight
//! values live here** — those go in the per-engine cement / facade
//! that knows its session context.

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

/// Q4 log-probability derived from a corpus-relative frequency.
/// Returns `Q4 · ln((1 + freq) / (1 + corpus_total))`, the **real
/// log probability** of seeing this word given the source corpus.
/// Values are in `[-∞, 0]` (Q4-scaled negative integers).
///
/// **The data primitive that should be baked into `.idf` snapshots**
/// (replacing the historical [`log_prior_from_freq`] which is missing
/// the `- ln(total)` term and is therefore an unnormalized log-score,
/// not a log-probability — see crate-level doc).
///
/// `corpus_total = 0` returns 0 (safety floor: degenerate "empty
/// corpus" case where the formula would divide by 0 in linear space).
/// Producers should always pass the engine's true corpus total when
/// available.
///
/// Comparable across engines: pinyin's `log_prob_corpus_from_freq(f,
/// 2_612_233_621)` is on the same numeric axis as wubi's
/// `log_prob_corpus_from_freq(f, wubi_total)`, which is exactly what
/// makes cross-engine ranking in [`compute_score`] sound.
#[cfg(feature = "std")]
#[inline]
pub fn log_prob_corpus_from_freq(freq: u64, corpus_total: u64) -> i32 {
    if corpus_total == 0 {
        return 0;
    }
    let ratio = ((freq as f64) + 1.0) / ((corpus_total as f64) + 1.0);
    (ratio.ln() * (Q4 as f64)).round() as i32
}

// ─── Dynamic weights (runtime config) ──────────────────────────────────

/// Per-session runtime knobs that the comparison layer applies on top
/// of per-candidate static data. Defaults are encoded in [`EngineWeights::neutral`]
/// (no-op, identity) and [`EngineWeights::inputx_default`] (production:
/// 五笔 simcode boost, bootstrap-entry floor, …).
///
/// **Mental model**: weights are **how the user / IME session
/// configures the ranker**, not what the dict knows. Changing weights
/// at runtime alters ranking without re-baking any `.idf` bytes. This
/// is the surface telemetry / per-user learning will eventually drive.
#[cfg(feature = "std")]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct EngineWeights {
    /// Per-engine additive log-space boost. Indexed by [`Source`] as
    /// `u8`: `[Wubi, Pinyin, Japanese]`. A `+Q4` entry ≈ ×2.7 linear.
    /// Use to express "in mixed mode, 五笔 candidates lead pinyin
    /// fallback" as an explicit numeric weight rather than a hidden
    /// ordering rule.
    pub engine_boost_q4: [i32; 3],

    /// Extra boost applied to wubi 简码 (Jianma1/2/3) candidates on
    /// top of `engine_boost_q4[Wubi]`. Captures the "五笔 simcode
    /// 常用字必须 lead" product promise as a numeric weight, not a
    /// runtime branch.
    pub simcode_boost_q4: i32,

    /// Replacement log-prob for bootstrap entries (e.g. 字根表
    /// entries whose source freq is 0 because they're prescriptive,
    /// not corpus-derived). Without this floor, `log_prob_corpus`
    /// would assign them the worst possible probability (≈ ln(1/total))
    /// even though they're hand-curated lead candidates.
    ///
    /// A typical value is the corpus log-prob of a median-rank entry
    /// (e.g. `-15 · Q4`), but the production value lives in the
    /// engine cement that knows its own corpus distribution.
    pub bootstrap_floor_q4: i32,
}

#[cfg(feature = "std")]
impl EngineWeights {
    /// Identity / no-op weights. `compute_score(data, &neutral())`
    /// is equivalent to `data.log_prob_corpus + data.log_likelihood`
    /// — exactly the legacy [`score`] sum, with no cross-engine
    /// preference and no simcode boost. Useful for baseline parity
    /// tests and for callers that opt out of weighted ranking.
    pub const fn neutral() -> Self {
        Self {
            engine_boost_q4: [0; 3],
            simcode_boost_q4: 0,
            bootstrap_floor_q4: 0,
        }
    }

    /// Production-default weight set for Inputx (placeholder values;
    /// real calibration happens in the per-engine cement once it
    /// owns its corpus_total + has user-polish-log telemetry to fit
    /// from). Today all zeros so behavior matches `neutral()`.
    ///
    /// Future calibration goes here (or, more likely, in a builder
    /// in each engine's cement crate that constructs this struct
    /// from a user-defaults dict + telemetry stats).
    pub const fn inputx_default() -> Self {
        Self::neutral()
    }
}

// ─── Compose (compute the comparison value) ────────────────────────────

/// The composed candidate value: static data fields the ranker actually
/// needs to see at compare time. Producers fill these from .idf reads
/// or runtime knowledge.
///
/// Intentionally separate from [`Candidate`] (the schema for legacy
/// callers) — `CandidateData` carries the **richer** fields that
/// `compute_score` needs (`is_simcode`, `is_bootstrap`), whereas
/// `Candidate` is the historical no_std-clean tuple.
#[cfg(feature = "std")]
#[derive(Copy, Clone, Debug)]
pub struct CandidateData {
    /// Q4 log P(W) relative to this engine's corpus, from
    /// [`log_prob_corpus_from_freq`]. Cross-engine comparable.
    pub log_prob_corpus_q4: i32,
    /// Q4 log P(i | W), the match likelihood term.
    pub log_likelihood_q4: i32,
    /// Which engine produced this candidate.
    pub source: Source,
    /// Whether this entry came from a prescriptive bootstrap source
    /// (字根表 / simcode table) rather than corpus frequency counts.
    pub is_bootstrap: bool,
    /// Whether this is a wubi 简码 (Jianma1/2/3) candidate. Producers
    /// for other engines pass `false`.
    pub is_simcode: bool,
}

/// Fold static data + dynamic weights into the i32 sort key.
///
/// Pure function. The returned value is what comparison sees and
/// nothing else. Identity property: with [`EngineWeights::neutral`]
/// and `is_bootstrap=false`, `compute_score(data, &neutral())` ==
/// `data.log_prob_corpus_q4 + data.log_likelihood_q4` (matches the
/// legacy [`score`] sum).
#[cfg(feature = "std")]
#[inline]
pub fn compute_score(data: &CandidateData, weights: &EngineWeights) -> i32 {
    // Bootstrap entries: corpus freq doesn't represent their real
    // prior. Override the data's log_prob_corpus with the
    // session-configured floor before composing.
    let log_prob = if data.is_bootstrap {
        weights.bootstrap_floor_q4
    } else {
        data.log_prob_corpus_q4
    };

    log_prob
        + data.log_likelihood_q4
        + weights.engine_boost_q4[data.source as usize]
        + if data.is_simcode { weights.simcode_boost_q4 } else { 0 }
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

    /// `log_prob_corpus_from_freq` is the data primitive — a real log
    /// probability, monotone increasing in `freq`, monotone decreasing
    /// in `corpus_total`. Returns ≤ 0 (a probability ≤ 1 in log space).
    /// `corpus_total = 0` floors at 0 (no division-by-zero crash).
    #[cfg(feature = "std")]
    #[test]
    fn log_prob_corpus_from_freq_real_log_probability() {
        // Empty corpus floor
        assert_eq!(log_prob_corpus_from_freq(0, 0), 0);
        assert_eq!(log_prob_corpus_from_freq(100, 0), 0);

        // freq=0 in a real corpus → ln(1/(1+total)), very negative
        let p_zero = log_prob_corpus_from_freq(0, 1_000_000);
        assert!(p_zero < -100, "ln(1/1e6) · Q4 ≈ -221; got {p_zero}");

        // Same freq, larger corpus → smaller (more negative) log-prob
        let a = log_prob_corpus_from_freq(100, 1_000_000);
        let b = log_prob_corpus_from_freq(100, 1_000_000_000);
        assert!(b < a, "{b} < {a}: same freq, larger total → smaller P");

        // Monotone in freq within same corpus
        let c = log_prob_corpus_from_freq(10, 1_000_000);
        let d = log_prob_corpus_from_freq(1000, 1_000_000);
        assert!(d > c, "freq up → log_prob up: {d} > {c}");

        // Probability ≤ 1 → log_prob ≤ 0 always (with non-empty total).
        for &(f, t) in &[(1u64, 100u64), (50, 100), (99, 100), (100, 100)] {
            let p = log_prob_corpus_from_freq(f, t);
            assert!(p <= 0, "log P(W) must be ≤ 0; got {p} for ({f}, {t})");
        }
    }

    /// `compute_score` identity property: with neutral weights and a
    /// non-bootstrap entry, the composed score equals the legacy
    /// `log_prior + log_likelihood` sum. Future weight values are
    /// expected to break this, but with `neutral()` it must hold —
    /// this is what lets us land the framework without disturbing
    /// any existing baseline.
    #[cfg(feature = "std")]
    #[test]
    fn compute_score_neutral_is_legacy_sum() {
        let weights = EngineWeights::neutral();
        for source in [Source::Wubi, Source::Pinyin, Source::Japanese] {
            for &(prior, lik) in &[(0, 0), (50, 50), (-200, 100), (10_000, -10)] {
                let data = CandidateData {
                    log_prob_corpus_q4: prior,
                    log_likelihood_q4: lik,
                    source,
                    is_bootstrap: false,
                    is_simcode: false,
                };
                assert_eq!(compute_score(&data, &weights), prior + lik,
                    "neutral weights must collapse to log_prior + log_likelihood");
            }
        }
    }

    /// `compute_score` actually USES the weights: nonzero engine_boost
    /// shifts the result. Sanity-checks that the wiring isn't dead code.
    #[cfg(feature = "std")]
    #[test]
    fn compute_score_engine_boost_applies() {
        let data = CandidateData {
            log_prob_corpus_q4: -100,
            log_likelihood_q4: 50,
            source: Source::Wubi,
            is_bootstrap: false,
            is_simcode: false,
        };
        let mut weights = EngineWeights::neutral();
        let baseline = compute_score(&data, &weights);
        weights.engine_boost_q4[Source::Wubi as usize] = 32; // +2 log = ×7.4 linear
        let boosted = compute_score(&data, &weights);
        assert_eq!(boosted - baseline, 32, "engine_boost_q4 must add additively");
    }

    /// `compute_score` simcode boost only fires for simcode entries.
    #[cfg(feature = "std")]
    #[test]
    fn compute_score_simcode_boost_only_for_simcode() {
        let mut weights = EngineWeights::neutral();
        weights.simcode_boost_q4 = 64;
        let common = CandidateData {
            log_prob_corpus_q4: -100,
            log_likelihood_q4: 0,
            source: Source::Wubi,
            is_bootstrap: false,
            is_simcode: false,
        };
        let simcode = CandidateData { is_simcode: true, ..common };
        assert_eq!(compute_score(&common, &weights), -100);
        assert_eq!(compute_score(&simcode, &weights), -100 + 64);
    }

    /// `compute_score` bootstrap-floor overrides `log_prob_corpus_q4`
    /// for bootstrap entries. Without it, 字根表 entries (freq=0)
    /// would land at the worst log-prob and lose to every corpus
    /// word — defeating their hand-curated priority.
    #[cfg(feature = "std")]
    #[test]
    fn compute_score_bootstrap_floor_overrides_log_prob() {
        let mut weights = EngineWeights::neutral();
        weights.bootstrap_floor_q4 = -50;
        let data = CandidateData {
            log_prob_corpus_q4: -300, // would lose to any corpus entry
            log_likelihood_q4: 0,
            source: Source::Wubi,
            is_bootstrap: true,
            is_simcode: false,
        };
        // floor (-50) replaces log_prob_corpus_q4 (-300), so score is -50, not -300.
        assert_eq!(compute_score(&data, &weights), -50);
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
