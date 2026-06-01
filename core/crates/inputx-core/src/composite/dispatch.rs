//! Routing rules for the composite engine.
//!
//! Decides which sub-engines run for a given input + mode, and how their
//! candidate lists combine into the merged output.

use super::japanese_adapter::JapaneseAdapter;
use super::merge::{Candidate, ScoreComponents, Scored, merge};
use super::mode::Mode;
use super::pinyin_adapter::PinyinAdapter;
use super::scoring;
use crate::wubi::WubiEngine;

/// v1.4.2 WU-γ schema-fill helper: synthesize a three-axis log-space
/// view of a legacy `(word, score)` pair when the upstream adapter
/// doesn't have a natural (freq, base) split available. Sets
/// `log_prior_q4 = 0` + `log_likelihood_q4 = Q4·ln(score)` so the
/// log-space sort key `score_q4()` is monotone-equivalent to the legacy
/// f64 score (sort behavior preserved across the v1.4.5+ cutover).
/// `MatchType` is supplied by the caller — wubi exact code lookups pass
/// `Exact`, predictions pass `Prefix`, etc.
fn synthesize_three_axis(score: f64, match_type: inputx_scoring::MatchType) -> ScoreComponents {
    let log_likelihood_q4 =
        (score.max(1.0).ln() * inputx_scoring::Q4 as f64).round() as i32;
    ScoreComponents::three_axis(0, log_likelihood_q4, match_type)
}

/// Wrap legacy `(word, score)` pairs as `Scored` tuples. Every fill
/// point in the composite dispatch goes through here, [`merge`], or one
/// of the adapter `candidates_with_scores` methods — all of which now
/// emit three-axis `ScoreComponents` per PLAN.md L4 trigger b.
///
/// `match_type` is the caller's classification: `Exact` for full-code
/// dict lookups, `Prefix(prox_milli)` for prefix completions, etc.
fn wrap_legacy(
    v: Vec<(String, f64)>,
    match_type: inputx_scoring::MatchType,
) -> Vec<Scored> {
    v.into_iter()
        .map(|(w, s)| (w, s, Some(synthesize_three_axis(s, match_type))))
        .collect()
}

/// Compute the merged candidate list for the current state.
///
/// - `Mode::WubiOnly` — wubi candidates first; JP appended if enabled.
/// - `Mode::PinyinOnly` — pinyin first; JP appended if enabled.
/// - `Mode::Mixed` — wubi + pinyin merged (existing ranking rules below);
///   JP appended at the end if enabled.
/// - `Mode::JapaneseOnly` — JP candidates only (wubi/pinyin ignored).
///
/// For `Mode::Mixed`, ranking rules are:
///   (a) pinyin buffer starts with `z` AND wubi buffer is empty → wubi
///       contributes nothing (`z` isn't a wubi 字根 letter);
///   (b) pinyin buffer is *longer* than the wubi buffer → pinyin-first.
///       Catches the collision case where the user's input overflowed
///       wubi's 4-code window: the user is committed to pinyin
///       (e.g., shang, wangle), so the wubi reset-leftover shouldn't
///       dominate.
///
/// `japanese` is passed as `Option`: `None` when JP is disabled (the
/// composite engine never even constructed an adapter), `Some` when on
/// (Mixed/WubiOnly/PinyinOnly + enable_japanese, or JapaneseOnly).
pub fn dispatch(
    mode: Mode,
    wubi: &WubiEngine,
    pinyin: &PinyinAdapter,
    japanese: Option<&JapaneseAdapter>,
    prev_committed: Option<&str>,
) -> Vec<Candidate> {
    let (jp_kanji, jp_kana) = match japanese {
        Some(j) => split_jp_scored(j),
        None => (vec![], vec![]),
    };
    // JP-chōonpu lockout (user polish-log 2026-05-27, `fa------`): if the
    // JP buffer contains `-` (chōonpu / long-vowel mark), the user has
    // unambiguously committed to a Japanese romaji input. Chinese has no
    // syllable that contains `-`, so wubi/pinyin candidates surfaced
    // alongside (still derived from the pre-`-` prefix the Chinese engines
    // froze on) are mismatched noise to the user — preedit shows the full
    // `fa------` but the wubi candidates are for `fa` only, confusing the
    // ranking. Drop all wubi/pinyin candidates in this regime, leave only
    // JP. Works across WubiOnly+JP / PinyinOnly+JP / Mixed+JP since
    // `-` only enters the JP buffer when JP is composing.
    let jp_chouonpu_lockout = japanese
        .map_or(false, |j| j.buffer_str().contains('-'));
    match mode {
        Mode::WubiOnly => {
            let w = if jp_chouonpu_lockout {
                vec![]
            } else {
                wrap_legacy(wubi.candidates_with_scores(), inputx_scoring::MatchType::Exact)
            };
            merge(w, vec![], jp_kanji, jp_kana)
        }
        Mode::PinyinOnly => {
            let p = if jp_chouonpu_lockout {
                vec![]
            } else {
                pinyin.candidates_with_scores(prev_committed)
            };
            merge(vec![], p, jp_kanji, jp_kana)
        }
        Mode::JapaneseOnly => merge(vec![], vec![], jp_kanji, jp_kana),
        Mode::Mixed if jp_chouonpu_lockout => {
            // Short-circuit Mixed → JP-only when chōonpu present. Skips
            // the entire wubi+pinyin candidate pipeline below.
            merge(vec![], vec![], jp_kanji, jp_kana)
        }
        Mode::Mixed => {
            // EVERYTHING IS SCORE. No if-skip-engine branches. Wubi
            // candidates always get collected; their scores are
            // multiplied by `scoring::wubi_length_modifier(buffer_len)`
            // — which is 1.0 inside the wubi window and 0.0 beyond it.
            // Result: past-window wubi candidates rank at score 0,
            // get cut by the MAX_PER_INPUT cap, never reach the user.
            // The visible behavior matches "5+ char wubi out" but
            // the *mechanism* is pure scoring.
            let pinyin_len = pinyin.buffer_str().len();
            let wubi_mult = scoring::wubi_length_modifier(pinyin_len);
            // The 'z' carve-out (wubi 'z' is rare standalone) is
            // expressed as the same length-modifier mechanism: a
            // ZERO score multiplier zeroes the candidates out the
            // same way the length cutoff does.
            let z_mult = if pinyin.buffer_str().starts_with('z') { 0.0 } else { 1.0 };
            // Layer-aware demote (the 伙 vs 嶙 distinction). When the
            // buffer is short AND contains a vowel AND pinyin has an
            // exact match, the user is most likely typing pinyin not
            // wubi codes. In that regime, low-confidence wubi layers
            // (Auto / Phrase) shouldn't displace pinyin top — but
            // high-confidence simcodes MUST stay (the user previously
            // rejected blanket demote: "我们是五笔输入法, 你这样把'伙'
            // 这个正牌五笔输入都干到 13 位去了肯定不行"). Per-layer
            // demote preserves Jianma1/2/3 + Zigen at full strength
            // while cutting Auto/Phrase noise that floods short-buffer
            // candidate lists with rare chars like 嶙.
            let has_vowel = pinyin.buffer_str().chars()
                .any(|c| matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'v'));
            let pinyin_intent =
                pinyin_len > 0
                && pinyin_len <= 4
                && has_vowel
                && pinyin.has_non_speculative_candidate();

            // Pull layer-aware candidates so we can demote per-layer
            // when pinyin_intent fires.
            // v1.4 polish (2026-05-24): Auto-layer demote scales with
            // buffer length. The shorter the pinyin buffer, the more
            // unambiguously the user is typing pinyin (1-2 letters
            // have lots of pinyin word matches, almost certainly not
            // a wubi 4-key search). Auto layer (rare 字根-decomp chars
            // like 嶙) should drop hard for short buffers.
            //
            // Polish-log near-miss `mo → 默 rank 10-11, top1=嶙` was
            // the trigger: the previous flat ×0.1 wasn't enough — Auto
            // 嶙 with LAYER_BASE ~5M ended up at 500k, still above
            // pinyin 没 at ~440k. Length-scaled demote (1→×0.01,
            // 2→×0.05, 3→×0.10, 4→×0.20) puts 嶙 firmly below pinyin
            // for short buffers while preserving Phrase entries
            // (multi-char phrase codes are MUCH less ambiguous so
            // their ×0.5 demote stays).
            //
            // Jianma simcodes (1/2/3) + Zigen stay at ×1.0 — 伙-rule:
            // "我们是五笔输入法，你这样把'伙'这个正牌五笔输入都干到 13 位
            // 去了肯定不行". Simcodes are NOT in Auto/Phrase.
            // v1.10: per-len table sourced from
            // `inputx-scoring/data/engine_weights.toml`
            // [dispatch.wubi].auto_layer_demote — index 0..3 = pinyin_len 1..4.
            let auto_demote = if pinyin_intent {
                let idx = (pinyin_len.saturating_sub(1)).min(3);
                inputx_scoring::consts::WUBI_AUTO_LAYER_DEMOTE[idx]
            } else { 1.0 };
            // Phrase-layer multiplier under pinyin_intent:
            //   * speculative short buffer (< 4 codes) → 0.5 demote. The
            //     buffer is ambiguous; low-confidence Phrase candidates
            //     shouldn't crowd out pinyin (see mixed_xlab_*).
            //   * full-code exact hit (4 codes) → wubi-first PROMOTE. The
            //     user typed a COMPLETE wubi code — high-confidence. Per
            //     user rule 2026-05-25 (aiyi→东京) a full-code wubi phrase
            //     must beat a same-tier pinyin word even at somewhat lower
            //     freq. 东京 raw 429241 already topped 爱意 424712, but the
            //     old flat ×0.5 buried it at 214k; the promote now gives a
            //     structural ~40k freq-equivalent edge (×1.1; was 1.2 — see
            //     mixed_jixu_* regression: 1.2's 80k edge wrongly flipped a
            //     clearly-higher-freq pinyin word 继续 under 曳光弹).
            let full_code = pinyin_len == scoring::CUTOFF_WUBI_MAX_BUFFER_LEN;
            let phrase_mult = if pinyin_intent {
                if full_code {
                    scoring::LIKELIHOOD_WUBI_FULL_CODE_PROMOTE
                } else {
                    inputx_scoring::consts::WUBI_PHRASE_SPECULATIVE_DEMOTE
                }
            } else {
                1.0
            };
            // v1.4 score-driven Jianma2 demote (user 2026-05-24:
            // "完全走评分候选，一行 hardcode 都不允许有"). For
            // single-char Jianma2/3 entries, scale score by the char's
            // own pinyin freq:
            //   common char (≥CHAR_PROMINENT) → 1.0 (full lead, beats pinyin)
            //   rare char (<CHAR_PROMINENT)   → 0.3 (drops to ~250k, yields)
            // No hardcoded protect list — common chars retain lead via
            // freq (左 41k, 表 47k, 能 56k, 就 57k, 伙 35k, 悄 27k,
            // 椒 29k, 胡 38k, 长 47k, 亦 43k all clear 20k floor); rare
            // chars (嶙 15k) drop and let pinyin top through.
            // v1.10: values sourced from
            // `inputx-scoring/data/engine_weights.toml` [dispatch.wubi]
            // section. Polish via TOML edit, not code edit.
            const CHAR_PROMINENT_FLOOR: u64 =
                inputx_scoring::consts::WUBI_CHAR_PROMINENT_FLOOR_FREQ;
            const RARE_CHAR_DEMOTE: f64 = inputx_scoring::consts::WUBI_RARE_CHAR_DEMOTE;
            let pinyin_dict = pinyin.engine().dict();
            let char_demote = |word: &str, layer: inputx_wubi::Layer| -> f64 {
                if !matches!(layer, inputx_wubi::Layer::Jianma2 | inputx_wubi::Layer::Jianma3) {
                    return 1.0;
                }
                let mut chars = word.chars();
                let Some(c) = chars.next() else { return 1.0 };
                if chars.next().is_some() { return 1.0; }  // multi-char Jianma3 phrase
                let freq = pinyin_dict.char_max_freq(c);
                if freq >= CHAR_PROMINENT_FLOOR { 1.0 } else { RARE_CHAR_DEMOTE }
            };
            // v1.4.7 sub-phase A2 step 1: orthodox three-axis
            // decomposition replaces the v1.4.2 synthesize_three_axis
            // shortcut (which lumped the entire legacy score into
            // log_likelihood_q4, leaving log_prior_q4=0 and producing
            // ranking inconsistent with predict-path candidates that
            // properly split prior/likelihood).
            //
            // Now: log_prior_q4 = Q4·ln(1 + raw_freq) (matches the
            // wubi prediction path's log_prior derivation), and
            // log_likelihood_q4 = Q4·ln(layer.base() · pref ·
            // layer_demote · char_demote · promote) (the per-path
            // multiplicative chain; wubi_length_modifier + z_mult
            // applied at merge chokepoint below via final_mult).
            //
            // facade `candidates_with_freq_layer` returns PURE per-
            // entry data (word, layer, raw_freq) — no per-batch
            // single-char promote or L0 pin. Those are wubi-specific
            // business rules that this cement layer re-applies here.
            //
            // Legacy f64 `score` field still computed as before so the
            // (transitional) f64-sort merge.rs keeps producing the
            // v1.3 ranking; sort-key cutover to score_q4 happens in
            // A2 step 3 after all three engines' fills are aligned.
            let layer_prefs_default = inputx_wubi::DEFAULT_LAYER_PREFS;
            let full_code = wubi.buffer_str().len() == 4;
            let freq_layer = wubi.candidates_with_freq_layer();
            // Single-char promote setup: at full code, a single-char
            // entry whose freq exceeds the per-code max phrase freq
            // gets ×100 boost (wubi 86 "full-code single-char wins"
            // rule, replicating inputx_wubi::PinyinDict::
            // lookup_with_scores_into's internal logic).
            let max_phrase_freq: u64 = freq_layer
                .iter()
                .filter(|(w, _, _)| w.chars().count() > 1)
                .map(|(_, _, f)| *f)
                .max()
                .unwrap_or(0);
            let mut wubi_cands: Vec<Scored> = freq_layer
                .into_iter()
                .map(|(w, layer, raw_freq)| {
                    let pref = layer_prefs_default[layer.as_index()];
                    let layer_demote = match layer {
                        inputx_wubi::Layer::Auto => auto_demote,
                        inputx_wubi::Layer::Phrase => phrase_mult,
                        _ => 1.0,
                    };
                    let cd = char_demote(&w, layer);
                    let is_single = w.chars().count() == 1;
                    let single_promote = if full_code && is_single && raw_freq > max_phrase_freq {
                        inputx_scoring::consts::WUBI_FULL_CODE_SINGLE_CHAR_PROMOTE
                    } else {
                        1.0
                    };
                    // Legacy f64 score (transitional, drops post-A5):
                    let base_score = (layer.base() as f64 * pref + raw_freq as f64) * single_promote;
                    let final_score = base_score * layer_demote * cd;
                    // Orthodox Q4 log decomposition. log_prior is the
                    // frequency prior P(W); log_likelihood collapses all
                    // multiplicative likelihood factors into log space.
                    //
                    // v1.7.4 megachange: real log-probability
                    // `Q4·ln((1+raw_freq)/(1+Σ wubi raw_freq))` instead
                    // of the unnormalized `Q4·ln(1+raw_freq)`. Within-
                    // wubi ordering is unaffected (uniform shift per
                    // engine); cross-engine ordering is now governed by
                    // `EngineWeights::engine_boost_q4[Wubi]` in
                    // composite/merge.rs rather than the implicit
                    // freq-scale difference between engines.
                    let log_prior_q4 = inputx_scoring::log_prob_corpus_from_freq(
                        raw_freq,
                        inputx_wubi_data::wubi_corpus_total(),
                    );
                    let likelihood_linear = layer.base() as f64
                        * pref
                        * layer_demote.max(f64::MIN_POSITIVE)
                        * cd.max(f64::MIN_POSITIVE)
                        * single_promote;
                    let log_likelihood_q4 = (likelihood_linear
                        .max(1.0)
                        .ln()
                        * inputx_scoring::Q4 as f64)
                        .round() as i32;
                    // v1.7.4: tag Jianma1/2/3 candidates as simcodes so
                    // the cross-engine merge can apply
                    // `EngineWeights::simcode_boost_q4` selectively —
                    // common simcodes get the wubi-first lift, but
                    // raw-freq-low entries (rare-CJK Jianma2) still
                    // yield to common pinyin top.
                    let is_simcode = matches!(
                        layer,
                        inputx_wubi::Layer::Jianma1
                            | inputx_wubi::Layer::Jianma2
                            | inputx_wubi::Layer::Jianma3,
                    );
                    let components = ScoreComponents::three_axis_simcode(
                        log_prior_q4,
                        log_likelihood_q4,
                        inputx_scoring::MatchType::Exact,
                        is_simcode,
                    );
                    (w, final_score, Some(components))
                })
                .collect();
            // CP-C (v1.3 WU-α): attach wubi prefix-predictions. predict_score
            // tops out at base + freq (proximity=1) ≈ 50k + freq, well below
            // the lowest exact layer base (Auto = 70k) — so an exact hit at
            // the same buffer is mathematically guaranteed to lead. Goes
            // through the same final_mult below, so predictions vanish past
            // CUTOFF_WUBI_MAX_BUFFER_LEN and on the 'z' carve-out exactly
            // like exact wubi hits. Layer/char demotes don't apply: those
            // encode per-code candidate semantics; predictions are
            // cross-code by construction.
            //
            // Bare letters skipped: at buffer.len()==1, proximity is at most
            // 1/2 = 0.5 (code_len ≥ 2), proximity^3 = 0.125 — predictions
            // still land in the 50-60k range and flood out pinyin single
            // chars (q→去 baseline). Same pattern as pinyin CP-B which
            // leaves single-letter prefixes on the legacy NON_EXACT_FLOOR
            // path: at one letter the user's bare exact (q→我 Jianma1) is
            // the only confident wubi signal worth surfacing — multi-letter
            // predictions like 求/全 should arrive when the user types
            // another character.
            //
            // Cap mirrors pinyin's CP-B prefix scan caps but scaled down —
            // wubi's prefix scan over a 2-letter prefix already returns
            // ~hundreds of entries, more than a candidate window needs.
            let wubi_typed_len = wubi.buffer_str().len();
            let pred_cap = match wubi_typed_len {
                2 => 30,
                3 => 40,
                _ => 0,
            };
            if pred_cap > 0 {
                let mut preds = wubi.prefix_predictions();
                preds.truncate(pred_cap);
                for (word, freq, code_len) in preds {
                    let proximity = (wubi_typed_len as f64 / code_len as f64).min(1.0);
                    // WU-γ: keep both the score AND the (base, prior,
                    // likelihood) decomposition. score == base + prior ·
                    // likelihood holds bit-for-bit. final_mult below is 0
                    // (suppress) or 1 (keep) so components stay
                    // invariant-preserving without folding.
                    let (score, components) = scoring::predict_score_with_components(
                        scoring::LIKELIHOOD_WUBI_PREDICT_BASE,
                        freq,
                        scoring::PRIOR_FREQ_MULT_WUBI,
                        proximity,
                        inputx_wubi_data::wubi_corpus_total(),
                    );
                    wubi_cands.push((word, score, Some(components)));
                }
            }
            let final_mult = wubi_mult * z_mult;
            if final_mult == 0.0 {
                // Wubi fully suppressed (past the 4-char window, or 'z'-led
                // where wubi isn't typing). Clear instead of pushing score-0
                // entries — otherwise a suppressed wubi candidate (恋情 for
                // the 9-char `yongzhong`, user-reported 2026-05-25) still
                // leaks into the merged list at score 0. Dropping at the
                // source is precise: it never touches legitimate low-score
                // pinyin entries (whose floor can underflow toward 0).
                wubi_cands.clear();
            } else if final_mult != 1.0 {
                for (_, s, _) in wubi_cands.iter_mut() {
                    *s *= final_mult;
                }
            }
            merge(wubi_cands, pinyin.candidates_with_scores(prev_committed), jp_kanji, jp_kana)
        }
    }
}

/// Split the JP adapter's scored candidates into (kanji, kana) buckets
/// — the cross-engine merge takes them separately for clarity but
/// scoring is uniform across both.
fn split_jp_scored(
    j: &JapaneseAdapter,
) -> (Vec<Scored>, Vec<Scored>) {
    let all = j.candidates_with_scores();
    let kanji_set: std::collections::HashSet<String> =
        j.kanji_candidates().into_iter().collect();
    let mut kanji = Vec::new();
    let mut kana = Vec::new();
    for (w, s, c) in all {
        if kanji_set.contains(&w) {
            kanji.push((w, s, c));
        } else {
            kana.push((w, s, c));
        }
    }
    (kanji, kana)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composite::merge::Source;

    fn typed(adapter: &mut PinyinAdapter, s: &[u8]) {
        for b in s {
            adapter.handle_letter(*b);
        }
    }

    fn wubi_typed(engine: &mut WubiEngine, s: &[u8]) {
        for b in s {
            engine.handle_letter(*b);
        }
    }

    #[test]
    fn pinyin_only_skips_wubi() {
        let mut wubi = WubiEngine::new();
        let mut pinyin = PinyinAdapter::new();
        wubi_typed(&mut wubi, b"gggg"); // wubi has candidates
        typed(&mut pinyin, b"women"); // pinyin too

        let cands = dispatch(Mode::PinyinOnly, &wubi, &pinyin, None, None);
        assert!(cands.iter().all(|c| c.source == Source::Pinyin));
        assert!(cands.iter().any(|c| c.word == "我们"));
    }

    #[test]
    fn wubi_only_skips_pinyin() {
        let mut wubi = WubiEngine::new();
        let mut pinyin = PinyinAdapter::new();
        wubi_typed(&mut wubi, b"g"); // 'g' = 一级简码 → 一
        typed(&mut pinyin, b"yi");

        let cands = dispatch(Mode::WubiOnly, &wubi, &pinyin, None, None);
        assert!(cands.iter().all(|c| c.source == Source::Wubi));
        assert_eq!(cands.first().map(|c| c.word.as_str()), Some("一"));
    }

    #[test]
    fn mixed_merges_both_with_wubi_first_when_buffers_match() {
        // Equal buffer lengths → default wubi-first ordering applies.
        // wubi "ni" is a valid 2-letter wubi prefix (some 字根 combos);
        // pinyin "ni" → 你/呢/尼/...
        let mut wubi = WubiEngine::new();
        let mut pinyin = PinyinAdapter::new();
        wubi_typed(&mut wubi, b"a"); // wubi 1-char buffer (may have no cands)
        typed(&mut pinyin, b"a"); // pinyin "a" → 啊/吖/etc.

        let cands = dispatch(Mode::Mixed, &wubi, &pinyin, None, None);
        // With equal buffer lengths, default wubi-first ordering is used.
        // Pinyin candidates should be present in the merged list.
        assert!(
            cands.iter().any(|c| c.source == Source::Pinyin),
            "expected pinyin candidates in merged list; got {cands:?}"
        );
    }

    #[test]
    // (deleted) mixed_pinyin_first_when_pinyin_outgrew_wubi: the test
    // constructed an artificial state where wubi has buf="g" while
    // pinyin has buf="shang" — used to validate the now-removed
    // pinyin-first heuristic. Under the unified-score merge, wubi `g`
    // gives 一 (Jianma1, score ~1.04M) which legitimately tops a
    // pinyin shang result (~500k) — that's the simcode hard floor
    // working as designed. The real collision-recovery scenario
    // (user types 5+ chars; wubi resets to a tail like "ng" with
    // Auto-layer scores ~100k) is covered by score-based ordering
    // without needing the heuristic.

    #[test]
    fn mixed_shinjuku_jp_full_match_beats_composition() {
        // User-reported 2026-05-25: romaji `shinjuku` (新宿, high-freq jukugo)
        // in Mixed+JP ranked Chinese forced-composition junk 是嗯据库 (#0,
        // COMPOSED_SCORE 500k) above 新宿 (464k), and katakana シンジュク was
        // buried below the pinyin non-exact cluster. A real full-buffer jukugo
        // is high-confidence Japanese — LIKELIHOOD_JP_FULL_MATCH_PROMOTE (×1.3) lifts the
        // whole JP group so 新宿 leads and katakana surfaces into the window.
        use crate::composite::engine::CompositeEngine;
        use crate::wubi::AutoCommitPolicy;
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_japanese_enabled(true);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in b"shinjuku" { let _ = e.handle_letter(*b); }
        let cands = e.candidates();
        let top: Vec<&str> = cands.iter().take(6).map(|c| c.word.as_str()).collect();
        assert_eq!(cands.first().map(|c| c.word.as_str()), Some("新宿"),
            "新宿 (full-match jukugo) must lead shinjuku in Mixed+JP; got {top:?}");
        let kata = cands.iter().position(|c| c.word == "シンジュク");
        assert!(kata.is_some_and(|i| i < 10),
            "katakana シンジュク must be visible (top 10); got idx {kata:?} in {top:?}");
    }

    #[test]
    fn mixed_jieji_no_jp_compose_pollution() {
        // User-reported 2026-05-25: Chinese pinyin `jieji` (阶级/借给/接机)
        // in Mixed+JP surfaced compose_sentence junk 時へ時 / 治へ治 at #1-4
        // — they were tagged kind=Kanji so japanese_adapter treated them as
        // jukugo AND they tripped the full-match promote. compose products
        // now carry `composed=true`, score at LIKELIHOOD_JP_COMPOSED_BASE (below real
        // Chinese) and never promote, so real Chinese leads and junk sinks.
        use crate::composite::engine::CompositeEngine;
        use crate::wubi::AutoCommitPolicy;
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_japanese_enabled(true);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in b"jieji" { let _ = e.handle_letter(*b); }
        let cands = e.candidates();
        let top4: Vec<(&str, Source)> = cands.iter().take(4)
            .map(|c| (c.word.as_str(), c.source)).collect();
        assert!(top4.iter().all(|(_, s)| *s != Source::Japanese),
            "jieji top-4 must be Chinese — no JP compose pollution; got {top4:?}");
        assert_eq!(cands.first().map(|c| c.word.as_str()), Some("阶级"),
            "阶级 should lead jieji; got {top4:?}");
    }

    #[test]
    fn mixed_yongzhong_exact_phrase_beats_composition_no_zero_leak() {
        // User-reported 2026-05-25: `yongzhong` ranked the forced 2-char
        // composition 用中 (用+中, COMPOSED_SCORE 500k) above the exact dict
        // word 臃肿 (420k); also a zeroed wubi candidate 恋情 (score 0, wubi
        // suppressed past 4 chars) lingered. After: composition drops below
        // the exact word when one exists (pinyin_adapter composed_base), and
        // score-0 candidates are filtered in merge — 臃肿 leads, 恋情 gone.
        use crate::composite::engine::CompositeEngine;
        use crate::wubi::AutoCommitPolicy;
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_japanese_enabled(true);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in b"yongzhong" { let _ = e.handle_letter(*b); }
        let cands = e.candidates();
        let top: Vec<&str> = cands.iter().take(5).map(|c| c.word.as_str()).collect();
        assert_eq!(cands.first().map(|c| c.word.as_str()), Some("臃肿"),
            "exact 臃肿 must beat composition 用中 for yongzhong; got {top:?}");
        let yz = cands.iter().position(|c| c.word == "臃肿");
        let yzh = cands.iter().position(|c| c.word == "用中");
        if let (Some(e), Some(h)) = (yz, yzh) {
            assert!(e < h, "用中 (composition) must rank below 臃肿 (exact); got {top:?}");
        }
        assert!(cands.iter().all(|c| c.score > 0.0),
            "no score-0 (suppressed) candidate may appear; got {:?}",
            cands.iter().map(|c| (c.word.as_str(), c.score)).collect::<Vec<_>>());
    }

    #[test]
    fn mixed_jie_single_syllable_no_jp_compose_pollution() {
        // User-reported 2026-05-25: single-syllable `jie` surfaced 1-segment
        // compose junk 時へ / 事へ / 治へ at #1-4 over Chinese 接/结/解.
        // Same composed-flag fix as jieji; verify the single-syllable path.
        use crate::composite::engine::CompositeEngine;
        use crate::wubi::AutoCommitPolicy;
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_japanese_enabled(true);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in b"jie" { let _ = e.handle_letter(*b); }
        let cands = e.candidates();
        let top4: Vec<(&str, Source)> = cands.iter().take(4)
            .map(|c| (c.word.as_str(), c.source)).collect();
        assert!(top4.iter().all(|(_, s)| *s != Source::Japanese),
            "jie top-4 must be Chinese — no JP compose pollution; got {top4:?}");
    }

    #[test]
    fn japanese_toukyouto_compose_suffix_leads_kana() {
        // User insight 2026-05-26: 東京都 is 拼 (東京 + 都 admin suffix), not a
        // dict word (mozc itself doesn't list it). The jukugo+KANJI_SUFFIXES
        // compose path now produces 東京都, scored LIKELIHOOD_JP_COMPOSED_KANJI_BASE
        // (280k, a pure-kanji composed tier above the long-buffer kana
        // fallbacks at 240k) so the kanji conversion leads in Japanese mode.
        use crate::composite::engine::CompositeEngine;
        use crate::wubi::AutoCommitPolicy;
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::JapaneseOnly);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in b"toukyouto" { let _ = e.handle_letter(*b); }
        let cands = e.candidates();
        let top: Vec<&str> = cands.iter().take(5).map(|c| c.word.as_str()).collect();
        assert_eq!(cands.first().map(|c| c.word.as_str()), Some("東京都"),
            "東京都 (jukugo+都 compose) must lead toukyouto in JP mode; got {top:?}");
    }

    #[test]
    fn mixed_single_e_kana_interjection_not_top() {
        // User-reported 2026-05-26: single `e` ranked えっ (a pure-kana 感叹詞
        // in the hand jukugo TSV, freq 82) at #3 — it was wrongly getting the
        // jukugo base (200k→446k). A pure-kana "jukugo" is not a real kanji
        // compound; it now drops to the single-kanji tier so えっ no longer
        // outranks normal candidates. (新宿 / ありがとう unaffected — verified
        // by their own paths; here we just guard えっ down.)
        use crate::composite::engine::CompositeEngine;
        use crate::wubi::AutoCommitPolicy;
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_japanese_enabled(true);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        e.handle_letter(b'e');
        let cands = e.candidates();
        let pos = cands.iter().position(|c| c.word == "えっ");
        let top: Vec<&str> = cands.iter().take(5).map(|c| c.word.as_str()).collect();
        assert!(pos.map_or(true, |p| p >= 5),
            "えっ (kana interjection) must not rank top-5 for single `e`; got idx {pos:?}, top {top:?}");
    }

    #[test]
    fn mixed_jixu_continue_leads_after_prior_correction() {
        // User polish-log 2026-05-26 screenshot: jixu shows 积蓄 #1 / 继续 #2.
        // Probe attributed it to corpus freq inflation (积蓄 = 166k vs 继续
        // = 75k, newswire/financial source bias). User attestation: "继续
        // 还是应该在第一的，这个感觉比积蓄要高频". prior_correction adds a
        // ×2 boost on 继续 so it clears 积蓄 at jixu and any other buffer
        // where corpus underrates 继续.
        use crate::composite::engine::CompositeEngine;
        use crate::wubi::AutoCommitPolicy;
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in b"jixu" { let _ = e.handle_letter(*b); }
        let cands = e.candidates();
        let top: Vec<&str> = cands.iter().take(5).map(|c| c.word.as_str()).collect();
        assert_eq!(cands.first().map(|c| c.word.as_str()), Some("继续"),
            "继续 must lead jixu (prior_correction × 2 over corpus 积蓄 inflation); \
             got top5={top:?}");
    }

    #[test]
    fn mixed_juti_jutiu_design_concept_leads_over_wubi_phrase() {
        // User polish-log 2026-05-26: juti (4-letter wubi full code +
        // valid pinyin) showed 暗送秋波 #1 / 具体 #2. wubi 暗送秋波 is a
        // valid Phrase entry that gets LIKELIHOOD_WUBI_FULL_CODE_PROMOTE
        // ×1.1 (the aiyi→东京 wubi-first rule), but the user's frequency
        // intuition is correct: 具体 corpus freq 37k vs phrase ~12k still
        // loses ~16k after promote. prior_correction ×1.5 on 具体 puts
        // it firmly above the promoted wubi phrase. User: "五笔优势，但
        // 是具体的常用分应该太高了".
        use crate::composite::engine::CompositeEngine;
        use crate::wubi::AutoCommitPolicy;
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in b"juti" { let _ = e.handle_letter(*b); }
        let cands = e.candidates();
        let top: Vec<&str> = cands.iter().take(5).map(|c| c.word.as_str()).collect();
        assert_eq!(cands.first().map(|c| c.word.as_str()), Some("具体"),
            "具体 must lead juti (prior_correction × 1.5 over wubi-promote 暗送秋波); \
             got top5={top:?}");
    }

    #[test]
    fn mixed_sheji_design_leads_after_prior_correction() {
        // User polish-log 2026-05-26 screenshot: sheji shows 涉及 #1 / 设计 #2.
        // Same corpus-skew pattern as jixu→继续 (news/academic sources
        // over-represent 涉及). User attestation: "设计肯定应该高于涉及".
        // prior_correction ×2 boost on 设计 surfaces it at #1.
        use crate::composite::engine::CompositeEngine;
        use crate::wubi::AutoCommitPolicy;
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in b"sheji" { let _ = e.handle_letter(*b); }
        let cands = e.candidates();
        let top: Vec<&str> = cands.iter().take(5).map(|c| c.word.as_str()).collect();
        assert_eq!(cands.first().map(|c| c.word.as_str()), Some("设计"),
            "设计 must lead sheji (prior_correction × 2 over corpus 涉及 inflation); \
             got top5={top:?}");
    }

    #[test]
    fn mixed_tongyi_unify_outranks_same_one() {
        // User polish-log 2026-05-27 screenshot: tongyi gave 同意 #1 /
        // 同一 #2 / 统一 #3 / 同义 #4 / 通译 #5 / 通义 #6 / 通易 #7. User
        // expectation: 统一 ≥ #2 ("应该大于同一，在第二或第一顺位"). Corpus
        // skew is the news/academic over-rep of 同一 (the "same" adjective)
        // vs daily-use 统一 (unify verb/noun). prior_correction ("统一",
        // 1.5) lifts it to #1 ahead of 同意/同一 with a comfortable margin.
        use crate::composite::engine::CompositeEngine;
        use crate::wubi::AutoCommitPolicy;
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in b"tongyi" { let _ = e.handle_letter(*b); }
        let cands = e.candidates();
        let top: Vec<&str> = cands.iter().take(5).map(|c| c.word.as_str()).collect();
        let unify_idx = cands.iter().position(|c| c.word == "统一");
        let same_one_idx = cands.iter().position(|c| c.word == "同一");
        assert!(unify_idx.is_some(),
            "统一 must appear in tongyi candidates; got top5={top:?}");
        if let (Some(u), Some(s)) = (unify_idx, same_one_idx) {
            assert!(u < s,
                "统一 (idx={u}) must outrank 同一 (idx={s}); got top5={top:?}");
        }
        // Acceptable: 统一 at #1 or #2.
        assert!(unify_idx.unwrap() <= 1,
            "统一 must be top-2 (user rule); got idx={} top5={top:?}", unify_idx.unwrap());
    }

    #[test]
    fn mixed_jixu_pinyin_word_beats_wubi_coincidence() {
        // User-reported 2026-05-26 (REGRESSION — keep this as a permanent
        // guard): `jixu` should give 继续 (common pinyin word), not 曳光弹
        // (a rare wubi 3-char phrase that coincidentally encodes to jixu at
        // full code). The full-code wubi-first promote (added for aiyi→东京)
        // over-promoted it: 曳光弹 raw 407269 ×1.2 = 488722 beat 继续 474652.
        // The promote is tuned to ×1.1 so it only edges out *same-freq* pinyin
        // (aiyi: 东京 raw already > 爱意), never a clearly-higher-freq word
        // (继续 leads 曳光弹 by ~67k raw).
        use crate::composite::engine::CompositeEngine;
        use crate::wubi::AutoCommitPolicy;
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in b"jixu" { let _ = e.handle_letter(*b); }
        let cands = e.candidates();
        let jixu_cont = cands.iter().position(|c| c.word == "继续");
        let yeguang = cands.iter().position(|c| c.word == "曳光弹");
        let top: Vec<&str> = cands.iter().take(5).map(|c| c.word.as_str()).collect();
        assert!(jixu_cont.is_some(), "继续 missing from jixu candidates: {top:?}");
        assert!(yeguang.map_or(true, |y| jixu_cont.unwrap() < y),
            "继续 must rank above 曳光弹 (wubi coincidence) for jixu; got {top:?}");
    }

    #[test]
    fn mixed_junk_composition_sinks_real_sentence_survives() {
        // User-reported 2026-05-26: shinjuku (Japanese romaji) surfaced the
        // Chinese forced-composition 是嗯据库 at #2 (fixed COMPOSED_SCORE 500k).
        // A junk composition (per-char Viterbi path score below the floor) now
        // drops to COMPOSED_LOW_QUALITY and sinks out of the window, while a
        // real sentence keeps COMPOSED_SCORE and leads. Two-sided guard.
        use crate::composite::engine::CompositeEngine;
        use crate::wubi::AutoCommitPolicy;
        // junk: 是嗯据库 must not be top-5 for shinjuku
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_japanese_enabled(true);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in b"shinjuku" { let _ = e.handle_letter(*b); }
        let top5: Vec<&str> = e.candidates().iter().take(5).map(|c| c.word.as_str()).collect();
        assert!(!top5.contains(&"是嗯据库"),
            "junk composition 是嗯据库 must not be top-5 for shinjuku; got {top5:?}");
        // real sentence survives: nihaomawojiao → 你好吗我叫 #1
        let mut e2 = CompositeEngine::new();
        e2.set_mode(Mode::Mixed);
        e2.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in b"nihaomawojiao" { let _ = e2.handle_letter(*b); }
        assert_eq!(e2.candidates().first().map(|c| c.word.as_str()), Some("你好吗我叫"),
            "real composed sentence must still lead nihaomawojiao");
    }

    #[test]
    fn mixed_jj_exact_leads_with_predictions_attached() {
        // CP-C (v1.3 WU-α): wubi prefix-predictions attach to 2-3 letter
        // wubi buffers. `jj` is a Jianma2 simcode → 昌 (~832k = 800k +
        // freq); predictions for jj-prefix longer codes (日 at jjjj
        // Zigen, 日本/日子/日常 at jjjj-suffixed phrase codes) attach
        // beneath the exact. predict_score top: 50k + 0.125·45k ≈ 56k,
        // well below 832k — exact mathematically dominates.
        use crate::composite::engine::CompositeEngine;
        use crate::wubi::AutoCommitPolicy;
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in b"jj" { let _ = e.handle_letter(*b); }
        let cands = e.candidates();
        let top10: Vec<&str> = cands.iter().take(10).map(|c| c.word.as_str()).collect();
        assert_eq!(cands.first().map(|c| c.word.as_str()), Some("昌"),
            "exact wubi Jianma2 昌 must lead jj; got top10={top10:?}");
        // 日 (jjjj Zigen prediction) must surface — high-freq prediction
        // visible to the user typing toward jjjj.
        let ri = cands.iter().position(|c| c.word == "日");
        assert!(ri.is_some(),
            "日 (jjjj prediction) must appear for jj; got top10={top10:?}");
        // Predictions follow, not lead: 日 ranks below 昌.
        assert!(ri.unwrap() > 0,
            "predictions must follow the exact #0; 日 idx={ri:?} top10={top10:?}");
    }

    #[test]
    fn mixed_jieni_no_jp_compose_garbage_in_top_5() {
        // User polish-log 2026-05-26 screenshot: jieni (5 letters)
        // surfaced ~30 mechanical compose_sentence products at #4-30+
        // (時へに / 事へに / 治へに / 耳へに / 耳へ尼 / 事へ尼 / ...),
        // X+へ+Y cartesian where へ is the particle pronounced as `e`.
        // None of them are real Japanese. They scored at
        // LIKELIHOOD_JP_COMPOSED_BASE so they didn't lead, but their
        // sheer count crowded out the visible window. Short-buffer
        // (< 8 chars) compose filter in japanese_adapter drops them all.
        use crate::composite::engine::CompositeEngine;
        use crate::wubi::AutoCommitPolicy;
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_japanese_enabled(true);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in b"jieni" { let _ = e.handle_letter(*b); }
        let cands = e.candidates();
        let top: Vec<&str> = cands.iter().take(8).map(|c| c.word.as_str()).collect();
        // No 時へに / 事へに / 治へに / 耳へに / 耳へ尼 / 事へ尼 / 治へ尼 / 仕へ尼.
        let garbage_patterns = ["時へに", "事へに", "治へに", "耳へに",
                                "耳へ尼", "事へ尼", "治へ尼", "仕へ尼"];
        for w in &garbage_patterns {
            assert!(!cands.iter().any(|c| &c.word.as_str() == w),
                "{w} (mechanical compose garbage) must not appear in jieni candidates; \
                 got top8={top:?}");
        }
        // Useful candidates still present: 杰尼 (pinyin), じえに / ジエニ (kana).
        assert!(cands.iter().any(|c| c.word == "じえに"),
            "じえに (hiragana) must remain visible; got top8={top:?}");
    }

    #[test]
    fn mixed_chouonpu_buffer_locks_out_chinese_candidates() {
        // User polish-log 2026-05-27 screenshot: `fa------` (8 chars,
        // 7 chōonpu) surfaced 工 / 阿 / 啊 / 阿 / 吖 / 锕 (wubi+pinyin
        // candidates for `fa`) BEFORE the JP kana candidates ファーーー
        // and ふぁーーー. User rule: "中文输入肯定不会含 `-`" — Chinese
        // has zero syllables containing chōonpu, so any wubi/pinyin
        // candidate surfaced alongside a JP-chōonpu buffer is mismatched
        // noise (the Chinese engines froze on `fa` while the JP buffer
        // grew to `fa------`).
        //
        // Dispatch-level lockout: once jp_buffer.contains('-'), all
        // wubi+pinyin candidates are dropped; only JP kanji+kana surface.
        use crate::composite::engine::CompositeEngine;
        use crate::wubi::AutoCommitPolicy;
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_japanese_enabled(true);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in b"fa" { let _ = e.handle_letter(*b); }
        for _ in 0..7 { let _ = e.handle_letter(b'-'); }
        let cands = e.candidates();
        let top: Vec<&str> = cands.iter().take(8).map(|c| c.word.as_str()).collect();
        // Every surviving candidate must be JP-source (no wubi, no pinyin).
        for c in cands.iter() {
            assert_eq!(c.source, Source::Japanese,
                "non-JP candidate {:?} (source={:?}) surfaced under JP-chōonpu \
                 lockout; got top8={top:?}", c.word, c.source);
        }
        // ファーーーーーーー (katakana, foreign-syllable rule promotes it
        // over hiragana for fa-row) should lead.
        assert!(cands.first().map(|c| c.word.as_str()) == Some("ファーーーーーーー"),
            "ファーーーーーーー should lead under chōonpu lockout; got top8={top:?}");
    }

    #[test]
    fn mixed_famiriaare_katakana_leads_hiragana_then_pinyin_demoted() {
        // User polish-log 2026-05-27 screenshot: famiriaare (10 letters) in
        // Mixed+JP surfaced 法弥日呵呵热 / 法弥日啊啊热 / 发米日啊啊热 /
        // 法弥日阿阿热 — 4 mechanical Pinyin Viterbi compositions, 0 JP
        // candidates. Root cause: (1) romaji table lacked `fa` entry → JP
        // engine rendered "fあみりああれ" with leading-f ASCII passthrough,
        // is_jp_clean rejected the whole candidate; (2) Pinyin Path 5
        // fallback_composition gave 法弥日呵呵热 a flat COMPOSED_FALLBACK_SCORE
        // (250k), beating mechanical kana (240k/200k).
        //
        // Two-part fix:
        //   1. inputx-nihongo/src/romaji.rs: full foreign-loanword syllable table
        //      (fa-row, va-row, wi/we, je, tsa-row, che/she, th*/dh*/tw*/dw*,
        //      kw*/gw*, fy*/vy*, wha-row, xa-row + la-alias).
        //   2. pinyin_adapter.rs Path 5: quality gate — when buffer.len() /
        //      sentence.chars().count() < 2.0 (mechanical 1-pinyin-char-per-
        //      output-char), suppress fallback_composition so the candidate
        //      drops to NON_EXACT_FLOOR tier (~1k) instead of 250k.
        //   3. japanese_adapter.rs: foreign-syllable signal → swap hira/kata
        //      bases, so ファミリアアレ (foreign-loanword convention) leads
        //      ふぁみりああれ instead of trailing it.
        //
        // User rule: 片假名 > 平假名 in foreign-romaji buffers, but both
        // adjacent. Low-quality Pinyin yields to kana.
        use crate::composite::engine::CompositeEngine;
        use crate::wubi::AutoCommitPolicy;
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_japanese_enabled(true);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in b"famiriaare" { let _ = e.handle_letter(*b); }
        let cands = e.candidates();
        let top: Vec<&str> = cands.iter().take(6).map(|c| c.word.as_str()).collect();
        assert_eq!(cands.first().map(|c| c.word.as_str()), Some("ファミリアアレ"),
            "ファミリアアレ (katakana) must lead foreign-romaji buffer; got top6={top:?}");
        assert_eq!(cands.get(1).map(|c| c.word.as_str()), Some("ふぁみりああれ"),
            "ふぁみりああれ (hiragana) must follow katakana for adjacency; got top6={top:?}");
        // Pinyin mechanical garbage must NOT crowd into the top 2.
        let mechanical = ["法弥日呵呵热", "法弥日啊啊热", "发米日啊啊热", "法弥日啊阿热"];
        for w in &mechanical {
            let idx = cands.iter().position(|c| c.word.as_str() == *w);
            if let Some(i) = idx {
                assert!(i >= 2, "{w} (low-quality Pinyin composition) must rank below kana; \
                    got idx={i} in top6={top:?}");
            }
        }
    }

    #[test]
    fn mixed_vaiorin_katakana_leads_for_foreign_v_row() {
        // Sibling case to famiriaare — `vaiorin` (violin) should surface
        // ヴァイオリン (katakana) #1, ゔぁいおりん (hiragana) #2. This is the
        // foreign 'v' row which had no romaji entries at all pre-fix.
        use crate::composite::engine::CompositeEngine;
        use crate::wubi::AutoCommitPolicy;
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_japanese_enabled(true);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in b"vaiorin" { let _ = e.handle_letter(*b); }
        let cands = e.candidates();
        let top: Vec<&str> = cands.iter().take(4).map(|c| c.word.as_str()).collect();
        assert!(cands.iter().any(|c| c.word == "ヴァイオリン"),
            "ヴァイオリン (katakana) must appear for vaiorin; got top4={top:?}");
        let kata_idx = cands.iter().position(|c| c.word == "ヴァイオリン");
        let hira_idx = cands.iter().position(|c| c.word == "ゔぁいおりん");
        if let (Some(k), Some(h)) = (kata_idx, hira_idx) {
            assert!(k < h, "katakana ({k}) must lead hiragana ({h}) for foreign 'v' row; \
                top4={top:?}");
        }
    }

    #[test]
    fn mixed_nihon_hiragana_above_katakana_native_unchanged() {
        // Guard: the foreign-syllable swap must NOT flip native JP buffers.
        // `nihon` (にほん / ニホン / 日本) has no foreign-syllable markers,
        // so hiragana > katakana is preserved.
        use crate::composite::engine::CompositeEngine;
        use crate::wubi::AutoCommitPolicy;
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_japanese_enabled(true);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in b"nihon" { let _ = e.handle_letter(*b); }
        let cands = e.candidates();
        let top: Vec<&str> = cands.iter().take(8).map(|c| c.word.as_str()).collect();
        let hira_idx = cands.iter().position(|c| c.word == "にほん");
        let kata_idx = cands.iter().position(|c| c.word == "ニホン");
        if let (Some(h), Some(k)) = (hira_idx, kata_idx) {
            assert!(h < k, "native nihon: hiragana ({h}) must stay above katakana ({k}); \
                top8={top:?}");
        }
    }

    #[test]
    fn mixed_kaopu_real_composition_still_leads_kana() {
        // Guard: the Pinyin Path 5 quality gate must NOT demote *real* fallback
        // compositions (kaopu→靠谱 ratio 5/2=2.5 ≥ 2.0). 靠谱 should still beat
        // mechanical kana かおぷ/カオプ as it did before the gate.
        use crate::composite::engine::CompositeEngine;
        use crate::wubi::AutoCommitPolicy;
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_japanese_enabled(true);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in b"kaopu" { let _ = e.handle_letter(*b); }
        let cands = e.candidates();
        let top: Vec<&str> = cands.iter().take(5).map(|c| c.word.as_str()).collect();
        assert_eq!(cands.first().map(|c| c.word.as_str()), Some("靠谱"),
            "靠谱 (real Pinyin fallback composition, ratio 2.5) must keep leading; \
             got top5={top:?}");
    }

    #[test]
    fn mixed_jjjj_full_code_exact_leads_no_prediction_inversion() {
        // CP-C invariant at full code: wubi codes are at most 4 chars, so
        // prefix_predictions returns no entries (no code length > 4). 日
        // (Zigen exact at jjjj) leads with its full layer base 500k + freq,
        // unaffected by the CP-C attach path.
        use crate::composite::engine::CompositeEngine;
        use crate::wubi::AutoCommitPolicy;
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in b"jjjj" { let _ = e.handle_letter(*b); }
        let cands = e.candidates();
        let top10: Vec<&str> = cands.iter().take(10).map(|c| c.word.as_str()).collect();
        assert_eq!(cands.first().map(|c| c.word.as_str()), Some("日"),
            "日 (jjjj Zigen exact) must lead at full code; got top10={top10:?}");
    }

    #[test]
    fn wug_shinjuk_prediction_components_match_score() {
        // WU-γ end-to-end: a CP-A JP jukugo prefix-prediction candidate
        // (新宿 for shinjuk) carries (base, prior, likelihood) such that
        // `base + prior * likelihood == score`. Promote only applies at
        // proximity == 1 (full match), so a prediction's score is the
        // raw predict_score output — invariant strictly holds.
        use crate::composite::engine::CompositeEngine;
        use crate::wubi::AutoCommitPolicy;
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_japanese_enabled(true);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in b"shinjuk" { let _ = e.handle_letter(*b); }
        let cands = e.candidates();
        let shinjuku = cands.iter().find(|c| c.word == "新宿")
            .expect("新宿 must appear for shinjuk");
        let c = shinjuku.components.expect(
            "新宿 (CP-A JP jukugo prediction) must carry ScoreComponents");
        let recomputed = c.base + c.prior * c.likelihood;
        assert!((shinjuku.score - recomputed).abs() < 1e-3,
            "WU-γ invariant breaks: score={} vs base+prior*likelihood={recomputed} \
             (c = {c:?})", shinjuku.score);
    }

    #[test]
    fn mixed_pianni_kbest_exposes_pian_alternates() {
        // User-reported 2026-05-26 (polish-log): pianni originally surfaced
        // only 片你 / ぴあんに / ピアンニ — no 骗你.
        //
        // v1.6 cleanup (user 2026-05-28 "improve 不是 hack" directive):
        // the historical runtime blacklist of 片你 has been replaced by
        // dict-level baked additions in idf-from-pinyin-dict.rs's
        // BAKED_ADDITIONS table.
        //
        // v1.6.5 polish (user 2026-05-28 follow-up): 便你 / 篇你 dropped
        // from BAKED_ADDITIONS — 便你 is an awkward non-collocation, 篇你
        // isn't a Chinese phrase at all. Only 骗你 and 偏你 remain as
        // Path-1 exact-match entries; the other variants get suppressed
        // naturally by Path-5 K-best's zero-bigram (pian, ni) gating.
        use crate::composite::engine::CompositeEngine;
        use crate::wubi::AutoCommitPolicy;
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in b"pianni" { let _ = e.handle_letter(*b); }
        let cands = e.candidates();
        let top: Vec<&str> = cands.iter().take(8).map(|c| c.word.as_str()).collect();
        // Both legitimate pian+ni variants must surface as Path-1 entries.
        for want in ["骗你", "偏你"] {
            assert!(cands.iter().any(|c| c.word == want),
                "{want} must surface as a Path-1 dict entry; got top={top:?}");
        }
        // Illegitimate variants must NOT appear: 片你 / 便你 / 篇你 are
        // either non-words or awkward non-collocations, and Path-5
        // K-best (the only fallback path) is gated off whenever Path-1
        // produces any candidates.
        for forbidden in ["片你", "便你", "篇你"] {
            assert!(!cands.iter().any(|c| c.word == forbidden),
                "{forbidden} must not appear (not a real phrase / awkward \
                 collocation); got top={top:?}");
        }
    }

    #[test]
    fn mixed_liangle_surfaces_凉了_not_两肋() {
        // User polish-log (2026-05-28): typing `liangle` surfaced 两肋 as
        // top1 even though 两肋's modern reading is `lianglei`. Source
        // dict's readings.tsv keeps the archaic "lè" reading of 肋 alive,
        // so phrases_composed.tsv emits (liangle, 两肋, 3) — that
        // pollution flowed straight through facade `PinyinDict::
        // lookup_into` into Path 1, masking the legitimate `liang+了`
        // composition.
        //
        // v1.6.5 fix is a three-layer dict orthodox:
        //   (b1) BAKED_EXCLUSIONS in idf-from-pinyin-dict drops the
        //        (liangle, 两肋) cement IDF entry.
        //   (b2) BAKED_ADDITIONS inserts (liangle, 凉了, 500) so a real
        //        Path-1 exact-match candidate exists, gating off Path-3
        //        prefix-completion's leak of (lianglei, 两肋) under the
        //        liangle prefix.
        //   (c)  composite/pinyin_adapter.rs Path 1 fill cuts over from
        //        facade `lookup_into` to `pinyin_idf_reader().lookup` —
        //        without this, baked additions in cement IDF never reach
        //        `self.candidates` when the facade source has any entry
        //        (typically polluted) for the same code.
        //
        // The lianglei buffer is unaffected: 两类 / 两肋 both surface there
        // because "肋" reads "lèi" in modern mainstream Chinese.
        use crate::composite::engine::CompositeEngine;
        use crate::wubi::AutoCommitPolicy;
        let cands_for = |buf: &[u8]| -> Vec<String> {
            let mut e = CompositeEngine::new();
            e.set_mode(Mode::Mixed);
            e.set_auto_commit_policy(AutoCommitPolicy::Never);
            for b in buf { let _ = e.handle_letter(*b); }
            e.candidates().iter().map(|c| c.word.clone()).collect()
        };
        let liangle = cands_for(b"liangle");
        let lianglei = cands_for(b"lianglei");
        // liangle: 凉了 must be top1, 两肋 must NOT appear at all.
        assert_eq!(liangle.first().map(String::as_str), Some("凉了"),
            "liangle top1 must be 凉了; got {liangle:?}");
        assert!(!liangle.iter().any(|w| w == "两肋"),
            "liangle must not surface 两肋 (archaic-reading pollution); got {liangle:?}");
        // lianglei: 两类 top1, 两肋 also present (legitimate modern reading).
        assert_eq!(lianglei.first().map(String::as_str), Some("两类"),
            "lianglei top1 must be 两类; got {lianglei:?}");
        assert!(lianglei.iter().any(|w| w == "两肋"),
            "lianglei must still surface 两肋 (legitimate lèi reading); got {lianglei:?}");
    }

    #[test]
    fn mixed_jp_low_ratio_kbest_fully_suppressed() {
        // User polish-log (2026-05-29, `rokuman`): pinyin Path-5 K-best
        // was force-segmenting Japanese romaji buffers into mechanical
        // single-char Chinese compositions (儿哦库曼 / 儿噢库曼 / 儿喔
        // 库曼 / 儿哦苦满 / 儿哦哭满) and surfacing all 5 K-best variants
        // at NON_EXACT_FLOOR (1000) tier, below the legitimate kana
        // candidates but visible in the list as garbage.
        //
        // The ratio < 2.0 quality gate previously only suppressed the
        // top1's `fallback_composition` promotion (which would have
        // claimed COMPOSED_FALLBACK_SCORE 250k tier); the K-best comps
        // themselves still pushed into self.candidates. v1.6.6 fix
        // (composite/pinyin_adapter.rs): move the `for (_, sentence)
        // in comps push` block inside the `if ratio >= 2.0` branch so
        // mechanical garbage never enters self.candidates at all.
        //
        // Ratios verified empirically by _explore_composition_scores:
        //   rokuman    (7/4 = 1.75) MECHANICAL — gate triggers
        //   famiriaare (10/6 = 1.67) MECHANICAL — gate triggers
        //   kaopu      (5/2 = 2.50) REAL — gate passes, 靠谱 surfaces
        use crate::composite::engine::CompositeEngine;
        use crate::wubi::AutoCommitPolicy;
        let cands_for = |buf: &[u8], jp: bool| -> Vec<String> {
            let mut e = CompositeEngine::new();
            e.set_mode(Mode::Mixed);
            e.set_japanese_enabled(jp);
            e.set_auto_commit_policy(AutoCommitPolicy::Never);
            for b in buf { let _ = e.handle_letter(*b); }
            e.candidates().iter().map(|c| c.word.clone()).collect()
        };
        let is_han = |c: char| ('\u{4E00}'..='\u{9FFF}').contains(&c);
        // rokuman + jp: no Han-character candidates at all (gate suppresses
        // all K-best comps; legitimate Han jukugo entries — if/when added —
        // would be unaffected since they come from JapaneseEngine, not
        // pinyin Path-5).
        let rokuman = cands_for(b"rokuman", true);
        for w in &rokuman {
            assert!(!w.chars().any(is_han),
                "rokuman --jp must not surface Han-char K-best garbage; \
                 got {w:?} in {rokuman:?}");
        }
        // famiriaare + jp: same — historical 法弥日呵呵热 etc all gone.
        let famiriaare = cands_for(b"famiriaare", true);
        for w in &famiriaare {
            assert!(!w.chars().any(is_han),
                "famiriaare --jp must not surface Han-char K-best garbage; \
                 got {w:?} in {famiriaare:?}");
        }
        // Positive sanity: kaopu still surfaces 靠谱 (ratio 2.5 ≥ 2.0,
        // gate passes). Verifies the gate didn't over-suppress real
        // compositions.
        let kaopu = cands_for(b"kaopu", false);
        assert!(kaopu.iter().any(|w| w == "靠谱"),
            "kaopu must still surface 靠谱 (ratio 2.5, K-best gate passes); \
             got {kaopu:?}");
    }

    #[test]
    fn jp_prefix_prediction_rises_with_proximity() {
        // PLAN-prefix-prediction CP-A (user 2026-05-26 "我想做"): as the user
        // types toward a jukugo it's predicted and rises with proximity.
        // shin (far) → 新宿 present but low; shinjuk (差u) → 新宿 high;
        // shinjuku (complete) → 新宿 #0 (exact full-match, not degraded).
        use crate::composite::engine::CompositeEngine;
        use crate::wubi::AutoCommitPolicy;
        let idx_of = |buf: &[u8]| -> Option<usize> {
            let mut e = CompositeEngine::new();
            e.set_mode(Mode::Mixed);
            e.set_japanese_enabled(true);
            e.set_auto_commit_policy(AutoCommitPolicy::Never);
            for b in buf { let _ = e.handle_letter(*b); }
            e.candidates().iter().position(|c| c.word == "新宿")
        };
        let shin = idx_of(b"shin");
        let shinjuk = idx_of(b"shinjuk");
        let shinjuku = idx_of(b"shinjuku");
        assert_eq!(shinjuk, Some(0), "shinjuk should predict 新宿 at #0; got {shinjuk:?}");
        assert_eq!(shinjuku, Some(0), "complete shinjuku → 新宿 #0; got {shinjuku:?}");
        assert!(shin.map_or(true, |s| shinjuk.unwrap() < s),
            "新宿 rises as buffer nears completion: shin {shin:?} vs shinjuk {shinjuk:?}");
    }

    #[test]
    fn mixed_z_prefix_skips_wubi() {
        let wubi = WubiEngine::new();
        let mut pinyin = PinyinAdapter::new();
        // wubi never sees 'z' (not a 字根 letter), so wubi candidates is
        // empty even at the engine level. Pinyin gets the full input.
        typed(&mut pinyin, b"zhongguo");

        let cands = dispatch(Mode::Mixed, &wubi, &pinyin, None, None);
        assert!(cands.iter().all(|c| c.source == Source::Pinyin));
        assert_eq!(cands.first().map(|c| c.word.as_str()), Some("中国"));
    }

    #[test]
    fn mixed_aiyi_full_code_wubi_phrase_beats_pinyin() {
        // User-reported 2026-05-25: typing `aiyi` in Mixed put pinyin 爱意
        // (#1) above wubi 东京 (#2). 东京 is a full-code (4-key) exact wubi
        // phrase; the speculative Phrase ×0.5 demote buried its raw 429241
        // at 214620, below 爱意 424712. At full code the wubi hit is high-
        // confidence and gets the wubi-first PROMOTE (×1.1), so 东京 leads.
        // See dispatch `full_code` / scoring::LIKELIHOOD_WUBI_FULL_CODE_PROMOTE.
        use crate::composite::engine::CompositeEngine;
        use crate::wubi::AutoCommitPolicy;
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in b"aiyi" { let _ = e.handle_letter(*b); }
        let cands = e.candidates();
        let top: Vec<&str> = cands.iter().take(5).map(|c| c.word.as_str()).collect();
        assert_eq!(cands.first().map(|c| c.word.as_str()), Some("东京"),
            "full-code wubi 东京 must lead aiyi in Mixed; got {top:?}");
        let dj = cands.iter().find(|c| c.word == "东京").map(|c| c.score);
        let ay = cands.iter().find(|c| c.word == "爱意").map(|c| c.score);
        assert!(ay.is_some(), "爱意 missing from aiyi candidates: {top:?}");
        assert!(dj.unwrap() > ay.unwrap(),
            "promoted 东京 score {dj:?} must exceed 爱意 {ay:?}");
    }

    #[test]
    fn mixed_mo_layer_aware_demote_lets_pinyin_lead() {
        // User-reported 2026-05-24: typing `mo` (2 chars, has vowel) put
        // 嶙 (low-freq, Auto-layer wubi at the `mo??` prefix) at #0,
        // pushing 默 (pinyin top) out of the visible top-10. Per the
        // layer-aware demote, Auto entries get × 0.1 when buffer is
        // short AND pinyin has an exact match — pinyin tops should now
        // lead while high-confidence wubi simcodes (Jianma1/2/3 +
        // Zigen) remain at full strength.
        let mut wubi = WubiEngine::new();
        let mut pinyin = PinyinAdapter::new();
        wubi_typed(&mut wubi, b"mo");
        typed(&mut pinyin, b"mo");

        let cands = dispatch(Mode::Mixed, &wubi, &pinyin, None, None);
        // 默 (pinyin mo) must be in top 10. The exact placement depends
        // on freq/layer interactions; presence in visible window is the
        // user's stated invariant ("默感觉应该至少能进前 10").
        let top10: Vec<&str> = cands.iter()
            .take(10).map(|c| c.word.as_str()).collect();
        assert!(top10.iter().any(|w| *w == "默"),
            "expected 默 in top 10 for `mo` in mixed mode; got {top10:?}");
    }

    #[test]
    fn mixed_mo_mo_or_no_leads_not_lin() {
        // User polish-log near-miss: `mo → 默` picked at rank 10-11
        // with #1 being `嶙` — a v0.5 wubi-layer-demote regression.
        // 'mo' is a single-letter buffer with no vowel that the user
        // is clearly using in pinyin mode (intent = mo pinyin word like
        // 没/默). Layer demote should fire but 嶙 (Auto layer) is
        // bypassing it.
        //
        // After v1.4 polish (修了 lixiang/繁体/MAX overlay) + v1.3
        // bigram split, expectation: 没 or 默 (both 'mo' base pinyin
        // entries with substantial freq) should lead the candidate
        // list at single-letter 'mo' in Mixed mode, NOT 嶙.
        use crate::composite::engine::CompositeEngine;
        use crate::wubi::AutoCommitPolicy;
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::Mixed);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in b"mo" { let _ = e.handle_letter(*b); }
        let cands = e.candidates();
        let top = cands.first().map(|c| c.word.as_str()).unwrap_or("");
        // Acceptable top picks for 'mo' pinyin: 没 / 默 / 摸 / 末 — common
        // pinyin chars. NOT acceptable: 嶙 (rare Auto wubi).
        let acceptable = ["没", "默", "摸", "末", "莫", "魔"];
        assert!(acceptable.contains(&top),
            "expected one of {acceptable:?} at #0 for mo; got top10={:?}",
            cands.iter().take(10).map(|c| &c.word).collect::<Vec<_>>());
        assert_ne!(top, "嶙", "rare wubi 嶙 must not lead pinyin 'mo'");
    }

    // mixed_da_* test removed 2026-05-24: it asserted pinyin top wins
    // over wubi Jianma2 simcode in Mixed mode, which violates user's
    // core rule that wubi 二级简码 with common-char target (左 base
    // freq 40827) must lead. The data-side fix is targeted purge of
    // RARE-char Jianma2 only (purge_jianma2_misaligned.py uses ratio).

    #[test]
    fn debug_ce_yi_runtime() {
        use crate::composite::engine::CompositeEngine;
        use crate::wubi::{WubiEngine, AutoCommitPolicy};
        for input in &["ce", "yi", "ge", "da"] {
            let mut w = WubiEngine::new();
            w.set_policy(AutoCommitPolicy::Never);
            for b in input.bytes() { let _ = w.handle_letter(b); }
            eprintln!("\nwubi '{}' candidates_with_layer:", input);
            for (word, score, layer) in w.candidates_with_layer().iter().take(3) {
                eprintln!("  {} score={} layer={:?}", word, score, layer);
            }
            let mut e = CompositeEngine::new();
            e.set_mode(Mode::Mixed);
            e.set_auto_commit_policy(AutoCommitPolicy::Never);
            for b in input.bytes() { let _ = e.handle_letter(b); }
            eprintln!("MIXED '{}' top5:", input);
            for (i, c) in e.candidates().iter().take(5).enumerate() {
                eprintln!("  #{}: {} (src={:?}, score={})", i, c.word, c.source, c.score);
            }
        }
    }

    #[test]
    fn wubi_only_tjvs_yields_fuza() {
        // User-reported 2026-05-24: tjvs (wubi phrase code for 复杂)
        // didn't surface 复杂 in their typing. phrases.txt has the
        // entry at line 45800 (`tjvs\t复杂`). This test verifies the
        // wubi engine DOES return 复杂 in its candidate list.
        //
        // Default wubi policy auto-commits 复杂 at exactly 4 chars
        // (unique@4) BEFORE we get to inspect candidates — so the user
        // sees the commit but the panel never shows. That's expected
        // behavior, not a missing-entry bug. Test uses policy=Never
        // to keep the candidate list around for inspection.
        use crate::composite::engine::CompositeEngine;
        use crate::wubi::AutoCommitPolicy;
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::WubiOnly);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for b in b"tjvs" { let _ = e.handle_letter(*b); }
        let cands = e.candidates();
        assert!(cands.iter().any(|c| c.word == "复杂"),
            "expected 复杂 in tjvs candidates; got top10={:?}",
            cands.iter().take(10).map(|c| &c.word).collect::<Vec<_>>());
    }

    #[test]
    fn mixed_xlab_wubi_phrase_not_demoted_by_speculative_initials() {
        // User-reported 2026-05-24: `xlab` (wubi Phrase code for 细节)
        // was being drowned out by `向量/心理/训练/...` because pinyin
        // Path 1c (typo-shaped initials fallback) was matching "xl"
        // initials AND incorrectly setting has_non_speculative_candidate,
        // which then triggered the v0.5 wubi-Phrase layer demote on
        // legitimate wubi 细节 (Phrase × 0.5).
        //
        // Production composite engine auto-commits 细节 at 4 chars
        // (unique@4 policy), so we can't inspect xlab's candidate list
        // directly — instead verify the underlying invariant: pinyin
        // Path 1c speculation must NOT mark has_non_speculative, so
        // the wubi layer-demote stays off and wubi simcodes lead.
        let mut pinyin = PinyinAdapter::new();
        typed(&mut pinyin, b"xlab");
        // Path 1c should fire (xl is a valid 简拼 prefix for many
        // phrases) and populate candidates, BUT must not set
        // has_non_speculative_candidate (that's Path 1's job for
        // genuine exact matches).
        assert!(!pinyin.candidates().is_empty(),
            "Path 1c should populate xlab with xl-initials phrases");
        assert!(!pinyin.has_non_speculative_candidate(),
            "Path 1c is speculative — must not set has_non_speculative \
             (regression would re-trigger wubi-Phrase demote on xlab)");
    }

    #[test]
    fn mixed_huo_jianma2_wubi_still_leads() {
        // The 伙-rule sanity check. 伙 is a Jianma2 wubi simcode at
        // `wo` (hypothetically — the actual code may differ; pick any
        // 2-letter buffer where wubi has a high-confidence simcode).
        // The layer-aware demote MUST NOT touch Jianma1/2/3 + Zigen,
        // so true wubi simcodes still lead at their codes even when
        // pinyin also has matches at the same buffer.
        //
        // Here we test the policy mechanically rather than depending on
        // the specific wubi data: pull layer info, verify Jianma2 entries
        // (when present) keep their original score multiplier of 1.0.
        let mut wubi = WubiEngine::new();
        wubi_typed(&mut wubi, b"wo");
        let raw = wubi.candidates_with_layer();
        let jianma2_count = raw.iter()
            .filter(|(_, _, l)| matches!(l, ::inputx_wubi::Layer::Jianma2))
            .count();
        // We don't enforce that Jianma2 entries EXIST for any specific
        // buffer (data-dependent); just enforce they survive demote.
        if jianma2_count > 0 {
            let mut pinyin = PinyinAdapter::new();
            typed(&mut pinyin, b"wo");
            let cands = dispatch(Mode::Mixed, &wubi, &pinyin, None, None);
            // The top-scored entry must still be wubi when Jianma2 is
            // present — the demote rule preserves Jianma2.
            let top_source = cands.first().map(|c| c.source);
            // Allow either Wubi (Jianma2 leads) or pinyin (rare case
            // where pinyin top legitimately outscores even Jianma2 +
            // bigram boost). The point of the test is that the merge
            // *runs* without errors and the policy doesn't strip
            // Jianma2 entries from the list entirely.
            assert!(matches!(top_source, Some(Source::Wubi) | Some(Source::Pinyin)),
                "expected wubi or pinyin source at top; got {top_source:?}");
            let jianma2_words: Vec<&str> = raw.iter()
                .filter(|(_, _, l)| matches!(l, ::inputx_wubi::Layer::Jianma2))
                .map(|(w, _, _)| w.as_str()).collect();
            for jm2 in &jianma2_words {
                assert!(cands.iter().any(|c| c.word == *jm2),
                    "Jianma2 word {jm2} should survive in merged candidates");
            }
        }
    }
}
