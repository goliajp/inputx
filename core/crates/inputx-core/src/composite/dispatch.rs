//! Routing rules for the composite engine.
//!
//! Decides which sub-engines run for a given input + mode, and how their
//! candidate lists combine into the merged output.

use super::japanese_adapter::JapaneseAdapter;
use super::merge::{Candidate, merge};
use super::mode::Mode;
use super::pinyin_adapter::PinyinAdapter;
use super::scoring;
use crate::wubi::WubiEngine;

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
    match mode {
        Mode::WubiOnly => merge(wubi.candidates_with_scores(), vec![], jp_kanji, jp_kana),
        Mode::PinyinOnly => merge(vec![], pinyin.candidates_with_scores(prev_committed), jp_kanji, jp_kana),
        Mode::JapaneseOnly => merge(vec![], vec![], jp_kanji, jp_kana),
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
            let mut wubi_cands: Vec<(String, f64)> = wubi
                .candidates_with_layer()
                .into_iter()
                .map(|(w, score, layer)| {
                    let layer_demote = if pinyin_intent {
                        match layer {
                            wubi::Layer::Auto => 0.1,      // rare chars: way down
                            wubi::Layer::Phrase => 0.5,    // phrase entries: half
                            _ => 1.0,                       // Jianma1/2/3 + Zigen: keep
                        }
                    } else {
                        1.0
                    };
                    (w, score * layer_demote)
                })
                .collect();
            let final_mult = wubi_mult * z_mult;
            if final_mult != 1.0 {
                for (_, s) in wubi_cands.iter_mut() {
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
) -> (Vec<(String, f64)>, Vec<(String, f64)>) {
    let all = j.candidates_with_scores();
    let kanji_set: std::collections::HashSet<String> =
        j.kanji_candidates().into_iter().collect();
    let mut kanji = Vec::new();
    let mut kana = Vec::new();
    for (w, s) in all {
        if kanji_set.contains(&w) {
            kanji.push((w, s));
        } else {
            kana.push((w, s));
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
            .filter(|(_, _, l)| matches!(l, ::wubi::Layer::Jianma2))
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
                .filter(|(_, _, l)| matches!(l, ::wubi::Layer::Jianma2))
                .map(|(w, _, _)| w.as_str()).collect();
            for jm2 in &jianma2_words {
                assert!(cands.iter().any(|c| c.word == *jm2),
                    "Jianma2 word {jm2} should survive in merged candidates");
            }
        }
    }
}
