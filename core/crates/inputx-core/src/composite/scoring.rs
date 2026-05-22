//! Centralized scoring model — the single source of truth for how
//! Inputx ranks candidates across wubi / pinyin / JP.
//!
//! # Why this module exists
//!
//! The runtime used to scatter scoring decisions across:
//!   - `inputx-wubi::dict.lookup_with_scores_into` (wubi layer × pref + freq)
//!   - `inputx-pinyin::dict.lookup_with_scores_into` (PINYIN_PHRASE_BASE + freq)
//!   - `composite::japanese_adapter::candidates_with_scores` (synthetic
//!      per-kind: 300k jukugo / 200k single-kanji / 100k hiragana / 90k katakana)
//!   - `composite::dispatch::dispatch` (ad-hoc policies: 5+ char wubi cut,
//!      cross-engine pin promotion)
//!   - `composite::merge::merge` (TC demote × 1e-3)
//!
//! Each came in as a patch for a specific symptom. The user (2026-05-22)
//! correctly observed: this isn't a *scoring system*, it's whack-a-mole.
//! This module consolidates the **constants** + the **decisions** that
//! collectively define the ranking. Code elsewhere references these
//! constants by name instead of hard-coding magic numbers.
//!
//! # Mental model
//!
//! Final candidate ranking is a **score sort with explicit rules**:
//!
//! 1. Each engine produces (word, score) per its own scoring formula. The
//!    formula is documented at each engine (see `LAYER_BASE` table in
//!    `inputx_wubi::layer`, `PINYIN_PHRASE_BASE` in
//!    `inputx_pinyin::dict`, the JP synthetic per-kind values below).
//!
//! 2. Score scales are calibrated so cross-engine sort produces the
//!    intended ranking. Jianma1 sits at 1e6+, JP single-kanji at 200k
//!    — there's a hierarchy implicit in the numbers.
//!
//! 3. Hard rules apply on top of score (this module + dispatch.rs):
//!     - **Wubi-first**: Inputx is 五笔, so valid wubi simcodes lead
//!       even when pinyin has the same letters as a valid syllable.
//!       This is mostly automatic via the wubi layer_base values —
//!       Jianma1=1M, Jianma2=800k, Jianma3=600k, all above pinyin's
//!       400k+freq ceiling. No demote logic; just the scoreboard.
//!     - **5+ char wubi cut** (`WUBI_MAX_BUFFER_LEN`): past 4 letters,
//!       the user is typing pinyin. Wubi's defuse-tail single chars
//!       (`jihua` defuses to `a` → 工 jianma1 1.04M) would crash #0
//!       otherwise. This is a CLIFF not a smooth curve, per the
//!       user's "超过 4 字就和五笔没关系了" rule.
//!     - **TC demote** (`TC_DEMOTE_MULTIPLIER`): traditional-Chinese
//!       chars × 1e-3 so they never beat their SC siblings.
//!     - **L0 pin** (`L0_PIN_MULTIPLIER`): user-pinned word × 1000 so
//!       it always tops merge.
//!     - **Wubi single-char-beats-phrase at full code** (in wubi/dict.rs):
//!       at length-4 input, if the only single-char freq > max-phrase
//!       freq among entries, promote × 100.
//!
//! 4. **No other hard rules.** If a ranking outcome surprises a user,
//!    the fix is either (a) corpus data, (b) one of the explicit
//!    multipliers below, or (c) a new explicitly-named rule. NOT a
//!    one-off `if` block in dispatch.

// ─── Engine score scale (informational; the actual scores come from
// each engine's `lookup_with_scores_into`) ─────────────────────────────

/// Wubi layer bases live in `inputx_wubi::layer::LAYER_BASE`. Documented
/// here for cross-engine context: Jianma1=1M, Jianma2=800k, Jianma3=600k,
/// Zigen=500k, Phrase=400k, Auto=100k×0.7=70k.
pub const WUBI_SCALE_NOTE: &str = "see inputx_wubi::layer::LAYER_BASE";

/// Pinyin Phrase base in `inputx_pinyin::dict::PINYIN_PHRASE_BASE` = 400k.
/// Pinyin freq is then added; top pinyin words land ~450k–500k.
pub const PINYIN_SCALE_NOTE: &str = "see inputx_pinyin::dict::PINYIN_PHRASE_BASE";

/// JP per-kind synthetic scores (in `japanese_adapter::candidates_with_scores`):
///   * Jukugo (multi-char kanji compound)  = 300_000
///   * Single-kanji (on/kun whole-buffer)  = 200_000
///   * Hiragana                            = 100_000
///   * Katakana                            =  90_000
/// Tuned to slot between wubi Auto (70k) and pinyin top (480k).
pub const JP_JUKUGO_SCORE: f64 = 300_000.0;
pub const JP_SINGLE_KANJI_SCORE: f64 = 200_000.0;
pub const JP_HIRAGANA_SCORE: f64 = 100_000.0;
pub const JP_KATAKANA_SCORE: f64 = 90_000.0;

// ─── Hard-rule multipliers ─────────────────────────────────────────────

/// Buffer length above which wubi is fully excluded from Mixed-mode
/// dispatch. User-stated: "超过 4 字就和五笔没关系了". Past this,
/// wubi's defuse-tail interpretations are mechanical noise and would
/// otherwise crash #0 via Jianma1 hard floor.
pub const WUBI_MAX_BUFFER_LEN: usize = 4;

/// Multiplier applied to candidates containing any traditional-Chinese
/// char (per OpenCC t2s table — 3549 chars). Demotes TC entries below
/// their SC siblings while still leaving them in the list if no SC
/// equivalent exists.
pub const TC_DEMOTE_MULTIPLIER: f64 = 1e-3;

/// Multiplier applied to L0-pinned words. Pin must dominate any
/// natural score for any source, including wubi Jianma1 (1.04M).
/// 1000 × pinyin Phrase top (~480k) = 480M, easily above Jianma1.
pub const L0_PIN_MULTIPLIER: f64 = 1000.0;

/// Wubi full-code-single-char-beats-phrase promote multiplier (applied
/// in `inputx_wubi::dict::lookup_with_scores_into`). At input length 4
/// (full wubi code), if the single-char candidate's freq exceeds the
/// max-phrase freq among entries at that code, promote × 100.
pub const WUBI_SINGLE_CHAR_PROMOTE_MULTIPLIER: f64 = 100.0;

/// "Above any Jianma1 candidate's natural score" floor. Used by
/// dispatch's cross-engine pin promotion path to detect whether a
/// pinned candidate needs an extra boost. Today: 1e6 = LAYER_BASE for
/// wubi Jianma1.
pub const JIANMA1_THRESHOLD: f64 = 1_000_000.0;

// ─── Future tunables (not yet wired) ───────────────────────────────────

/// Per-engine multiplier — would scale the raw scores from each
/// engine. Currently NOT applied (engines' raw scores are taken
/// as-is); adding this is the v0.3 path for fine-grained calibration.
#[allow(dead_code)]
pub const ENGINE_MULT_WUBI: f64 = 1.0;
#[allow(dead_code)]
pub const ENGINE_MULT_PINYIN: f64 = 1.0;
#[allow(dead_code)]
pub const ENGINE_MULT_JP: f64 = 1.0;

/// Length bias (not yet wired). Future: short phrases (2-3 char) get
/// a small boost since they dominate real typing; very long phrases
/// get a penalty unless explicitly typed.
#[allow(dead_code)]
pub fn length_bias(_word_len: usize) -> f64 { 1.0 }
