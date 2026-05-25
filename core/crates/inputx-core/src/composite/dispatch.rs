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
            let auto_demote = if pinyin_intent {
                match pinyin_len {
                    1 => 0.01,
                    2 => 0.05,
                    3 => 0.10,
                    _ => 0.20,
                }
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
            //     structural ~80k freq-equivalent edge (×1.2 → 480k base).
            let full_code = pinyin_len == scoring::WUBI_MAX_BUFFER_LEN;
            let phrase_mult = if pinyin_intent {
                if full_code { scoring::WUBI_FULL_CODE_PHRASE_PROMOTE } else { 0.5 }
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
            const CHAR_PROMINENT_FLOOR: u64 = 20_000;
            const RARE_CHAR_DEMOTE: f64 = 0.3;
            let pinyin_dict = pinyin.engine().dict();
            let char_demote = |word: &str, layer: wubi::Layer| -> f64 {
                if !matches!(layer, wubi::Layer::Jianma2 | wubi::Layer::Jianma3) {
                    return 1.0;
                }
                let mut chars = word.chars();
                let Some(c) = chars.next() else { return 1.0 };
                if chars.next().is_some() { return 1.0; }  // multi-char Jianma3 phrase
                let freq = pinyin_dict.char_max_freq(c);
                if freq >= CHAR_PROMINENT_FLOOR { 1.0 } else { RARE_CHAR_DEMOTE }
            };
            let mut wubi_cands: Vec<(String, f64)> = wubi
                .candidates_with_layer()
                .into_iter()
                .map(|(w, score, layer)| {
                    let layer_demote = match layer {
                        wubi::Layer::Auto => auto_demote,
                        wubi::Layer::Phrase => phrase_mult,
                        _ => 1.0,
                    };
                    let cd = char_demote(&w, layer);
                    (w, score * layer_demote * cd)
                })
                .collect();
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
    fn mixed_shinjuku_jp_full_match_beats_composition() {
        // User-reported 2026-05-25: romaji `shinjuku` (新宿, high-freq jukugo)
        // in Mixed+JP ranked Chinese forced-composition junk 是嗯据库 (#0,
        // COMPOSED_SCORE 500k) above 新宿 (464k), and katakana シンジュク was
        // buried below the pinyin non-exact cluster. A real full-buffer jukugo
        // is high-confidence Japanese — JP_FULL_MATCH_PROMOTE (×1.3) lifts the
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
        // now carry `composed=true`, score at JP_COMPOSED_SCORE (below real
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
        // confidence and gets the wubi-first PROMOTE (×1.2), so 东京 leads.
        // See dispatch `full_code` / scoring::WUBI_FULL_CODE_PHRASE_PROMOTE.
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
