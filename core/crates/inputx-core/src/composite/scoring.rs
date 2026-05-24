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

// ─── JP base scores ──────────────────────────────────────────────────────
//
// Each JP candidate carries a per-entry `freq` (0-100 from the kanji /
// jukugo data tables). Final JP score = base + JP_FREQ_MULTIPLIER × freq.
//
// User-stated design rule (2026-05-23):
//   * "日语基础权重分就不如五笔和拼音高" — JP base < wubi/pinyin base.
//     Wubi Phrase base = 400k, pinyin Phrase base = 400k. JP bases sit
//     BELOW these.
//   * "日语高频要高于中文难检字和生僻词组" — JP high-freq must beat
//     Chinese rare entries (wubi Auto ~70k, pinyin rare ~410k).
//
// Calibration: top JP jukugo (freq 100) lands at 200k + 100·3000 = 500k.
// Mid-freq (50) at 350k. Low (10) at 230k. Top JP single-kanji (freq 100)
// at 400k. The numbers can be tuned here; runtime updates immediately.

/// JP jukugo (multi-char kanji compound) base score. Below wubi/pinyin
/// Phrase base (400k) so default ordering favors Chinese; freq boost
/// lets high-frequency jukugo (日本/今日/学校/会社) climb above rare
/// Chinese candidates.
pub const JP_JUKUGO_SCORE: f64 = 200_000.0;

/// JP single-kanji base score. Below jukugo (single chars typically
/// less specific than compounds), still below Chinese bases.
pub const JP_SINGLE_KANJI_SCORE: f64 = 100_000.0;

/// JP hiragana base — mechanical romaji→kana rendering.
/// Tuned 2026-05-24 from 200k → 150k after user-reported `di → ぢ #1
/// over 的`: at base 200k + freq 100·3000 = 500k, top hiragana beat
/// pinyin top 的 (465k). New target: top hiragana = 150k + 300k = 450k,
/// just under pinyin top, preserving user rule "JP top > Chinese rare,
/// JP top < Chinese top". `え` at 'e' (low freq) lands at 150k, still
/// visible mid-list (rank 3-6 typical), so the え-recovery regression
/// stays fixed without overpowering pinyin.
pub const JP_HIRAGANA_SCORE: f64 = 150_000.0;

/// JP katakana base — below hiragana (less common as the default romaji
/// rendering). Tuned 150k → 110k for the same reason as hiragana:
/// top katakana = 110k + 300k = 410k, comfortably under pinyin top.
pub const JP_KATAKANA_SCORE: f64 = 110_000.0;

/// Multiplier on the per-entry freq value. Calibrated so top JP entries
/// (freq 100) land at base + 300k. Combined with the tuned bases above,
/// top hiragana = 450k, top katakana = 410k — both below pinyin top
/// (~465k for common particles like 的/了/是) while staying above
/// pinyin rare (~410k+) so confident JP picks aren't drowned out.
pub const JP_FREQ_MULTIPLIER: f64 = 3000.0;

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
/// Pin wins. (Constant kept for documentation + future external
/// callers; current pin path uses the literal `1000.0` in dict.rs.)
#[allow(dead_code)]
pub const L0_PIN_MULTIPLIER: f64 = 1000.0;

/// Multiplier applied to a wubi single-char candidate at full-code
/// input length when its freq exceeds the max phrase freq at the same
/// code. Documentation constant; promotion currently lives inside
/// wubi engine's own scoring.
#[allow(dead_code)]
pub const WUBI_SINGLE_CHAR_PROMOTE_MULTIPLIER: f64 = 100.0;

/// Score floor recognizing "this is a wubi Jianma1 hit". Used by
/// diagnostic / FFI code. Today = LAYER_BASE[Jianma1] in inputx-wubi.
#[allow(dead_code)]
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

/// Length bias for **prefix-completion** ranking (bare letter / partial
/// syllable, e.g. `q`). At a single-syllable EXACT code the dict already
/// returns only single chars, so this never touches those. But a bare
/// prefix can complete to a single char OR a multi-char phrase, and raw
/// corpus freq buries common single chars (去/起) under tech-corpus
/// phrases (前端/前端工程师/企业微信). This multiplier favors shorter
/// candidates so single chars lead — while staying multiplicative, so a
/// phrase whose freq is high enough can still climb back (user rule
/// 2026-05-24: "单个字的评分肯定要更高", with the implicit "除非多字词频
/// 率远高"). 1.0 for a single char; sharp decay past that.
///
/// Scoped deliberately to `compute_single_letter_top_k` only — NOT to
/// multi-letter prefix completion (`zho` → 中国), where the user is
/// mid-syllable toward a phrase and phrases are the desired result.
pub fn length_bias(word_len: usize) -> f64 {
    match word_len {
        0 | 1 => 1.0,
        2 => 0.18,
        3 => 0.10,
        4 => 0.07,
        _ => 0.05,
    }
}
