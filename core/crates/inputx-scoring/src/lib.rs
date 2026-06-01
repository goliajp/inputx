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
    /// Typed input used as shorthand initials (e.g. `zg` → `中国` where
    /// the user typed initial consonants only). Distinguished from
    /// `Prefix` because initials shorthand skips entire syllable
    /// nuclei, not just trailing characters — the proximity decay is
    /// gentler (`K = 1`) than `Prefix`'s `K = 3`.
    ///
    /// Payload: `typed_len` (number of initial-letter characters the
    /// user typed) + `full_len` (estimated full pinyin length of the
    /// candidate's reading, typically `word.chars().count() * 4` since
    /// average pinyin syllable ≈ 4 ASCII letters). Both clipped to u8;
    /// `typed_len ≥ full_len` collapses to no-decay (treated as a
    /// fully-typed initials shorthand).
    Initials { typed_len: u8, full_len: u8 },
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

    /// Single-character candidate boost — applied when
    /// [`CandidateData::word_char_count`] `== 1`.
    ///
    /// Today's PRIOR architecture (corpus log-prob + per-engine boost)
    /// treats single-char vs multi-char candidates identically. A
    /// single common char (e.g. 我 raw_freq ~ M) and a multi-char
    /// phrase (e.g. 我们 raw_freq ~ M) compete in the same log-prior
    /// space; under additive `prior + likelihood`, multi-char tends
    /// to win on rare ties thanks to longer ngram boost. WU-τ surfaces
    /// the char-vs-word weighting as an explicit knob so polish-log
    /// telemetry can calibrate the gap rather than relying on
    /// implicit bigram-bonus side effects.
    ///
    /// Default `0`: behavior identical to pre-v1.7.5 (knob lives in
    /// the struct, doesn't move ranking until calibrated).
    pub char_boost_q4: i32,

    /// Per-extra-character bonus for multi-character candidates —
    /// applied as `(word_char_count − 1) · word_len_bonus_q4` when
    /// `word_char_count > 1`. A 2-char word gets `× 1`, a 3-char word
    /// `× 2`, etc.
    ///
    /// Captures the v1.5.3 implicit "longer phrases lead on ties"
    /// effect (bigram bonus accumulating per link) as an explicit
    /// length-graded weight. The product spec (PLAN.md §v1.7.3 WU-τ)
    /// expresses this as `log_prior_word = log_freq + W_WORD ·
    /// len_bonus(len)` — the implementation collapses `len_bonus(len)`
    /// to `(len − 1)` for simplicity, leaving room for a richer
    /// piecewise schedule once telemetry argues for one.
    ///
    /// Default `0`: ranking unchanged.
    pub word_len_bonus_q4: i32,

    /// Per-engine LIKELIHOOD floor for fuzzy / phonetic-edit candidates,
    /// in Q4 log-space. Consumers (e.g. `composite/pinyin_adapter.rs`'s
    /// fuzzy branch) pass this as `base_log_q4` to
    /// [`derive_log_likelihood`] alongside `MatchType::Fuzzy(cost_milli)`
    /// derived from the real phonetic edit distance — closer typos pay
    /// less decay, distant ones pay more.
    ///
    /// Pre-v1.8 the fuzzy path used a flat `Q4·ln(FUZZY_BASE ·
    /// FUZZY_DISCOUNT) = Q4·ln(105_000) ≈ 185` regardless of typo
    /// magnitude. The v1.8 default of `Q4·ln(FUZZY_BASE) ≈ 205` plus
    /// `Fuzzy(cost_milli)` decay reproduces that 185 baseline at
    /// `cost_milli ≈ 700` (`ln(1 − 0.7)·Q4 ≈ -19`, `205 − 19 = 186` ≈
    /// pre-v1.8 185). Real edit_distance ∈ `[0.2, 1.0]` from
    /// `inputx-phonetic-edit::edit_distance` mapped to milli (× 1000,
    /// clamp 999) lets nearby typos clear the bar and distant ones
    /// fall further.
    ///
    /// Default `205` (calibrated v1.8.0; pre-v1.8 ranking preserved
    /// when paired with cost_milli ≈ 700, which is what Path 1a +
    /// Path 1b emit by construction).
    pub fuzzy_likelihood_floor_q4: i32,

    /// Per-engine LIKELIHOOD floor for initials-shorthand candidates,
    /// in Q4 log-space. Consumers (e.g. `composite/pinyin_adapter.rs`'s
    /// initials Path 1c) pass this as `base_log_q4` to
    /// [`derive_log_likelihood`] alongside `MatchType::Initials {
    /// typed_len, full_len }` derived from the typed buffer's initial
    /// consonant cluster vs. the candidate's estimated full pinyin
    /// length.
    ///
    /// Pre-v1.8.0 initials candidates went through the
    /// NON_EXACT_FLOOR tier (~`Q4·ln(1000) ≈ 110`); v1.8.0 inadvertent-
    /// ly routed them through the fuzzy tier at the canonical-fuzzy
    /// log_likelihood (~199 ≈ `Q4·ln(0.3·350k)`). v1.8.1 surfaces
    /// initials as its own tier with explicit proximity decay
    /// (`K = 1` per `derive_log_likelihood`'s `MatchType::Initials`
    /// branch), defaulting to `221 ≈ Q4·ln(450k)` so the typical
    /// case `typed_len = 2, full_len = 8` (2-letter abbrev of a
    /// 2-char compound) lands at `221 + ln(0.25)·16 ≈ 199`, matching
    /// the v1.8.0 ranking for initials candidates.
    pub initials_likelihood_base_q4: i32,
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
            char_boost_q4: 0,
            word_len_bonus_q4: 0,
            fuzzy_likelihood_floor_q4: 0,
            initials_likelihood_base_q4: 0,
        }
    }

    /// Production-default weight set for Inputx. Calibrated v1.7.4
    /// against the 24-test baseline + the comprehensive Jianma1/2/3
    /// regression matrix, after the global migration of `log_prior_q4`
    /// from `Q4·ln(1+freq)` (unnormalized) to real log-probability
    /// `Q4·ln((1+freq)/(1+corpus_total))` per engine.
    ///
    /// Why nonzero values: each engine's `log_prob_corpus_q4` is now
    /// shifted by `-Q4·ln(1+T_engine)` relative to the legacy
    /// unnormalized score (pinyin T ≈ 2.6e9 → shift ≈ -343, wubi T ≈
    /// 1.23e9 → shift ≈ -333, jukugo T ≈ 1.1e6 → shift ≈ -224, kanji T
    /// ≈ 78k → shift ≈ -180). To keep within-engine ranking unchanged
    /// AND restore the legacy cross-engine relationships (notably the
    /// Inputx-五笔 product promise "wubi-first in mixed mode"), the
    /// per-source boost bundles both effects:
    ///
    ///   engine_boost_q4[Wubi]     = legacy +15 plus enough to offset
    ///                               wubi simcodes' very-low raw_freq
    ///                               (their corpus log-prob is highly
    ///                               negative; the layer.base term in
    ///                               log_likelihood carries the lift,
    ///                               but the boost re-asserts wubi-
    ///                               first across the cross-engine
    ///                               compare).
    ///   engine_boost_q4[Pinyin]   = 0 (anchor)
    ///   engine_boost_q4[Japanese] = small downshift to match the JP
    ///                               corpus's much smaller total (JP
    ///                               log_prob_corpus is less negative;
    ///                               without the offset, JP would float
    ///                               above pinyin in mixed mode).
    ///
    /// Hand-tuned. Calibration knob lives here so future iterations
    /// can shift it without touching `compute_score` or the data
    /// primitives.
    pub const fn inputx_default() -> Self {
        Self {
            // Pinyin anchored at 0; wubi/jp expressed relative to it.
            engine_boost_q4: [
                8,   // Wubi: scaled-down legacy "Inputx wubi-first"
                     // prior. Pre-v1.7.4 the boost was +15 Q4 (×2.6
                     // linear) — under the log_prob_corpus shift the
                     // relative wubi-vs-pinyin gap widened (wubi's
                     // smaller corpus_total gives wubi a +10 Q4 lift
                     // "for free"), so +8 Q4 lands the cross-engine
                     // ranking back where it was without overshooting
                     // the rare-Jianma2 → pinyin-top yields. Applied
                     // uniformly to ALL wubi candidates.
                0,   // Pinyin anchor.
                -100, // Japanese: shift down so JP candidates (smaller
                      // corpus total, less-negative log_prob_corpus)
                      // sit in the same band the legacy unnormalized
                      // path placed them.
            ],
            // Wubi simcode lift (Jianma1/2/3 only). Set to 0: under
            // the v1.7.4 log_prob_corpus shift, the wubi `engine_boost_q4`
            // + simcode `layer.base` (in log_likelihood_q4) together
            // already place common simcodes above pinyin top while the
            // `RARE_CHAR_DEMOTE` (× 0.3 on log_likelihood) drops
            // rare-CJK Jianma2 entries below pinyin top. Reserved for
            // future calibration if telemetry shows the gap is too
            // tight.
            simcode_boost_q4: 0,
            bootstrap_floor_q4: 0,
            // WU-τ knobs (v1.7.5) — both 0 keeps post-v1.7.4 ranking
            // intact. Future polish-log calibration shifts these.
            char_boost_q4: 0,
            word_len_bonus_q4: 0,
            // WU-ν fuzzy floor (v1.8.0). 205 ≈ Q4·ln(FUZZY_BASE=350k).
            // Paired with `Fuzzy(cost_milli ≈ 700)` (the cost the v1.8
            // fuzzy path emits for a canonical southern-dialect swap
            // like zh↔z, edit_distance ≈ 0.3 → cost_milli 300 — but
            // see `pinyin_adapter`'s mapping: linear `distance·1000` is
            // too generous for the legacy comparable, so the actual
            // mapping is `(1 − exp(-1.2·distance))·1000` which hits
            // ~700 at distance 0.3, reproducing the pre-v1.8 flat 185
            // log_likelihood for the most common fuzzy case).
            fuzzy_likelihood_floor_q4: 205,
            // WU-ξ initials base (v1.8.1). 221 ≈ Q4·ln(450k). Paired
            // with `MatchType::Initials { typed_len=2, full_len=8 }`
            // (the typical 2-letter abbreviation of a 2-char
            // compound) yields `221 + ln(0.25)·16 ≈ 199`, reproducing
            // the v1.8.0-inadvertent ranking of initials at the
            // canonical-fuzzy tier. Longer words (4-char phrases via
            // 4 initials) get more decay (proximity → 0.125, decay
            // → -33 Q4, log_lik → 188) — correct: more letters typed
            // = more confident the user meant initials, BUT the word
            // is longer so the abbreviation is less unique → net
            // decay is right.
            initials_likelihood_base_q4: 221,
        }
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
    /// Length of the candidate word in characters (UTF-8 code points,
    /// matching `word.chars().count()`). Saturates to `u8::MAX` for
    /// pathological inputs; production words are ≤ ~10 chars.
    ///
    /// Drives [`EngineWeights::char_boost_q4`] (when `== 1`) and
    /// [`EngineWeights::word_len_bonus_q4`] (when `> 1`, scaled by
    /// `count − 1`). Fill sites compute this once per candidate and
    /// pass it through; the compose layer doesn't re-walk the word
    /// string.
    pub word_char_count: u8,
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

    // WU-τ (v1.7.5): char vs word length weight. Single-character
    // candidates get a flat `char_boost_q4`; multi-character ones get
    // `(count − 1) · word_len_bonus_q4`. Both default to 0 so the
    // knobs are inert until calibrated.
    let length_weight = if data.word_char_count <= 1 {
        weights.char_boost_q4
    } else {
        (data.word_char_count as i32 - 1).saturating_mul(weights.word_len_bonus_q4)
    };

    log_prob
        + data.log_likelihood_q4
        + weights.engine_boost_q4[data.source as usize]
        + if data.is_simcode { weights.simcode_boost_q4 } else { 0 }
        + length_weight
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
        MatchType::Initials { typed_len, full_len } => {
            // Initials shorthand: user typed N initial-letter
            // characters as an abbreviation for a multi-syllable word
            // with full pinyin length M ≈ chars · 4. Proximity =
            // `typed_len / full_len` (clipped to (0, 1]). Decay K=1
            // (linear in log-space, gentler than Prefix's K=3) — the
            // typed/full ratio for 2-letter initials of a 2-char word
            // is already small (≈ 0.25), the K=3 prefix decay
            // (`ln(0.25^3) ≈ -66 Q4`) would crush initials below
            // anything useful; K=1 (`ln(0.25) ≈ -22 Q4`) keeps them
            // ranked as plausible-but-low-confidence alternates.
            //
            // `typed_len ≥ full_len` (degenerate: user typed more
            // than the full reading) collapses to proximity=1, no
            // decay.
            if typed_len == 0 || full_len == 0 {
                return base_log_q4;
            }
            let prox = (typed_len as f64 / full_len as f64).min(1.0);
            let decay_q4 = (prox.ln() * q4).round() as i32;
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
                    word_char_count: 1,
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
            word_char_count: 1,
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
            word_char_count: 1,
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
            word_char_count: 1,
        };
        // floor (-50) replaces log_prob_corpus_q4 (-300), so score is -50, not -300.
        assert_eq!(compute_score(&data, &weights), -50);
    }

    /// WU-τ (v1.7.5): `char_boost_q4` fires only for single-character
    /// candidates; multi-character ones get `(count − 1) ·
    /// word_len_bonus_q4` instead. Both default to 0, keeping the v1.7.4
    /// baseline invariant intact.
    #[cfg(feature = "std")]
    #[test]
    fn compute_score_char_boost_fires_only_for_single_char() {
        let mut weights = EngineWeights::neutral();
        weights.char_boost_q4 = 40;
        let single = CandidateData {
            log_prob_corpus_q4: -50,
            log_likelihood_q4: 0,
            source: Source::Pinyin,
            is_bootstrap: false,
            is_simcode: false,
            word_char_count: 1,
        };
        let multi = CandidateData { word_char_count: 3, ..single };
        // single-char gets +40; multi-char does NOT get char_boost.
        assert_eq!(compute_score(&single, &weights), -50 + 40);
        assert_eq!(compute_score(&multi, &weights), -50, "multi-char must not see char_boost");
    }

    /// WU-τ (v1.7.5): `word_len_bonus_q4` scales linearly with extra
    /// characters; `count == 1` candidates ignore it.
    #[cfg(feature = "std")]
    #[test]
    fn compute_score_word_len_bonus_scales_with_count() {
        let mut weights = EngineWeights::neutral();
        weights.word_len_bonus_q4 = 10;
        let mk = |count: u8| CandidateData {
            log_prob_corpus_q4: -50,
            log_likelihood_q4: 0,
            source: Source::Pinyin,
            is_bootstrap: false,
            is_simcode: false,
            word_char_count: count,
        };
        // count=1 → no bonus (single-char path falls through to char_boost which is 0).
        assert_eq!(compute_score(&mk(1), &weights), -50);
        // count=2 → +(2-1)·10 = +10
        assert_eq!(compute_score(&mk(2), &weights), -50 + 10);
        // count=4 → +(4-1)·10 = +30
        assert_eq!(compute_score(&mk(4), &weights), -50 + 30);
    }

    /// WU-ξ (v1.8.1): `MatchType::Initials` decay is gentler than
    /// `MatchType::Prefix` (K=1 vs K=3) at the same `typed/full`
    /// proximity, because initials shorthand skips entire syllable
    /// nuclei (each abbreviation step is a much larger "leap" than a
    /// prefix-completion truncation).
    #[cfg(feature = "std")]
    #[test]
    fn derive_log_likelihood_initials_gentler_than_prefix() {
        let base = 221;
        // Same nominal proximity (2/8 = 25%) — Initials should decay
        // less than Prefix (250 prox_milli) because K=1 not 3.
        let init = derive_log_likelihood(base, MatchType::Initials { typed_len: 2, full_len: 8 });
        let pref = derive_log_likelihood(base, MatchType::Prefix(250));
        assert!(init > pref,
            "Initials decay (K=1) must be gentler than Prefix decay (K=3) at same proximity; got init={init} pref={pref}");
        // Typical pinyin initials case lands at ~199 (the calibration
        // target for inputx_default()).
        assert!((196..=202).contains(&init),
            "typical 2/8 initials decay should land near 199; got {init}");
    }

    /// WU-ξ: degenerate inputs (typed_len ≥ full_len, or zero) collapse
    /// to the exact case (no decay) rather than panicking or going
    /// negative.
    #[cfg(feature = "std")]
    #[test]
    fn derive_log_likelihood_initials_degenerate_no_decay() {
        let base = 100;
        // typed_len > full_len → proximity clamped to 1.0 → no decay.
        assert_eq!(
            derive_log_likelihood(base, MatchType::Initials { typed_len: 8, full_len: 4 }),
            base,
            "typed >= full collapses to no decay"
        );
        // zero typed_len/full_len → no decay (defensive).
        assert_eq!(
            derive_log_likelihood(base, MatchType::Initials { typed_len: 0, full_len: 4 }),
            base,
        );
        assert_eq!(
            derive_log_likelihood(base, MatchType::Initials { typed_len: 4, full_len: 0 }),
            base,
        );
    }

    /// Neutral weights leave word_char_count irrelevant — the v1.7.5
    /// identity property (compute_score == log_prob + log_likelihood)
    /// holds across char counts.
    #[cfg(feature = "std")]
    #[test]
    fn compute_score_neutral_is_count_invariant() {
        let weights = EngineWeights::neutral();
        for count in [1u8, 2, 5, 10, 50, u8::MAX] {
            let data = CandidateData {
                log_prob_corpus_q4: -100,
                log_likelihood_q4: 50,
                source: Source::Pinyin,
                is_bootstrap: false,
                is_simcode: false,
                word_char_count: count,
            };
            assert_eq!(compute_score(&data, &weights), -50,
                "neutral weights must give -100+50=-50 regardless of word_char_count (got count={count})");
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
