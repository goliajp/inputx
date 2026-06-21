//! Build-time codegen for inputx-scoring: parses
//! `data/engine_weights.toml` and emits
//! `$OUT_DIR/engine_weights_generated.rs` — a private module of const
//! literals consumed by `inputx_default()` and (post-WU-β) by the
//! composite layer's scoring formulas.
//!
//! This is the v1.10 "single source of truth for ranking formula"
//! mechanism described in `docs/POLISH-ARCHITECTURE.md`. Users polish
//! the TSV / TOML data assets; build.rs handles wiring; runtime stays
//! const-only.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let crate_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let toml_path = crate_dir.join("data/engine_weights.toml");
    let out_path = out_dir.join("engine_weights_generated.rs");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=data/engine_weights.toml");
    // WU-ψ phase 5: tier overlay TSV lives in tools/scoring/data so
    // it's grouped with the other polish-overlay assets. Path is
    // relative to crate dir.
    let tier_overlay_path = crate_dir.join("../../../tools/scoring/data/polish/tier_overlay.tsv");
    println!("cargo:rerun-if-changed={}", tier_overlay_path.display());

    let text = fs::read_to_string(&toml_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", toml_path.display()));
    let parsed: toml::Value =
        toml::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", toml_path.display()));

    let ew = parsed
        .get("engine_weights")
        .and_then(|v| v.as_table())
        .unwrap_or_else(|| panic!("engine_weights.toml: missing [engine_weights] table"));

    // engine_boost_q4 + simcode_boost_q4: retired in WU-ψ phase 6.
    // The tier × engine table subsumes both ("wubi-first via tier_base
    // engine offset", "simcode lift via tier 0 assignment"). TOML rows
    // for these fields can be deleted at the user's convenience.

    // WU-ψ tier × engine base table (v1.11).
    let teb = parsed
        .get("tier_engine_base")
        .and_then(|v| v.as_table())
        .unwrap_or_else(|| panic!("[tier_engine_base] section missing"));
    let tier_gap_q4 = read_i32(teb, "tier_gap_q4");
    let engine_gap_q4 = read_i32(teb, "engine_gap_q4");
    let within_tier_max_q4 = read_i32(teb, "within_tier_max_q4");
    let tier_count = read_i32(teb, "tier_count");
    if tier_count <= 0 || tier_count > 32 {
        panic!("tier_count must be in (0, 32]; got {tier_count}");
    }
    if within_tier_max_q4 >= tier_gap_q4 {
        panic!(
            "within_tier_max_q4 ({within_tier_max_q4}) must be < tier_gap_q4 ({tier_gap_q4}) — \
             otherwise within-tier variance crosses tier boundaries"
        );
    }
    if engine_gap_q4 * 2 >= tier_gap_q4 {
        // Engine offset max (2 × engine_gap_q4) must fit comfortably
        // within tier_gap so tier ordering is preserved across engines.
        panic!("engine_gap_q4 ({engine_gap_q4}) × 2 must be < tier_gap_q4 ({tier_gap_q4})");
    }

    let bootstrap_floor_q4 = read_i32(ew, "bootstrap_floor_q4");
    let char_boost_q4 = read_i32(ew, "char_boost_q4");
    let word_len_bonus_q4 = read_i32(ew, "word_len_bonus_q4");
    let fuzzy_likelihood_floor_q4 = read_i32(ew, "fuzzy_likelihood_floor_q4");
    let initials_likelihood_base_q4 = read_i32(ew, "initials_likelihood_base_q4");
    let viterbi_link_decay_q4 = read_i32(ew, "viterbi_link_decay_q4");

    // WU-β: [scoring.*] sections — formerly composite/scoring.rs
    // `pub const` block. Surfaced as `inputx_scoring::consts::*` so
    // downstream files keep their existing `pub const NAME: f64 = ...`
    // declarations and just bind to the generated value.
    let scoring_jp = parsed
        .get("scoring")
        .and_then(|v| v.get("jp"))
        .and_then(|v| v.as_table())
        .unwrap_or_else(|| panic!("[scoring.jp] section missing"));
    let scoring_ms = parsed
        .get("scoring")
        .and_then(|v| v.get("match_shape"))
        .and_then(|v| v.as_table())
        .unwrap_or_else(|| panic!("[scoring.match_shape] section missing"));
    let scoring_zh = parsed
        .get("scoring")
        .and_then(|v| v.get("zh"))
        .and_then(|v| v.as_table())
        .unwrap_or_else(|| panic!("[scoring.zh] section missing"));
    let scoring_ce = parsed
        .get("scoring")
        .and_then(|v| v.get("cross_engine"))
        .and_then(|v| v.as_table())
        .unwrap_or_else(|| panic!("[scoring.cross_engine] section missing"));

    // WU-δ: [dispatch.wubi]
    let dispatch_wubi = parsed
        .get("dispatch")
        .and_then(|v| v.get("wubi"))
        .and_then(|v| v.as_table())
        .unwrap_or_else(|| panic!("[dispatch.wubi] section missing"));
    let dw_char_prominent_floor = dispatch_wubi
        .get("char_prominent_floor_freq")
        .and_then(|v| v.as_integer())
        .unwrap_or_else(|| panic!("dispatch.wubi.char_prominent_floor_freq missing"))
        as u64;
    let dw_auto_demote_arr = dispatch_wubi
        .get("auto_layer_demote")
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| panic!("dispatch.wubi.auto_layer_demote missing"));
    let mut dw_auto_demote = [0f64; 4];
    for (i, v) in dw_auto_demote_arr.iter().enumerate().take(4) {
        dw_auto_demote[i] = v
            .as_float()
            .or_else(|| v.as_integer().map(|x| x as f64))
            .unwrap_or_else(|| panic!("auto_layer_demote[{i}] not numeric"));
    }
    let dw_phrase_speculative_demote = read_f64(dispatch_wubi, "phrase_speculative_demote");
    let dw_full_code_single_char_promote = read_f64(dispatch_wubi, "full_code_single_char_promote");
    let dw_phrase_extreme_freq_floor = dispatch_wubi
        .get("phrase_extreme_freq_floor")
        .and_then(|v| v.as_integer())
        .unwrap_or_else(|| panic!("dispatch.wubi.phrase_extreme_freq_floor missing"))
        as u64;

    let pinyin_path = parsed
        .get("pinyin_path")
        .and_then(|v| v.as_table())
        .unwrap_or_else(|| panic!("[pinyin_path] section missing"));
    let pp_phrase_base = read_f64(pinyin_path, "phrase_base");
    let pp_l0_pin_multiplier = read_f64(pinyin_path, "l0_pin_multiplier");
    let pp_non_exact_floor = read_f64(pinyin_path, "non_exact_floor");
    let pp_composed_score = read_f64(pinyin_path, "composed_score");
    let pp_composed_fallback_score = read_f64(pinyin_path, "composed_fallback_score");
    let pp_fuzzy_base = read_f64(pinyin_path, "fuzzy_base");
    let pp_composed_quality_floor = read_f64(pinyin_path, "composed_quality_floor");

    let jp_jukugo_base = read_f64(scoring_jp, "jukugo_base");
    let jp_single_kanji_base = read_f64(scoring_jp, "single_kanji_base");
    let jp_hiragana_base = read_f64(scoring_jp, "hiragana_base");
    let jp_katakana_base = read_f64(scoring_jp, "katakana_base");
    let jp_composed_base = read_f64(scoring_jp, "composed_base");
    let jp_composed_kanji_base = read_f64(scoring_jp, "composed_kanji_base");
    let jp_full_match_promote = read_f64(scoring_jp, "full_match_promote");
    let prior_freq_mult_jp = read_f64(scoring_jp, "prior_freq_mult");

    let predict_proximity_k = read_f64(scoring_ms, "predict_proximity_k");
    let predict_base_pinyin = read_f64(scoring_ms, "predict_base_pinyin");
    let predict_base_wubi = read_f64(scoring_ms, "predict_base_wubi");

    let prior_freq_mult_pinyin = read_f64(scoring_zh, "prior_freq_mult_pinyin");
    let prior_freq_mult_wubi = read_f64(scoring_zh, "prior_freq_mult_wubi");

    // Phase E (2026-06-03) — bigram quality gate floor for pinyin
    // 2-char phrase tier demote.
    let phrase_quality = parsed
        .get("scoring")
        .and_then(|v| v.get("phrase_quality"))
        .and_then(|v| v.as_table())
        .unwrap_or_else(|| panic!("[scoring.phrase_quality] section missing"));
    let pq_bigram_floor = read_f64(phrase_quality, "bigram_signal_floor");
    let pq_inflation_floor = phrase_quality
        .get("inflation_floor_freq")
        .and_then(|v| v.as_integer())
        .unwrap_or_else(|| panic!("scoring.phrase_quality.inflation_floor_freq missing"))
        as u64;
    let pq_inflation_ceil = phrase_quality
        .get("inflation_ceil_freq")
        .and_then(|v| v.as_integer())
        .unwrap_or_else(|| panic!("scoring.phrase_quality.inflation_ceil_freq missing"))
        as u64;
    if pq_inflation_ceil <= pq_inflation_floor {
        panic!(
            "scoring.phrase_quality.inflation_ceil_freq ({pq_inflation_ceil}) must be > inflation_floor_freq ({pq_inflation_floor})"
        );
    }

    let wubi_full_code_promote = read_f64(scoring_ce, "wubi_full_code_promote");
    let wubi_single_char_promote_mult = read_f64(scoring_ce, "wubi_single_char_promote_mult");
    let tc_demote_mult = read_f64(scoring_ce, "tc_demote_mult");
    let engine_mult_wubi = read_f64(scoring_ce, "engine_mult_wubi");
    let engine_mult_pinyin = read_f64(scoring_ce, "engine_mult_pinyin");
    let engine_mult_jp = read_f64(scoring_ce, "engine_mult_jp");

    // Tier 落点 Phase B (PLAN-tier-by-quantile §3.1): pinyin z-score
    // quantile parameters.  μ/σ measured per-engine on freq>0 library
    // rows.  Six z thresholds map z to tier 1..=6; below tier_6_above
    // → tier 9 longtail.  freq == 0 forced to tier 9 in the producer.
    let tq_p = parsed
        .get("scoring")
        .and_then(|v| v.get("tier_quantile_pinyin"))
        .and_then(|v| v.as_table())
        .unwrap_or_else(|| panic!("[scoring.tier_quantile_pinyin] section missing"));
    let pq_mu = read_f64(tq_p, "log_freq_mu");
    let pq_sigma = read_f64(tq_p, "log_freq_sigma");
    let pq_tier_1_above = read_f64(tq_p, "tier_1_above");
    let pq_tier_2_above = read_f64(tq_p, "tier_2_above");
    let pq_tier_3_above = read_f64(tq_p, "tier_3_above");
    let pq_tier_4_above = read_f64(tq_p, "tier_4_above");
    let pq_tier_5_above = read_f64(tq_p, "tier_5_above");
    let pq_tier_6_above = read_f64(tq_p, "tier_6_above");
    // [scoring.tier_wubi] — engine-internal wubi freq→tier (plain freq
    // floors, no z-score; no cross-engine pinyin dependency). 2026-06-07.
    let tw = parsed
        .get("scoring")
        .and_then(|v| v.get("tier_wubi"))
        .and_then(|v| v.as_table())
        .unwrap_or_else(|| panic!("[scoring.tier_wubi] section missing"));
    let wt1 = read_i32(tw, "tier_1_floor");
    let wt2 = read_i32(tw, "tier_2_floor");
    let wt3 = read_i32(tw, "tier_3_floor");
    let wt4 = read_i32(tw, "tier_4_floor");
    if pq_sigma <= 0.0 {
        panic!("scoring.tier_quantile_pinyin.log_freq_sigma must be > 0; got {pq_sigma}");
    }
    for (a, b, name) in [
        (
            pq_tier_1_above,
            pq_tier_2_above,
            "tier_1_above > tier_2_above",
        ),
        (
            pq_tier_2_above,
            pq_tier_3_above,
            "tier_2_above > tier_3_above",
        ),
        (
            pq_tier_3_above,
            pq_tier_4_above,
            "tier_3_above > tier_4_above",
        ),
        (
            pq_tier_4_above,
            pq_tier_5_above,
            "tier_4_above > tier_5_above",
        ),
        (
            pq_tier_5_above,
            pq_tier_6_above,
            "tier_5_above > tier_6_above",
        ),
    ] {
        if a.partial_cmp(&b) != Some(std::cmp::Ordering::Greater) {
            panic!("scoring.tier_quantile_pinyin {name} violated: {a} !> {b}");
        }
    }

    // Phase D — nihongo z-score quantile (mirror of pinyin Phase B).
    let tq_n = parsed
        .get("scoring")
        .and_then(|v| v.get("tier_quantile_nihongo"))
        .and_then(|v| v.as_table())
        .unwrap_or_else(|| panic!("[scoring.tier_quantile_nihongo] section missing"));
    let nq_mu = read_f64(tq_n, "log_freq_mu");
    let nq_sigma = read_f64(tq_n, "log_freq_sigma");
    let nq_tier_1_above = read_f64(tq_n, "tier_1_above");
    let nq_tier_2_above = read_f64(tq_n, "tier_2_above");
    let nq_tier_3_above = read_f64(tq_n, "tier_3_above");
    let nq_tier_4_above = read_f64(tq_n, "tier_4_above");
    let nq_tier_5_above = read_f64(tq_n, "tier_5_above");
    let nq_tier_6_above = read_f64(tq_n, "tier_6_above");
    if nq_sigma <= 0.0 {
        panic!("scoring.tier_quantile_nihongo.log_freq_sigma must be > 0; got {nq_sigma}");
    }
    for (a, b, name) in [
        (
            nq_tier_1_above,
            nq_tier_2_above,
            "tier_1_above > tier_2_above",
        ),
        (
            nq_tier_2_above,
            nq_tier_3_above,
            "tier_2_above > tier_3_above",
        ),
        (
            nq_tier_3_above,
            nq_tier_4_above,
            "tier_3_above > tier_4_above",
        ),
        (
            nq_tier_4_above,
            nq_tier_5_above,
            "tier_4_above > tier_5_above",
        ),
        (
            nq_tier_5_above,
            nq_tier_6_above,
            "tier_5_above > tier_6_above",
        ),
    ] {
        if a.partial_cmp(&b) != Some(std::cmp::Ordering::Greater) {
            panic!("scoring.tier_quantile_nihongo {name} violated: {a} !> {b}");
        }
    }

    let generated = format!(
        r#"// AUTO-GENERATED by inputx-scoring/build.rs from
// inputx-scoring/data/engine_weights.toml — DO NOT EDIT BY HAND.
// Edit the TOML and rebuild.

/// Const values mirroring `[engine_weights]` in engine_weights.toml,
/// embedded at build time. `EngineWeights::inputx_default()` reads
/// from this module.
pub(crate) mod __engine_weights_generated {{
    pub const BOOTSTRAP_FLOOR_Q4: i32 = {bootstrap};
    pub const CHAR_BOOST_Q4: i32 = {char_b};
    pub const WORD_LEN_BONUS_Q4: i32 = {word_len};
    pub const FUZZY_LIKELIHOOD_FLOOR_Q4: i32 = {fuzzy};
    pub const INITIALS_LIKELIHOOD_BASE_Q4: i32 = {initials};
    pub const VITERBI_LINK_DECAY_Q4: i32 = {viterbi};
}}

/// WU-ψ tier × engine base score table (v1.11). All values in Q4
/// log-space. `TIER_BASE_Q4[tier][engine]` = the floor score for
/// any candidate placed in `tier` produced by `engine`, with the
/// caveat that within-tier ordering uses an additional clamped
/// `within_tier_q4 ∈ [0, WITHIN_TIER_MAX_Q4]`.
///
/// Derived from `[tier_engine_base]` in `engine_weights.toml`:
///   row(tier) = TIER_GAP_Q4 × (TIER_COUNT − 1 − tier)
///   engine(src) = ENGINE_GAP_Q4 × (offset_for_source(src))
/// where wubi → +2·gap, pinyin → +1·gap, nihongo → +0.
/// Tier 0 is highest (top of candidates list).
pub mod tier {{
    // ENGINE_OFFSET_Q4 below is written uniformly as `N * ENGINE_GAP_Q4`
    // (offset-step × gap) for readability; the N=0 / N=1 rows trip
    // erasing_op / identity_op (deny-by-default). The uniform form is
    // intentional and clearer than special-casing 0 and 1.
    #![allow(clippy::erasing_op, clippy::identity_op)]
    pub const TIER_GAP_Q4: i32 = {tier_gap_q4};
    pub const ENGINE_GAP_Q4: i32 = {engine_gap_q4};
    pub const WITHIN_TIER_MAX_Q4: i32 = {within_tier_max_q4};
    pub const TIER_COUNT: usize = {tier_count};

    /// Per-source engine offset in the row. wubi gets the largest
    /// (highest rank within a tier), nihongo gets 0.
    /// Indexed as Source::* `as usize` — must match
    /// crate::Source layout.
    pub const ENGINE_OFFSET_Q4: [i32; 3] = [
        2 * ENGINE_GAP_Q4,  // [0] Wubi
        1 * ENGINE_GAP_Q4,  // [1] Pinyin
        0 * ENGINE_GAP_Q4,  // [2] Japanese
    ];

    /// Lookup the base Q4 score for `(tier, source)`.
    ///
    /// Saturates: `tier >= TIER_COUNT` is treated as the bottom tier,
    /// `source as usize >= 3` is treated as Japanese (the floor).
    /// Producers should pass valid tiers; the saturation is a guard
    /// against out-of-range u8 values without panicking the hot path.
    #[inline]
    pub const fn tier_base_q4(tier: u8, source_idx: u8) -> i32 {{
        let t = if (tier as usize) >= TIER_COUNT {{
            TIER_COUNT - 1
        }} else {{
            tier as usize
        }};
        let s = if (source_idx as usize) >= 3 {{
            2
        }} else {{
            source_idx as usize
        }};
        TIER_GAP_Q4 * (TIER_COUNT as i32 - 1 - t as i32)
            + ENGINE_OFFSET_Q4[s]
    }}
}}

/// Polish-formula constants — every value in
/// `data/engine_weights.toml`'s `[scoring.*]` sections, surfaced for
/// cross-crate consumers. `composite/scoring.rs` (in `inputx-core`)
/// binds its `pub const NAME: f64` declarations to these so
/// callers' existing import paths (`inputx_core::composite::scoring::FOO`)
/// keep working — the constant *value* moves to the TOML, the
/// constant *name* + documentation stay where they always lived.
pub mod consts {{
    // [scoring.jp]
    pub const LIKELIHOOD_JP_JUKUGO_BASE: f64 = {jp_jukugo_base};
    pub const LIKELIHOOD_JP_SINGLE_KANJI_BASE: f64 = {jp_single_kanji_base};
    pub const LIKELIHOOD_JP_HIRAGANA_BASE: f64 = {jp_hiragana_base};
    pub const LIKELIHOOD_JP_KATAKANA_BASE: f64 = {jp_katakana_base};
    pub const LIKELIHOOD_JP_COMPOSED_BASE: f64 = {jp_composed_base};
    pub const LIKELIHOOD_JP_COMPOSED_KANJI_BASE: f64 = {jp_composed_kanji_base};
    pub const LIKELIHOOD_JP_FULL_MATCH_PROMOTE: f64 = {jp_full_match_promote};
    pub const PRIOR_FREQ_MULT_JP: f64 = {prior_freq_mult_jp};
    // [scoring.match_shape]
    pub const LIKELIHOOD_PREDICT_PROXIMITY_K: f64 = {predict_proximity_k};
    pub const LIKELIHOOD_PINYIN_PREDICT_BASE: f64 = {predict_base_pinyin};
    pub const LIKELIHOOD_WUBI_PREDICT_BASE: f64 = {predict_base_wubi};
    // [scoring.zh]
    pub const PRIOR_FREQ_MULT_PINYIN: f64 = {prior_freq_mult_pinyin};
    pub const PRIOR_FREQ_MULT_WUBI: f64 = {prior_freq_mult_wubi};
    // [scoring.cross_engine]
    pub const LIKELIHOOD_WUBI_FULL_CODE_PROMOTE: f64 = {wubi_full_code_promote};
    pub const LIKELIHOOD_WUBI_SINGLE_CHAR_PROMOTE_MULT: f64 = {wubi_single_char_promote_mult};
    pub const LIKELIHOOD_TC_DEMOTE_MULT: f64 = {tc_demote_mult};
    pub const LIKELIHOOD_ENGINE_MULT_WUBI: f64 = {engine_mult_wubi};
    pub const LIKELIHOOD_ENGINE_MULT_PINYIN: f64 = {engine_mult_pinyin};
    pub const LIKELIHOOD_ENGINE_MULT_JP: f64 = {engine_mult_jp};
    // [pinyin_path]
    pub const PINYIN_PHRASE_BASE: f64 = {pp_phrase_base};
    pub const L0_PIN_MULTIPLIER: f64 = {pp_l0_pin_multiplier};
    pub const NON_EXACT_FLOOR: f64 = {pp_non_exact_floor};
    pub const COMPOSED_SCORE: f64 = {pp_composed_score};
    pub const COMPOSED_FALLBACK_SCORE: f64 = {pp_composed_fallback_score};
    pub const FUZZY_BASE: f64 = {pp_fuzzy_base};
    pub const COMPOSED_QUALITY_FLOOR: f64 = {pp_composed_quality_floor};
    // [dispatch.wubi]
    pub const WUBI_CHAR_PROMINENT_FLOOR_FREQ: u64 = {dw_floor};
    pub const WUBI_AUTO_LAYER_DEMOTE: [f64; 4] = [{dw_ad0}, {dw_ad1}, {dw_ad2}, {dw_ad3}];
    pub const WUBI_PHRASE_SPECULATIVE_DEMOTE: f64 = {dw_phr};
    pub const WUBI_FULL_CODE_SINGLE_CHAR_PROMOTE: f64 = {dw_sgl};
    pub const WUBI_PHRASE_EXTREME_FREQ_FLOOR: u64 = {dw_phr_floor};
    // [scoring.tier_quantile_pinyin] — Phase B (2026-06-03):
    // z-score quantile tier mapping for pinyin single-char candidates.
    // See PLAN-tier-by-quantile.md for the math + .claude/PLAN-tier-by-
    // quantile-spike-data.md for the spike that fixed μ, σ, thresholds.
    pub const PINYIN_LOG_FREQ_MU: f64 = {pq_mu};
    pub const PINYIN_LOG_FREQ_SIGMA: f64 = {pq_sigma};
    pub const PINYIN_TIER_1_Z_ABOVE: f64 = {pq_t1};
    pub const PINYIN_TIER_2_Z_ABOVE: f64 = {pq_t2};
    pub const PINYIN_TIER_3_Z_ABOVE: f64 = {pq_t3};
    pub const PINYIN_TIER_4_Z_ABOVE: f64 = {pq_t4};
    pub const PINYIN_TIER_5_Z_ABOVE: f64 = {pq_t5};
    pub const PINYIN_TIER_6_Z_ABOVE: f64 = {pq_t6};
    // [scoring.tier_quantile_nihongo] — Phase D (2026-06-03).
    pub const NIHONGO_LOG_FREQ_MU: f64 = {nq_mu};
    pub const NIHONGO_LOG_FREQ_SIGMA: f64 = {nq_sigma};
    pub const NIHONGO_TIER_1_Z_ABOVE: f64 = {nq_t1};
    pub const NIHONGO_TIER_2_Z_ABOVE: f64 = {nq_t2};
    pub const NIHONGO_TIER_3_Z_ABOVE: f64 = {nq_t3};
    pub const NIHONGO_TIER_4_Z_ABOVE: f64 = {nq_t4};
    pub const NIHONGO_TIER_5_Z_ABOVE: f64 = {nq_t5};
    pub const NIHONGO_TIER_6_Z_ABOVE: f64 = {nq_t6};
    // [scoring.phrase_quality] — Phase E (2026-06-03).
    pub const PHRASE_BIGRAM_SIGNAL_FLOOR: f64 = {phrase_bigram_floor};
    pub const PHRASE_INFLATION_FLOOR_FREQ: u64 = {phrase_inflation_floor};
    pub const PHRASE_INFLATION_CEIL_FREQ: u64 = {phrase_inflation_ceil};
    // [scoring.tier_wubi] — wubi freq→tier floors (engine-internal, 2026-06-07).
    pub const WUBI_TIER_1_FLOOR: u64 = {wt1};
    pub const WUBI_TIER_2_FLOOR: u64 = {wt2};
    pub const WUBI_TIER_3_FLOOR: u64 = {wt3};
    pub const WUBI_TIER_4_FLOOR: u64 = {wt4};
}}

/// Tier落点 helper — Phase B (2026-06-03).
///
/// Maps a pinyin candidate's raw freq to its natural tier via z-score
/// position in the per-engine log-freq distribution.  See
/// PLAN-tier-by-quantile.md §5.1.
///
/// CALLERS: composite/pinyin_adapter.rs single-char exact-match path.
/// Multi-char phrases keep their unconditional tier 1 (phrase priority);
/// non-exact (initials / fuzzy / predict) paths use their own scoring.
///
/// SEMANTICS:
///   - freq == 0 → tier 9 (longtail).  These rows exist in library
///     for reverse-lookup only; they should never participate in
///     forward ranking.
///   - freq > 0  → z = (ln(freq) - μ) / σ; walk descending thresholds
///     to find the highest tier whose `Z_ABOVE` z satisfies.
pub fn pinyin_tier_from_freq(raw_freq: u64) -> u8 {{
    if raw_freq == 0 {{ return 9; }}
    let z = ((raw_freq as f64).ln() - consts::PINYIN_LOG_FREQ_MU)
        / consts::PINYIN_LOG_FREQ_SIGMA;
    if z >= consts::PINYIN_TIER_1_Z_ABOVE {{ 1 }}
    else if z >= consts::PINYIN_TIER_2_Z_ABOVE {{ 2 }}
    else if z >= consts::PINYIN_TIER_3_Z_ABOVE {{ 3 }}
    else if z >= consts::PINYIN_TIER_4_Z_ABOVE {{ 4 }}
    else if z >= consts::PINYIN_TIER_5_Z_ABOVE {{ 5 }}
    else if z >= consts::PINYIN_TIER_6_Z_ABOVE {{ 6 }}
    else {{ 9 }}
}}

/// Tier 落点 helper — Phase D (2026-06-03 — nihongo).
///
/// Same shape as `pinyin_tier_from_freq` but with nihongo-tuned
/// μ/σ + "wide & low" z thresholds (per user directive: 日语整体应
/// 偏低,只是张得相对开,基础假名在很高级).  Applies to nihongo
/// candidate paths that go through freq quantile:
///
///   - multi-char real jukugo (新宿 大学 自主)
///   - single kanji (気 起 記)
///
/// EXEMPT paths (still use fixed tier in `japanese_adapter.rs`):
///   - multi-char pure_kana (えっ ありがとう)     → tier 2 (Phase C)
///   - single basic kana from dict (も で を)     → tier 1 (Phase C)
///   - mechanical kana 3-band by buffer length    → Phase C-2
pub fn nihongo_tier_from_freq(raw_freq: u64) -> u8 {{
    if raw_freq == 0 {{ return 9; }}
    let z = ((raw_freq as f64).ln() - consts::NIHONGO_LOG_FREQ_MU)
        / consts::NIHONGO_LOG_FREQ_SIGMA;
    if z >= consts::NIHONGO_TIER_1_Z_ABOVE {{ 1 }}
    else if z >= consts::NIHONGO_TIER_2_Z_ABOVE {{ 2 }}
    else if z >= consts::NIHONGO_TIER_3_Z_ABOVE {{ 3 }}
    else if z >= consts::NIHONGO_TIER_4_Z_ABOVE {{ 4 }}
    else if z >= consts::NIHONGO_TIER_5_Z_ABOVE {{ 5 }}
    else if z >= consts::NIHONGO_TIER_6_Z_ABOVE {{ 6 }}
    else {{ 9 }}
}}

/// Tier落点 — wubi by its OWN corpus freq (2026-06-07, orthogonal-table
/// design). Plain freq floors, no z-score: wubi is encoding-stable and
/// low-cardinality so it converges high; within-tier order is deliberately
/// not modeled (五笔不关心 t1 内谁高). Semantics: t1 常用 / t2 中低频 /
/// t3 低频 / t4 难检 / t5 生僻. Engine-internal — NO pinyin char_max_freq
/// dependency. CALLERS: composite/dispatch.rs jianma1/2/3 single-char path.
pub fn wubi_tier_from_freq(raw_freq: u64) -> u8 {{
    if raw_freq >= consts::WUBI_TIER_1_FLOOR {{ 1 }}
    else if raw_freq >= consts::WUBI_TIER_2_FLOOR {{ 2 }}
    else if raw_freq >= consts::WUBI_TIER_3_FLOOR {{ 3 }}
    else if raw_freq >= consts::WUBI_TIER_4_FLOOR {{ 4 }}
    else {{ 5 }}
}}
"#,
        bootstrap = bootstrap_floor_q4,
        char_b = char_boost_q4,
        word_len = word_len_bonus_q4,
        fuzzy = fuzzy_likelihood_floor_q4,
        initials = initials_likelihood_base_q4,
        viterbi = viterbi_link_decay_q4,
        tier_gap_q4 = tier_gap_q4,
        engine_gap_q4 = engine_gap_q4,
        within_tier_max_q4 = within_tier_max_q4,
        tier_count = tier_count,
        jp_jukugo_base = fmt_f64(jp_jukugo_base),
        jp_single_kanji_base = fmt_f64(jp_single_kanji_base),
        jp_hiragana_base = fmt_f64(jp_hiragana_base),
        jp_katakana_base = fmt_f64(jp_katakana_base),
        jp_composed_base = fmt_f64(jp_composed_base),
        jp_composed_kanji_base = fmt_f64(jp_composed_kanji_base),
        jp_full_match_promote = fmt_f64(jp_full_match_promote),
        prior_freq_mult_jp = fmt_f64(prior_freq_mult_jp),
        predict_proximity_k = fmt_f64(predict_proximity_k),
        predict_base_pinyin = fmt_f64(predict_base_pinyin),
        predict_base_wubi = fmt_f64(predict_base_wubi),
        prior_freq_mult_pinyin = fmt_f64(prior_freq_mult_pinyin),
        prior_freq_mult_wubi = fmt_f64(prior_freq_mult_wubi),
        wubi_full_code_promote = fmt_f64(wubi_full_code_promote),
        wubi_single_char_promote_mult = fmt_f64(wubi_single_char_promote_mult),
        tc_demote_mult = fmt_f64(tc_demote_mult),
        engine_mult_wubi = fmt_f64(engine_mult_wubi),
        engine_mult_pinyin = fmt_f64(engine_mult_pinyin),
        engine_mult_jp = fmt_f64(engine_mult_jp),
        pp_phrase_base = fmt_f64(pp_phrase_base),
        pp_l0_pin_multiplier = fmt_f64(pp_l0_pin_multiplier),
        pp_non_exact_floor = fmt_f64(pp_non_exact_floor),
        pp_composed_score = fmt_f64(pp_composed_score),
        pp_composed_fallback_score = fmt_f64(pp_composed_fallback_score),
        pp_fuzzy_base = fmt_f64(pp_fuzzy_base),
        pp_composed_quality_floor = fmt_f64(pp_composed_quality_floor),
        dw_floor = dw_char_prominent_floor,
        dw_ad0 = fmt_f64(dw_auto_demote[0]),
        dw_ad1 = fmt_f64(dw_auto_demote[1]),
        dw_ad2 = fmt_f64(dw_auto_demote[2]),
        dw_ad3 = fmt_f64(dw_auto_demote[3]),
        dw_phr = fmt_f64(dw_phrase_speculative_demote),
        dw_sgl = fmt_f64(dw_full_code_single_char_promote),
        dw_phr_floor = dw_phrase_extreme_freq_floor,
        pq_mu = fmt_f64(pq_mu),
        pq_sigma = fmt_f64(pq_sigma),
        pq_t1 = fmt_f64(pq_tier_1_above),
        pq_t2 = fmt_f64(pq_tier_2_above),
        pq_t3 = fmt_f64(pq_tier_3_above),
        pq_t4 = fmt_f64(pq_tier_4_above),
        pq_t5 = fmt_f64(pq_tier_5_above),
        pq_t6 = fmt_f64(pq_tier_6_above),
        nq_mu = fmt_f64(nq_mu),
        nq_sigma = fmt_f64(nq_sigma),
        nq_t1 = fmt_f64(nq_tier_1_above),
        nq_t2 = fmt_f64(nq_tier_2_above),
        nq_t3 = fmt_f64(nq_tier_3_above),
        nq_t4 = fmt_f64(nq_tier_4_above),
        nq_t5 = fmt_f64(nq_tier_5_above),
        nq_t6 = fmt_f64(nq_tier_6_above),
        phrase_bigram_floor = fmt_f64(pq_bigram_floor),
        phrase_inflation_floor = pq_inflation_floor,
        phrase_inflation_ceil = pq_inflation_ceil,
        wt1 = wt1,
        wt2 = wt2,
        wt3 = wt3,
        wt4 = wt4,
    );

    fs::write(&out_path, generated).unwrap_or_else(|e| panic!("write {}: {e}", out_path.display()));

    // ─── WU-ψ phase 5: tier overlay codegen ─────────────────────
    //
    // Parse `tier_overlay.tsv` into a sorted `&[(buffer, word, tier)]`
    // slice. Lookup is O(log n) via `binary_search_by` over the
    // sorted key. Sort by (buffer, word) ascending.
    let mut overlay_rows: Vec<(String, String, u8)> = Vec::new();
    let overlay_text = fs::read_to_string(&tier_overlay_path).unwrap_or_else(|e| {
        panic!(
            "read tier_overlay.tsv at {}: {e}",
            tier_overlay_path.display()
        )
    });
    for (line_no, raw) in overlay_text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Drop trailing `# comment` so the comment isn't parsed as
        // part of the tier field.
        let payload = if let Some(idx) = line.find('#') {
            line[..idx].trim_end()
        } else {
            line
        };
        let parts: Vec<&str> = payload.split('\t').collect();
        if parts.len() < 3 {
            panic!(
                "tier_overlay.tsv:{}: expected `<buffer>\\t<word>\\t<tier>`, got {raw:?}",
                line_no + 1
            );
        }
        let buffer = parts[0].trim().to_ascii_lowercase();
        let word = parts[1].trim().to_string();
        let tier: u8 = parts[2].trim().parse().unwrap_or_else(|_| {
            panic!(
                "tier_overlay.tsv:{}: tier field not a valid u8: {:?}",
                line_no + 1,
                parts[2]
            )
        });
        if (tier as i32) >= tier_count {
            panic!(
                "tier_overlay.tsv:{}: tier {tier} >= tier_count {tier_count}",
                line_no + 1
            );
        }
        overlay_rows.push((buffer, word, tier));
    }
    overlay_rows.sort();
    overlay_rows.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);

    let mut overlay_lines = String::new();
    overlay_lines.push_str("&[\n");
    for (b, w, t) in &overlay_rows {
        overlay_lines.push_str(&format!("    ({b:?}, {w:?}, {t}u8),\n"));
    }
    overlay_lines.push(']');
    let overlay_generated = format!(
        r#"// AUTO-GENERATED by inputx-scoring/build.rs from
// tools/scoring/data/polish/tier_overlay.tsv —
// DO NOT EDIT BY HAND. Edit the TSV and rebuild.

/// Sorted `(buffer_lc, word, tier)` triples — binary-search
/// lookup via `tier_overlay_get(buffer, word)`.
pub static TIER_OVERLAY_ROWS: &[(&str, &str, u8)] = {overlay_lines};
"#
    );
    let overlay_out = out_dir.join("tier_overlay_generated.rs");
    fs::write(&overlay_out, overlay_generated)
        .unwrap_or_else(|e| panic!("write {}: {e}", overlay_out.display()));
}

fn read_i32(t: &toml::value::Table, key: &str) -> i32 {
    t.get(key)
        .and_then(|v| v.as_integer())
        .unwrap_or_else(|| panic!("engine_weights.{key} missing or not integer")) as i32
}

fn read_f64(t: &toml::value::Table, key: &str) -> f64 {
    let v = t.get(key).unwrap_or_else(|| panic!("missing key {key}"));
    if let Some(f) = v.as_float() {
        return f;
    }
    if let Some(i) = v.as_integer() {
        return i as f64;
    }
    panic!("key {key} not float/int");
}

/// Render an f64 so Rust source parses it back to the same bit value
/// — uses precise debug formatting and ensures a `.` (so the literal
/// is f64 not int).
fn fmt_f64(f: f64) -> String {
    if f.is_finite() && f == f.trunc() {
        // e.g. 200000.0 — make sure the literal shows as `200000.0_f64`
        // shape, otherwise rustc reads `200000` as i32 and the const-
        // assign fails.
        format!("{f}_f64")
    } else {
        // Use `:?` which gives a round-trippable representation
        // (e.g. 1.0e-3 → "1e-3" or "0.001" — both valid f64 literals).
        format!("{f:?}_f64")
    }
}
