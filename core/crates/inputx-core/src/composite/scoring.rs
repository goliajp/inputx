//! Centralized scoring model — the single source of truth for how
//! Inputx ranks candidates across wubi / pinyin / JP.
//!
//! # The single principle
//!
//! **Candidate ranking is sort-by-score-descending. Always. Everywhere.**
//!
//! There are no "rules" in code that re-order candidates. Every effect
//! you see in the candidate list — including Inputx 五笔's "wubi simcode
//! at #0" promise, the "5+ char wubi out" behavior, the TC demote, and
//! L0 pins surfacing — is the consequence of a SCORE assigned to a
//! candidate. Higher score → earlier position. That's the entire model.
//!
//! If a candidate ranks where you don't want it, the fix is *always*
//! adjusting its score. It is never "add a rule".
//!
//! # Why Jianma1 (e→有, g→一, …) sits at #0
//!
//! Not because there's an `if Jianma1 { put_first() }` somewhere. Because
//! `inputx_wubi::layer::LAYER_BASE[Jianma1] = 1_000_000`. The score
//! formula for a wubi candidate is `layer.base() × pref + freq`, so a
//! Jianma1 candidate scores ~1.04M. Pinyin's top-phrase score ceiling
//! sits at ~480k. Sort descending → Jianma1 wins. No rule, just numbers.
//!
//! # Why "5+ char input → wubi disappears"
//!
//! Not because dispatch.rs has `if buffer_len > 4 { skip_wubi() }`.
//! Because dispatch multiplies all wubi candidate scores by
//! `wubi_length_modifier(buffer_len)`, which is 1.0 inside the 4-char
//! window and 0.0 outside. Zero-scored candidates sort to the bottom
//! and get cut by `MAX_PER_INPUT`. The user sees "wubi gone past 4
//! chars" but the mechanism is pure scoring.
//!
//! # Score components, top to bottom
//!
//! Each candidate's final score is the product of these factors:
//!
//! 1. **Engine-internal base + freq**. See the LAYER_BASE table in
//!    `inputx_wubi::layer` (Jianma1=1M / Jianma2=800k / Jianma3=600k /
//!    Zigen=500k / Phrase=400k / Auto=70k), `PINYIN_PHRASE_BASE` in
//!    `inputx_pinyin::dict` (=400k), and the `JP_*_SCORE` consts below.
//!
//! 2. **Multiplicative modifiers** applied at dispatch / merge / engine-
//!    internal time. Each is a real number; no special-case logic:
//!      - `wubi_length_modifier()` — 1.0 inside 4-char window, 0.0 beyond.
//!      - `TC_DEMOTE_MULTIPLIER` (1e-3) — applied to candidates with any
//!        traditional-Chinese-only char.
//!      - `L0_PIN_MULTIPLIER` (1000) — applied inside the engine's
//!        `lookup_with_scores_into` when the candidate matches a user pin.
//!      - `WUBI_SINGLE_CHAR_PROMOTE_MULTIPLIER` (100) — applied inside
//!        `inputx_wubi::dict::lookup_with_scores_into` at full-code
//!        length, when the single-char freq exceeds max phrase freq.
//!
//! Adjust the constants here, watch the candidate list reorder.

/// JP jukugo (multi-char kanji compound) synthetic score. Slots between
/// wubi Auto (~70k) and pinyin top (~480k).
pub const JP_JUKUGO_SCORE: f64 = 300_000.0;

/// JP single-kanji (on/kun whole-buffer reading) synthetic score.
pub const JP_SINGLE_KANJI_SCORE: f64 = 200_000.0;

/// JP hiragana (mechanical romaji→kana) synthetic score.
pub const JP_HIRAGANA_SCORE: f64 = 100_000.0;

/// JP katakana synthetic score.
pub const JP_KATAKANA_SCORE: f64 = 90_000.0;

/// Past this input length (pinyin-buffer chars), wubi candidate scores
/// get multiplied by 0.0 via `wubi_length_modifier`. Effect: wubi
/// vanishes from the user-visible list past 4 chars because the user
/// is clearly typing pinyin and wubi's defuse-tail interpretations are
/// mechanical noise.
pub const WUBI_MAX_BUFFER_LEN: usize = 4;

/// Multiplier on candidates containing any traditional-Chinese-only
/// char (per OpenCC t2s map). Pulls TC variants below their SC siblings
/// while still leaving them in the list if no SC equivalent exists.
pub const TC_DEMOTE_MULTIPLIER: f64 = 1e-3;

/// Multiplier applied to L0-pinned words inside the engine's
/// `lookup_with_scores_into`. Brings any pin above any natural score:
/// Jianma1 (1.04M) × 1.0 = 1.04M; pinyin top (444k) × 1000 = 444M.
/// Pin wins.
pub const L0_PIN_MULTIPLIER: f64 = 1000.0;

/// Multiplier applied to a wubi single-char candidate at full-code
/// input length when its freq exceeds the max phrase freq at the same
/// code. Lifts e.g. 两 (single char, freq 37k) above 两败俱伤 (phrase,
/// freq 15k) at code `gmww`.
pub const WUBI_SINGLE_CHAR_PROMOTE_MULTIPLIER: f64 = 100.0;

/// Score floor recognizing "this is a wubi Jianma1 hit". Used by
/// diagnostic / FFI code. Today = LAYER_BASE[Jianma1] in inputx-wubi.
pub const JIANMA1_THRESHOLD: f64 = 1_000_000.0;

/// Score multiplier for wubi candidates given the user's input length.
/// Inside the 4-char window → 1.0 (wubi competes normally). Outside →
/// 0.0 (wubi candidates score 0, sort bottom, get cut by cap).
pub fn wubi_length_modifier(input_len: usize) -> f64 {
    if input_len <= WUBI_MAX_BUFFER_LEN { 1.0 } else { 0.0 }
}

// ─── Future tunables (not yet wired) ───────────────────────────────────

/// Per-engine global multiplier — would scale raw scores per engine.
/// Currently NOT applied (each engine's score taken as-is). v0.3 lever
/// for cross-engine calibration.
#[allow(dead_code)]
pub const ENGINE_MULT_WUBI: f64 = 1.0;
#[allow(dead_code)]
pub const ENGINE_MULT_PINYIN: f64 = 1.0;
#[allow(dead_code)]
pub const ENGINE_MULT_JP: f64 = 1.0;

/// Length bias (not yet wired). Future: short phrases get a small boost,
/// long phrases penalized unless the user typed all chars.
#[allow(dead_code)]
pub fn length_bias(_word_len: usize) -> f64 { 1.0 }
