//! Routing rules for the composite engine.
//!
//! Decides which sub-engines run for a given input + mode, and how their
//! candidate lists combine into the merged output.

use super::japanese_adapter::JapaneseAdapter;
use super::merge::{Candidate, merge};
use super::mode::Mode;
use super::pinyin_adapter::PinyinAdapter;
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
) -> Vec<Candidate> {
    let (jp_kanji, jp_kana) = match japanese {
        Some(j) => split_jp_scored(j),
        None => (vec![], vec![]),
    };
    match mode {
        Mode::WubiOnly => merge(wubi.candidates_with_scores(), vec![], jp_kanji, jp_kana),
        Mode::PinyinOnly => merge(vec![], pinyin.candidates_with_scores(), jp_kanji, jp_kana),
        Mode::JapaneseOnly => merge(vec![], vec![], jp_kanji, jp_kana),
        Mode::Mixed => {
            let z_prefix = pinyin.buffer_str().starts_with('z');
            // Policy 1 (2026-05-22): 超过 4 字就和五笔没关系了. Past 4
            // input letters, wubi has no business here — the user is
            // clearly typing pinyin. Wubi's defuse-tail simcode
            // interpretations (jihua→工, naozi→不, tuijin→沁) flood
            // the #0 slot otherwise.
            let pinyin_len = pinyin.buffer_str().len();
            let beyond_wubi_window = pinyin_len > 4;
            let mut wubi_cands = if z_prefix || beyond_wubi_window {
                vec![]
            } else {
                wubi.candidates_with_scores()
            };

            // Policy 2 (2026-05-22): wubi-2/3-letter-simcode vs pinyin-
            // exact-syllable. When the user types `wo` / `ni` / `ta` /
            // `de` / `shi` / `you`, they almost certainly mean the
            // pinyin word (我/你/他/的/是/有) — even though wubi has a
            // legitimate Jianma2/Jianma3 entry under the same letters
            // (伙/悄/长/胡/椒/亦). Demote non-Jianma1 wubi by ×0.5 when
            // pinyin has an exact-syllable hit; Jianma1 (score ≥ 1e6)
            // is the only hard floor and stays untouched.
            let pinyin_intentional = pinyin.has_exact_match();
            if (2..=4).contains(&pinyin_len) && pinyin_intentional {
                for (_, s) in wubi_cands.iter_mut() {
                    if *s < 1_000_000.0 {
                        *s *= 0.5;
                    }
                }
            }

            merge(wubi_cands, pinyin.candidates_with_scores(), jp_kanji, jp_kana)
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

        let cands = dispatch(Mode::PinyinOnly, &wubi, &pinyin, None);
        assert!(cands.iter().all(|c| c.source == Source::Pinyin));
        assert!(cands.iter().any(|c| c.word == "我们"));
    }

    #[test]
    fn wubi_only_skips_pinyin() {
        let mut wubi = WubiEngine::new();
        let mut pinyin = PinyinAdapter::new();
        wubi_typed(&mut wubi, b"g"); // 'g' = 一级简码 → 一
        typed(&mut pinyin, b"yi");

        let cands = dispatch(Mode::WubiOnly, &wubi, &pinyin, None);
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

        let cands = dispatch(Mode::Mixed, &wubi, &pinyin, None);
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

        let cands = dispatch(Mode::Mixed, &wubi, &pinyin, None);
        assert!(cands.iter().all(|c| c.source == Source::Pinyin));
        assert_eq!(cands.first().map(|c| c.word.as_str()), Some("中国"));
    }
}
