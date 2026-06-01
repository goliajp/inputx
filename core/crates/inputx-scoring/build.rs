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

    let text = fs::read_to_string(&toml_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", toml_path.display()));
    let parsed: toml::Value = toml::from_str(&text)
        .unwrap_or_else(|e| panic!("parse {}: {e}", toml_path.display()));

    let ew = parsed
        .get("engine_weights")
        .and_then(|v| v.as_table())
        .unwrap_or_else(|| panic!("engine_weights.toml: missing [engine_weights] table"));

    let engine_boost_q4 = ew
        .get("engine_boost_q4")
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| panic!("engine_weights.engine_boost_q4 missing or not array"));
    let mut eb = [0i32; 3];
    for (i, v) in engine_boost_q4.iter().enumerate().take(3) {
        eb[i] = v.as_integer().unwrap_or_else(|| {
            panic!("engine_boost_q4[{i}] not integer")
        }) as i32;
    }

    let simcode_boost_q4 = read_i32(ew, "simcode_boost_q4");
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
        .unwrap_or_else(|| panic!("dispatch.wubi.char_prominent_floor_freq missing")) as u64;
    let dw_rare_char_demote = read_f64(dispatch_wubi, "rare_char_demote");
    let dw_auto_demote_arr = dispatch_wubi
        .get("auto_layer_demote")
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| panic!("dispatch.wubi.auto_layer_demote missing"));
    let mut dw_auto_demote = [0f64; 4];
    for (i, v) in dw_auto_demote_arr.iter().enumerate().take(4) {
        dw_auto_demote[i] = v.as_float().or_else(|| v.as_integer().map(|x| x as f64))
            .unwrap_or_else(|| panic!("auto_layer_demote[{i}] not numeric"));
    }
    let dw_phrase_speculative_demote = read_f64(dispatch_wubi, "phrase_speculative_demote");
    let dw_full_code_single_char_promote = read_f64(dispatch_wubi, "full_code_single_char_promote");

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
    let pp_fuzzy_discount = read_f64(pinyin_path, "fuzzy_discount");
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

    let wubi_full_code_promote = read_f64(scoring_ce, "wubi_full_code_promote");
    let wubi_single_char_promote_mult = read_f64(scoring_ce, "wubi_single_char_promote_mult");
    let tc_demote_mult = read_f64(scoring_ce, "tc_demote_mult");
    let engine_mult_wubi = read_f64(scoring_ce, "engine_mult_wubi");
    let engine_mult_pinyin = read_f64(scoring_ce, "engine_mult_pinyin");
    let engine_mult_jp = read_f64(scoring_ce, "engine_mult_jp");

    let generated = format!(
        r#"// AUTO-GENERATED by inputx-scoring/build.rs from
// inputx-scoring/data/engine_weights.toml — DO NOT EDIT BY HAND.
// Edit the TOML and rebuild.

/// Const values mirroring `[engine_weights]` in engine_weights.toml,
/// embedded at build time. `EngineWeights::inputx_default()` reads
/// from this module.
pub(crate) mod __engine_weights_generated {{
    pub const ENGINE_BOOST_Q4: [i32; 3] = [{eb0}, {eb1}, {eb2}];
    pub const SIMCODE_BOOST_Q4: i32 = {simcode};
    pub const BOOTSTRAP_FLOOR_Q4: i32 = {bootstrap};
    pub const CHAR_BOOST_Q4: i32 = {char_b};
    pub const WORD_LEN_BONUS_Q4: i32 = {word_len};
    pub const FUZZY_LIKELIHOOD_FLOOR_Q4: i32 = {fuzzy};
    pub const INITIALS_LIKELIHOOD_BASE_Q4: i32 = {initials};
    pub const VITERBI_LINK_DECAY_Q4: i32 = {viterbi};
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
    pub const FUZZY_DISCOUNT: f64 = {pp_fuzzy_discount};
    pub const COMPOSED_QUALITY_FLOOR: f64 = {pp_composed_quality_floor};
    // [dispatch.wubi]
    pub const WUBI_CHAR_PROMINENT_FLOOR_FREQ: u64 = {dw_floor};
    pub const WUBI_RARE_CHAR_DEMOTE: f64 = {dw_rare};
    pub const WUBI_AUTO_LAYER_DEMOTE: [f64; 4] = [{dw_ad0}, {dw_ad1}, {dw_ad2}, {dw_ad3}];
    pub const WUBI_PHRASE_SPECULATIVE_DEMOTE: f64 = {dw_phr};
    pub const WUBI_FULL_CODE_SINGLE_CHAR_PROMOTE: f64 = {dw_sgl};
}}
"#,
        eb0 = eb[0],
        eb1 = eb[1],
        eb2 = eb[2],
        simcode = simcode_boost_q4,
        bootstrap = bootstrap_floor_q4,
        char_b = char_boost_q4,
        word_len = word_len_bonus_q4,
        fuzzy = fuzzy_likelihood_floor_q4,
        initials = initials_likelihood_base_q4,
        viterbi = viterbi_link_decay_q4,
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
        pp_fuzzy_discount = fmt_f64(pp_fuzzy_discount),
        pp_composed_quality_floor = fmt_f64(pp_composed_quality_floor),
        dw_floor = dw_char_prominent_floor,
        dw_rare = fmt_f64(dw_rare_char_demote),
        dw_ad0 = fmt_f64(dw_auto_demote[0]),
        dw_ad1 = fmt_f64(dw_auto_demote[1]),
        dw_ad2 = fmt_f64(dw_auto_demote[2]),
        dw_ad3 = fmt_f64(dw_auto_demote[3]),
        dw_phr = fmt_f64(dw_phrase_speculative_demote),
        dw_sgl = fmt_f64(dw_full_code_single_char_promote),
    );

    fs::write(&out_path, generated)
        .unwrap_or_else(|e| panic!("write {}: {e}", out_path.display()));
}

fn read_i32(t: &toml::value::Table, key: &str) -> i32 {
    t.get(key)
        .and_then(|v| v.as_integer())
        .unwrap_or_else(|| panic!("engine_weights.{key} missing or not integer"))
        as i32
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
