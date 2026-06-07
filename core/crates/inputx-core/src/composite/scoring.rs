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
//! # Probability framing (`P(W|i) = P(i|W) · P(W)`)
//!
//! All scoring constants below estimate one of the two factors of the
//! Bayesian decomposition. The naming convention reflects this:
//!
//! - **`PRIOR_*`** — factors of `P(W)`. Frequency-derived: word/char
//!   frequencies, user-pin overrides. "How common is W in the language /
//!   for this user."
//! - **`LIKELIHOOD_*`** — factors of `P(i|W)`. Match-type-derived: exact
//!   full-code, prefix proximity, simplified initials, fuzzy variants,
//!   composed Viterbi, traditional-character demote. "Given the user
//!   wanted W, how confidently does this match the typed `i`."
//! - **`CUTOFF_*`** — explicit truncation of the rendered output `o`.
//!   `P(W|i) ≈ 0` cases are dropped, not ranked.
//! - **`MARKER_*`** — diagnostic anchors / documentation values not used
//!   in ranking logic.
//!
//! See `.claude/PLAN-probabilistic-model.md` for the full framing.
//!
//! # Why Jianma1 (e→有, g→一, …) sits at #0
//!
//! Not because there's an `if Jianma1 { put_first() }` somewhere. Because
//! `inputx_wubi::layer::LAYER_BASE[Jianma1] = 1_000_000`. The score
//! formula for a wubi candidate is `layer.base() × pref + freq`, so a
//! Jianma1 candidate scores ~1.04M. Pinyin's top-phrase score ceiling
//! sits at ~480k. Sort descending → Jianma1 wins. No rule, just numbers.
//!
//! In the probability framing: `layer.base()` is the per-match-type
//! `LIKELIHOOD_*_BASE` for the wubi engine; the `+ freq` term is the
//! `PRIOR_FREQ_MULT * freq` contribution.
//!
//! # Why "5+ char input → wubi disappears"
//!
//! Not because dispatch.rs has `if buffer_len > 4 { skip_wubi() }`.
//! Because dispatch multiplies all wubi candidate scores by
//! `wubi_length_modifier(buffer_len)`, which is 1.0 inside the 4-char
//! window and 0.0 outside. Zero-scored candidates sort to the bottom
//! and get cut by `MAX_PER_INPUT`. The user sees "wubi gone past 4
//! chars" but the mechanism is pure scoring — `LIKELIHOOD_WUBI` falls
//! to 0 past the cutoff buffer length.
//!
//! # Score components, top to bottom
//!
//! Each candidate's final score is the product of these factors:
//!
//! 1. **Engine-internal base + freq**. See the LAYER_BASE table in
//!    `inputx_wubi::layer` (Jianma1=1M / Jianma2=800k / Jianma3=600k /
//!    Zigen=500k / Phrase=400k / Auto=70k), `PINYIN_PHRASE_BASE` in
//!    `inputx_pinyin::dict` (=400k), and the `LIKELIHOOD_JP_*_BASE`
//!    consts below.
//!
//! 2. **Multiplicative modifiers** applied at dispatch / merge / engine-
//!    internal time. Each is a real number; no special-case logic:
//!      - `wubi_length_modifier()` — 1.0 inside 4-char window, 0.0 beyond.
//!      - `LIKELIHOOD_TC_DEMOTE_MULT` (1e-3) — applied to candidates with
//!        any traditional-Chinese-only char.
//!      - `PRIOR_L0_PIN_MULT` (1000) — applied inside the engine's
//!        `lookup_with_scores_into` when the candidate matches a user pin.
//!      - `LIKELIHOOD_WUBI_SINGLE_CHAR_PROMOTE_MULT` (100) — applied inside
//!        `inputx_wubi::dict::lookup_with_scores_into` at full-code
//!        length, when the single-char freq exceeds max phrase freq.
//!
//! Adjust the constants here, watch the candidate list reorder.

// ─── LIKELIHOOD_JP_*_BASE: JP per-match-type base scores ────────────────
//
// Each JP candidate carries a per-entry `freq` (0-100 from the kanji /
// jukugo data tables). Final JP score
//     = LIKELIHOOD_JP_<TYPE>_BASE + PRIOR_FREQ_MULT_JP × freq
// is the additive split of likelihood (match-type base) and prior
// (frequency-scaled).
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

/// **LIKELIHOOD** — base for JP jukugo (multi-char kanji compound) match.
/// Below wubi/pinyin Phrase base (400k) so default ordering favors Chinese;
/// freq boost lets high-frequency jukugo (日本/今日/学校/会社) climb above
/// rare Chinese candidates.
pub const LIKELIHOOD_JP_JUKUGO_BASE: f64 = inputx_scoring::consts::LIKELIHOOD_JP_JUKUGO_BASE;

/// **LIKELIHOOD** — base for JP single-kanji match. Below jukugo (single
/// chars typically less specific than compounds), still below Chinese bases.
pub const LIKELIHOOD_JP_SINGLE_KANJI_BASE: f64 =
    inputx_scoring::consts::LIKELIHOOD_JP_SINGLE_KANJI_BASE;

/// **LIKELIHOOD** — base for JP hiragana (mechanical romaji→kana rendering).
/// Tuned 2026-05-24 from 200k → 150k after user-reported `di → ぢ #1
/// over 的`: at base 200k + freq 100·3000 = 500k, top hiragana beat
/// pinyin top 的 (465k). New target: top hiragana = 150k + 300k = 450k,
/// just under pinyin top, preserving user rule "JP top > Chinese rare,
/// JP top < Chinese top". `え` at 'e' (low freq) lands at 150k, still
/// visible mid-list (rank 3-6 typical), so the え-recovery regression
/// stays fixed without overpowering pinyin.
pub const LIKELIHOOD_JP_HIRAGANA_BASE: f64 = inputx_scoring::consts::LIKELIHOOD_JP_HIRAGANA_BASE;

/// **LIKELIHOOD** — base for JP katakana. Below hiragana (less common as
/// the default romaji rendering). Tuned 150k → 110k for the same reason
/// as hiragana: top katakana = 110k + 300k = 410k, comfortably under
/// pinyin top.
pub const LIKELIHOOD_JP_KATAKANA_BASE: f64 = inputx_scoring::consts::LIKELIHOOD_JP_KATAKANA_BASE;

/// **PRIOR** — multiplier on the per-entry freq value (P(W) scaling for
/// JP engine). Calibrated so top JP entries (freq 100) land at base +
/// 300k. Combined with the tuned bases above, top hiragana = 450k, top
/// katakana = 410k — both below pinyin top (~465k for common particles
/// like 的/了/是) while staying above pinyin rare (~410k+) so confident
/// JP picks aren't drowned out.
pub const PRIOR_FREQ_MULT_JP: f64 = inputx_scoring::consts::PRIOR_FREQ_MULT_JP;

/// **PRIOR** — multiplier on the per-entry freq value for the **pinyin
/// engine**. Pinyin dict entries already carry corpus-derived freq at a
/// scale where 1.0 multiplier is well-calibrated against the pinyin
/// Phrase base 400k (top pinyin words land near 400k + freq, see
/// `inputx_pinyin::PinyinDict::lookup_with_scores_into`). This factor
/// stays 1.0 unless cross-engine calibration says otherwise.
pub const PRIOR_FREQ_MULT_PINYIN: f64 = inputx_scoring::consts::PRIOR_FREQ_MULT_PINYIN;

/// **PRIOR** — multiplier on the per-entry freq value for the **wubi
/// engine** in prefix-prediction. Wubi raw freq (from the embedded FST)
/// already sits on a corpus-scaled axis where the bare value reads as a
/// score contribution; 1.0 keeps it. See `LIKELIHOOD_WUBI_PREDICT_BASE`
/// for the calibration of the additive floor.
pub const PRIOR_FREQ_MULT_WUBI: f64 = inputx_scoring::consts::PRIOR_FREQ_MULT_WUBI;

/// **LIKELIHOOD** — base for wubi prefix-prediction candidates (typed
/// buffer is a **prefix** of the candidate's full wubi code, not an exact
/// match). Per PLAN-prefix-prediction §3 (CP-C) — predictions must NOT
/// outrank a real exact wubi hit at the same buffer. Tuned 50_000.0:
/// below the lowest-confidence exact layer (Auto = 70k) so any exact
/// match leads its predictions; well above the score-0 cutoff so high-
/// freq predictions still surface mid-list. Combined with `proximity^K`
/// the actual delivered score for "3/8 of code typed" sits near base +
/// 0.05·freq; for "7/8 of code" near base + 0.67·freq — predictions rise
/// as the user types closer to the word.
///
/// Always-attached (no `has_non_speculative` gate, unlike pinyin Path 3):
/// since the exact wubi candidates carry layer-base scores in the
/// 70k-1M range and predictions max around base + freq, the math
/// naturally keeps an exact #0 (CP-A JP path uses the same pattern).
/// Predictions are subject to the same `wubi_length_modifier` cutoff,
/// so they silently vanish past `CUTOFF_WUBI_MAX_BUFFER_LEN` (4 chars).
/// Wired into `predict_score()` via `dispatch.rs`.
pub const LIKELIHOOD_WUBI_PREDICT_BASE: f64 = inputx_scoring::consts::LIKELIHOOD_WUBI_PREDICT_BASE;

/// **LIKELIHOOD** — base for pinyin prefix-prediction candidates
/// (typed buffer is a **prefix** of the candidate's full pinyin, not a
/// complete match). Per PLAN-prefix-prediction §4: must sit ABOVE the
/// non-exact-match floor (1k) so a high-freq predicted word like 中国
/// for `zho` surfaces visibly, but BELOW a real exact pinyin match
/// (Phrase base 400k) so an exact dict word always wins when both
/// exist. Combined with `proximity^K` damping, the actual delivered
/// score for "buffer is 3/8 of code" sits around base + freq·0.05; for
/// "7/8 of code" around base + freq·0.67 — predictions rise as the
/// user types closer to the word.
///
/// Must also sit BELOW:
///   - `FUZZY_BASE * FUZZY_DISCOUNT` (= 350k * 0.7 = 245k in
///     pinyin_adapter): a fuzzy match (`famin` → `faming` via in↔ing
///     swap) is a higher-confidence match than mid-typing prediction —
///     user typed a typo of an EXISTING word, vs typed a prefix toward
///     SOME word. Polish-log 2026-05-27 (`famin`): user reported
///     发明家 (prediction, base 250k + decay = 253k) outranking 发明
///     (fuzzy, 245k); fix is base lowered so prediction stays below
///     fuzzy across the proximity range.
///   - JP exact whole-buffer kana (`LIKELIHOOD_JP_HIRAGANA_BASE` +
///     full freq lift ≈ 240k for `fami` → ファミ): an exact JP match
///     for the typed buffer (proximity=1.0) is more confident than a
///     pinyin mid-typing prediction. Polish-log 2026-05-27 (`fami`):
///     pinyin predictions buried ファミ / ふぁみ.
///
/// Calibration: cap at 180k. With max freq ~50k and proximity^3 max 1.0,
/// peak prediction lands at 180k + 50k = 230k — comfortably below
/// fuzzy 245k and JP exact 240k. NON_EXACT_FLOOR (1k) remains well
/// below. Real Chinese exact (400k+) still leads.
///
/// Only applies when `allow_prefix_completion` fires
/// (`has_non_speculative_candidate == false`), so exact matches like
/// `lianxiang → 联想` are not affected (2026-05-22 user rule). Wired
/// into `predict_score()` below.
pub const LIKELIHOOD_PINYIN_PREDICT_BASE: f64 =
    inputx_scoring::consts::LIKELIHOOD_PINYIN_PREDICT_BASE;

/// **LIKELIHOOD** — JP full-match PROMOTE: multiplier applied to *every*
/// JP candidate's score when the buffer yields a real full-buffer 熟語
/// (multi-char kanji jukugo with freq > 0). A jukugo match means the
/// *entire* romaji buffer maps to a genuine Japanese word — a
/// high-confidence "the user is typing Japanese" signal (high P(i|W)),
/// analogous to a full-code exact wubi hit. User rule 2026-05-25
/// (shinjuku→新宿): in that case JP must take precedence over a
/// Chinese FORCED-composition fallback (the Viterbi 整句拼接 junk like
/// 是嗯据库 at LIKELIHOOD_JP_COMPOSED_BASE=130k after promote), and the
/// kana forms (esp. katakana, base 110k) must surface into the visible
/// window instead of drowning under pinyin non-exact noise (~244k).
/// Calibration: top jukugo 新宿 (464k) ×1.3 = 603k clears the 500k
/// composition; katakana シンジュク (200k) ×1.3 = 260k clears the pinyin
/// cluster. Only fires when a real jukugo is present, so plain pinyin
/// input (no jukugo) is untouched. Bounded so it only matters for long
/// romaji buffers (jukugo ≥ ~4 chars ⇒ wubi already zeroed by
/// wubi_length_modifier; no simcode collision).
pub const LIKELIHOOD_JP_FULL_MATCH_PROMOTE: f64 =
    inputx_scoring::consts::LIKELIHOOD_JP_FULL_MATCH_PROMOTE;

/// **LIKELIHOOD** — exponent on prefix-prediction proximity
/// (`typed_len / full_reading_len`). A predicted candidate's freq
/// contribution is scaled by `proximity^K`, so "almost done"
/// (proximity→1) keeps most of the freq while "just started" (low
/// proximity) is strongly damped — predictions rise as the user types
/// closer to the word. K=3 (草案): 0.875→0.67, 0.5→0.125, 0.375→0.05.
/// In the probability framing this IS `P(i|W)` for prefix matches.
/// Tune in CP-A calibration. See PLAN-prefix-prediction.md §4.
pub const LIKELIHOOD_PREDICT_PROXIMITY_K: f64 =
    inputx_scoring::consts::LIKELIHOOD_PREDICT_PROXIMITY_K;

/// **LIKELIHOOD** — base for `compose_sentence` products (mechanical
/// content+particle sentence guesses: 私は for watashiwa, but junk like
/// 時へ時 for the Chinese pinyin `jieji`). User-reported 2026-05-25:
/// these polluted the top of Chinese pinyin input (jieji/jieshou
/// surfaced 時へ時 / 治へ上 at #1-4, worsened by treating them as jukugo
/// + the full-match promote). They are LOW likelihood — set well below
/// the pinyin/wubi Phrase base (400k) so real Chinese words always lead,
/// while still letting a composed guess surface when there is NO Chinese
/// competition (watashiwa→私は). Never freq-scaled here and never
/// promoted (the freq of a mechanical compose is unreliable); a flat
/// floor keeps the whole compose group beneath real candidates. Above
/// katakana (110k) so a composed sentence still beats a bare mechanical
/// kana rendering.
pub const LIKELIHOOD_JP_COMPOSED_BASE: f64 = inputx_scoring::consts::LIKELIHOOD_JP_COMPOSED_BASE;

/// **LIKELIHOOD** — base for a PURE-KANJI compose product — specifically
/// the "jukugo + category-suffix kanji" path (東京+都 = 東京都, 大阪+府 =
/// 大阪府). User insight 2026-05-26: 東京都 is "拼" (productive 词+后缀),
/// not a dict word (mozc itself doesn't list it), so it's composed — but
/// unlike a 私は / 時へ時 particle-compose (which carries kana) a
/// pure-kanji admin compound is a high-confidence real reading the user
/// wants AS the kanji conversion. Set ABOVE the long-buffer kana
/// fallbacks (hiragana base 150k + kana_freq 30×3000 = 240k, katakana =
/// 200k) so the kanji conversion 東京都 leads the kana in Japanese mode
/// — the standard JP-IME workflow (type romaji, see kanji first, kana as
/// fallback). Kept BELOW real Chinese (400k) and never promoted, so it
/// can't pollute Chinese pinyin that happens to end in a suffix reading.
/// (It can edge a very-low-freq real jukugo, freq<27 → <280k; acceptable
/// — 東京都 is a legit reading.) Pure-kanji vs has-kana split is done by
/// inspecting the word in japanese_adapter (no extra field).
pub const LIKELIHOOD_JP_COMPOSED_KANJI_BASE: f64 =
    inputx_scoring::consts::LIKELIHOOD_JP_COMPOSED_KANJI_BASE;

/// **CUTOFF** — past this input length (pinyin-buffer chars), wubi
/// candidate scores get multiplied by 0.0 via `wubi_length_modifier`.
/// Effect: wubi vanishes from the user-visible list past 4 chars because
/// the user is clearly typing pinyin and wubi's defuse-tail
/// interpretations are mechanical noise (P(i|W) ≈ 0).
pub const CUTOFF_WUBI_MAX_BUFFER_LEN: usize = 4;

/// **LIKELIHOOD** — multiplier on candidates containing any
/// traditional-Chinese-only char (per OpenCC t2s map). Pulls TC variants
/// below their SC siblings while still leaving them in the list if no SC
/// equivalent exists. In the probability framing: when the user types
/// SC-friendly input, a TC candidate has much lower P(i|W) than its SC
/// sibling.
pub const LIKELIHOOD_TC_DEMOTE_MULT: f64 = inputx_scoring::consts::LIKELIHOOD_TC_DEMOTE_MULT;

/// **LIKELIHOOD** — wubi-first PROMOTE for a full-code (4-key) exact wubi
/// **Phrase** hit when the user is simultaneously typing a valid pinyin
/// word (`pinyin_intent`). User rule (2026-05-25, aiyi→东京): a
/// *complete* wubi code is a high-confidence wubi-first signal (high
/// P(i|W) for wubi engine) that should edge out a *same-freq* pinyin
/// word.
///
/// Tuned to ×1.1 (was 1.2). With Phrase base ~400k that's a ~40k
/// freq-equivalent edge — enough that a same-or-slightly-lower-freq wubi
/// phrase wins (aiyi: 东京 raw 429k already tops 爱意 425k, promote
/// widens it), but NOT enough to flip a *clearly* higher-freq pinyin
/// word. The 2026-05-26 jixu regression forced this down: 曳光弹 (rare
/// wubi 3-char coincidence, raw 407k) was beating 继续 (common pinyin,
/// raw 475k — 67k higher) because ×1.2 gave an 80k edge; ×1.1's 40k edge
/// keeps 继续 ahead while still honoring the aiyi same-freq case.
/// Bounded below the Zigen base (500k) so wubi-internal layering is
/// untouched. NOT applied to speculative short buffers (those keep the
/// 0.5 demote) nor to Auto junk.
pub const LIKELIHOOD_WUBI_FULL_CODE_PROMOTE: f64 =
    inputx_scoring::consts::LIKELIHOOD_WUBI_FULL_CODE_PROMOTE;

/// **PRIOR** — multiplier applied to L0-pinned words inside the engine's
/// `lookup_with_scores_into`. Brings any pin above any natural score:
/// Jianma1 (1.04M) × 1.0 = 1.04M; pinyin top (444k) × 1000 = 444M.
/// Pin wins. In the probability framing: a user pin asserts massive
/// P(W) for that user. (Constant kept for documentation + future
/// external callers; current pin path uses the literal `1000.0` in
/// dict.rs.)
#[allow(dead_code)]
pub const PRIOR_L0_PIN_MULT: f64 = 1000.0;

/// **LIKELIHOOD** — multiplier applied to a wubi single-char candidate
/// at full-code input length when its freq exceeds the max phrase freq
/// at the same code. P(i|W) for the single-char interpretation is
/// boosted because the typed exact code is more confidently the char
/// than the phrase. Documentation constant; promotion currently lives
/// inside wubi engine's own scoring.
#[allow(dead_code)]
pub const LIKELIHOOD_WUBI_SINGLE_CHAR_PROMOTE_MULT: f64 =
    inputx_scoring::consts::LIKELIHOOD_WUBI_SINGLE_CHAR_PROMOTE_MULT;

/// **MARKER** — diagnostic anchor recognizing "this is a wubi Jianma1
/// hit". Used by FFI / diagnostic code. Equals `LAYER_BASE[Jianma1]` in
/// inputx-wubi. Not a ranking factor itself.
#[allow(dead_code)]
pub const MARKER_WUBI_JIANMA1_BASE: f64 = 1_000_000.0;

/// **LIKELIHOOD** — score multiplier for wubi candidates given the
/// user's input length. Inside the 4-char window → 1.0 (wubi competes
/// normally). Outside → 0.0 (P(i|W) collapses; candidates sort bottom,
/// get cut by cap). Implementation of the cutoff governed by
/// `CUTOFF_WUBI_MAX_BUFFER_LEN`.
pub fn wubi_length_modifier(input_len: usize) -> f64 {
    if input_len <= CUTOFF_WUBI_MAX_BUFFER_LEN {
        1.0
    } else {
        0.0
    }
}

// ─── LIKELIHOOD_ENGINE_MULT_*: per-engine global multipliers ───────────

/// **LIKELIHOOD** — per-engine global multiplier. Would scale raw scores
/// per engine (cross-engine calibration). Currently NOT applied (each
/// engine's score taken as-is). v0.3+ lever for cross-engine
/// calibration if the (prior, likelihood) split needs an additional
/// per-engine confidence scale.
#[allow(dead_code)]
pub const LIKELIHOOD_ENGINE_MULT_WUBI: f64 = inputx_scoring::consts::LIKELIHOOD_ENGINE_MULT_WUBI;
#[allow(dead_code)]
pub const LIKELIHOOD_ENGINE_MULT_PINYIN: f64 =
    inputx_scoring::consts::LIKELIHOOD_ENGINE_MULT_PINYIN;
#[allow(dead_code)]
pub const LIKELIHOOD_ENGINE_MULT_JP: f64 = inputx_scoring::consts::LIKELIHOOD_ENGINE_MULT_JP;

/// **LIKELIHOOD** — length bias for **prefix-completion** ranking (bare
/// letter / partial syllable, e.g. `q`). At a single-syllable EXACT code
/// the dict already returns only single chars, so this never touches
/// those. But a bare prefix can complete to a single char OR a multi-char
/// phrase, and raw corpus freq buries common single chars (去/起) under
/// tech-corpus phrases (前端/前端工程师/企业微信). This multiplier favors
/// shorter candidates so single chars lead — while staying multiplicative,
/// so a phrase whose freq is high enough can still climb back (user rule
/// 2026-05-24: "单个字的评分肯定要更高", with the implicit "除非多字词频
/// 率远高"). 1.0 for a single char; sharp decay past that.
///
/// In the probability framing: for a bare-letter `i`, single-char W has
/// a higher `P(i|W)` than phrase W because the user "would have typed
/// more" for a phrase intent.
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

/// **LIKELIHOOD × PRIOR** — predicted-candidate score for a typed buffer
/// that is a **prefix** of the candidate's full code.
///
/// Decomposition (per `.claude/PLAN-probabilistic-model.md`):
///   `score(W) = base + prior(W) · likelihood(i, W)`
///       = base + (freq · freq_mult) · proximity^K
///
/// - `base` — match-type floor (e.g., `LIKELIHOOD_PINYIN_PREDICT_BASE`)
/// - `freq` — corpus / per-entry frequency (raw integer from the dict)
/// - `freq_mult` — engine-specific scaling (`PRIOR_FREQ_MULT_{PINYIN,JP}`)
/// - `proximity` — `len(typed) / len(full_code)`, ∈ (0, 1]; at 1.0 the
///   typed buffer fully matches the code (proximity^K = 1, max signal);
///   at 0.375 (3/8 of code typed) damped to 0.053.
/// - `LIKELIHOOD_PREDICT_PROXIMITY_K` (=3.0) — the exponent K. Higher K
///   damps short prefixes harder; K=3 chosen so "almost-complete"
///   (proximity 7/8) keeps ~67% signal while "just started" (3/8) keeps
///   ~5%.
///
/// Used by:
///   - pinyin `push_prefix_top_k` for Path 3 prefix-completion
///     (CP-B, v1.3 WU-α)
///   - JP `japanese_adapter::candidates_with_scores` for jukugo prefix
///     prediction (CP-A, v1.2)
///   - wubi prefix-prediction (CP-C, v1.3 WU-α, planned)
#[allow(dead_code)] // documented simple-form API; predict_score_decomposed is the wired variant
pub fn predict_score(base: f64, freq: u64, freq_mult: f64, proximity: f64) -> f64 {
    base + (freq as f64) * freq_mult * proximity.powf(LIKELIHOOD_PREDICT_PROXIMITY_K)
}

/// Variant of [`predict_score`] that also returns the two-axis
/// decomposition (`base`, `prior = freq · freq_mult`,
/// `likelihood = proximity^K`). Invariant:
///   `score == base + prior · likelihood`
/// holds bit-for-bit (`base + prior * likelihood` is the exact same
/// expression as in `predict_score`'s body, just split). The
/// composite layer surfaces these via `Candidate.components` so
/// `inputx-probe` can render the (base, prior, likelihood) view (v1.3
/// WU-γ) without disturbing the underlying score.
///
/// Used by every `predict_score` caller (CP-A JP / CP-B pinyin / CP-C
/// wubi prediction) — they now keep both the score and its components.
pub fn predict_score_with_components(
    base: f64,
    freq: u64,
    freq_mult: f64,
    proximity: f64,
    corpus_total: u64,
) -> (f64, crate::composite::merge::ScoreComponents) {
    let prior = (freq as f64) * freq_mult;
    let likelihood = proximity.powf(LIKELIHOOD_PREDICT_PROXIMITY_K);
    let score = base + prior * likelihood;
    // v1.4.2 WU-γ three-axis log-space view of the same chain. Producers
    // emit MatchType::Prefix(proximity_milli); proximity=1.0 collapses
    // to MatchType::Exact-equivalent (decay = 0). `freq` drives the
    // log_prior; `base` (already a positive number in linear space, the
    // per-engine LIKELIHOOD_*_BASE) becomes the log_likelihood floor.
    //
    // v1.7.4: log_prior is a real log-probability against the calling
    // engine's corpus total — the caller knows its engine and passes
    // the right `corpus_total` (e.g. `wubi_corpus_total()` from
    // `inputx-wubi-data`, `pinyin_corpus_total()` from
    // `inputx-pinyin-helpers`, `nihongo_jukugo_corpus_total()` /
    // `nihongo_kanji_corpus_total()` for JP).
    let log_prior_q4 = inputx_scoring::log_prob_corpus_from_freq(freq, corpus_total);
    let base_log_q4 = (base.max(1.0).ln() * inputx_scoring::Q4 as f64).round() as i32;
    let prox_milli = (proximity.clamp(0.0, 1.0) * 1000.0).round() as u16;
    let match_type = inputx_scoring::MatchType::Prefix(prox_milli);
    let log_likelihood_q4 = inputx_scoring::derive_log_likelihood(base_log_q4, match_type);
    (
        score,
        crate::composite::merge::ScoreComponents::from_predict(
            base,
            prior,
            likelihood,
            log_prior_q4,
            log_likelihood_q4,
            match_type,
        ),
    )
}

#[cfg(test)]
mod tests {
    //! Manifest tests for the v1.3 PRIOR/LIKELIHOOD/CUTOFF rename.
    //!
    //! These pin the *values* of each constant so the rename pass can be
    //! verified byte-for-byte: any drift requires touching this file
    //! deliberately. The full byte-for-byte behavior invariant comes from
    //! the 265 baseline / dispatch / engine tests asserting specific
    //! (word, score) tuples — this module is the explicit attestation.
    //!
    //! See `.claude/PLAN-v1.3.md` WU-β and `.claude/PLAN-probabilistic-model.md`
    //! for the probability framing (`P(W|i) = P(i|W) · P(W)`).
    use super::*;

    #[test]
    fn prior_factors_match_v1_2_values() {
        // P(W) — frequency-derived priors. Unchanged across the v1.3 rename.
        assert_eq!(PRIOR_FREQ_MULT_JP, 3000.0);
        assert_eq!(PRIOR_L0_PIN_MULT, 1000.0);
    }

    #[test]
    fn wug_predict_score_components_invariant() {
        // v1.3 WU-γ invariant: for every (base, freq, freq_mult, proximity)
        // accepted by `predict_score`, `predict_score_with_components` must
        // return a `ScoreComponents` whose `base + prior * likelihood`
        // reproduces the score bit-for-bit (modulo IEEE rounding within an
        // epsilon). This is the only thing that keeps the probe's three-
        // axis decomposition honest — without this test a future refactor
        // could silently desync the two helpers.
        let cases: &[(f64, u64, f64, f64)] = &[
            (
                LIKELIHOOD_PINYIN_PREDICT_BASE,
                50_000,
                PRIOR_FREQ_MULT_PINYIN,
                0.875,
            ),
            (
                LIKELIHOOD_PINYIN_PREDICT_BASE,
                0,
                PRIOR_FREQ_MULT_PINYIN,
                1.0,
            ),
            (
                LIKELIHOOD_WUBI_PREDICT_BASE,
                45_000,
                PRIOR_FREQ_MULT_WUBI,
                0.5,
            ),
            (LIKELIHOOD_JP_JUKUGO_BASE, 88, PRIOR_FREQ_MULT_JP, 0.875),
            (LIKELIHOOD_JP_JUKUGO_BASE, 100, PRIOR_FREQ_MULT_JP, 1.0),
        ];
        for &(base, freq, mult, proximity) in cases {
            let plain = predict_score(base, freq, mult, proximity);
            // Dummy corpus_total — the linear-space `score` and the
            // (base, prior, likelihood) invariant are independent of
            // the v1.7.4 `log_prob_corpus` shift; the log-space axes
            // shift uniformly per engine but the linear chain is
            // unchanged.
            let (split, c) = predict_score_with_components(base, freq, mult, proximity, 1_000_000);
            assert_eq!(
                plain, split,
                "predict_score and _with_components must agree for ({base}, {freq}, {mult}, {proximity})"
            );
            let recomputed = c.base + c.prior * c.likelihood;
            assert!(
                (split - recomputed).abs() < 1e-9,
                "invariant breaks for ({base}, {freq}, {mult}, {proximity}): \
                 score={split}, base+prior*likelihood={recomputed}"
            );
        }
    }

    #[test]
    fn predict_factors_match_v1_3_values() {
        // v1.3 prefix-prediction constants (WU-α CP-B/C). PINYIN already
        // pinned by CP-B; CP-C adds WUBI alongside.
        assert_eq!(PRIOR_FREQ_MULT_PINYIN, 1.0);
        assert_eq!(PRIOR_FREQ_MULT_WUBI, 1.0);
        assert_eq!(LIKELIHOOD_PINYIN_PREDICT_BASE, 180_000.0);
        assert_eq!(LIKELIHOOD_WUBI_PREDICT_BASE, 50_000.0);
    }

    #[test]
    fn likelihood_factors_match_v1_2_values() {
        // P(i|W) — match-type confidence factors. Unchanged across the rename.
        // JP match-type bases (additive component of LIKELIHOOD × match floor):
        assert_eq!(LIKELIHOOD_JP_JUKUGO_BASE, 200_000.0);
        assert_eq!(LIKELIHOOD_JP_SINGLE_KANJI_BASE, 100_000.0);
        assert_eq!(LIKELIHOOD_JP_HIRAGANA_BASE, 150_000.0);
        assert_eq!(LIKELIHOOD_JP_KATAKANA_BASE, 110_000.0);
        assert_eq!(LIKELIHOOD_JP_COMPOSED_BASE, 130_000.0);
        assert_eq!(LIKELIHOOD_JP_COMPOSED_KANJI_BASE, 280_000.0);
        // Multiplicative confidence boosts / damps:
        assert_eq!(LIKELIHOOD_JP_FULL_MATCH_PROMOTE, 1.3);
        assert_eq!(LIKELIHOOD_WUBI_FULL_CODE_PROMOTE, 1.1);
        assert_eq!(LIKELIHOOD_TC_DEMOTE_MULT, 1e-3);
        assert_eq!(LIKELIHOOD_WUBI_SINGLE_CHAR_PROMOTE_MULT, 100.0);
        assert_eq!(LIKELIHOOD_PREDICT_PROXIMITY_K, 3.0);
        // Per-engine cross-calibration scales (currently 1.0 = no-op):
        assert_eq!(LIKELIHOOD_ENGINE_MULT_WUBI, 1.0);
        assert_eq!(LIKELIHOOD_ENGINE_MULT_PINYIN, 1.0);
        assert_eq!(LIKELIHOOD_ENGINE_MULT_JP, 1.0);
    }

    #[test]
    fn cutoff_factors_match_v1_2_values() {
        // P(W|i) ≈ 0 cutoffs. Unchanged across the rename.
        assert_eq!(CUTOFF_WUBI_MAX_BUFFER_LEN, 4);
    }

    #[test]
    fn marker_factors_match_v1_2_values() {
        // Diagnostic anchors — not ranking factors.
        assert_eq!(MARKER_WUBI_JIANMA1_BASE, 1_000_000.0);
    }

    #[test]
    fn wubi_length_modifier_matches_v1_2_shape() {
        // Cutoff function: 1.0 inside the buffer-length window, 0.0 beyond.
        assert_eq!(wubi_length_modifier(0), 1.0);
        assert_eq!(wubi_length_modifier(4), 1.0);
        assert_eq!(wubi_length_modifier(5), 0.0);
        assert_eq!(wubi_length_modifier(10), 0.0);
    }

    #[test]
    fn length_bias_matches_v1_2_shape() {
        // Likelihood length-bias function for single-letter prefix.
        assert_eq!(length_bias(0), 1.0);
        assert_eq!(length_bias(1), 1.0);
        assert_eq!(length_bias(2), 0.18);
        assert_eq!(length_bias(3), 0.10);
        assert_eq!(length_bias(4), 0.07);
        assert_eq!(length_bias(99), 0.05);
    }
}
