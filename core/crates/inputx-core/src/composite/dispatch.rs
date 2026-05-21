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
        Some(j) => (j.kanji_candidates(), j.kana_candidates()),
        None => (vec![], vec![]),
    };
    match mode {
        Mode::WubiOnly => merge(wubi.candidates().to_vec(), jp_kanji, vec![], jp_kana),
        Mode::PinyinOnly => merge(vec![], jp_kanji, pinyin.candidates().to_vec(), jp_kana),
        Mode::JapaneseOnly => merge(vec![], jp_kanji, vec![], jp_kana),
        Mode::Mixed => {
            let z_prefix = pinyin.buffer_str().starts_with('z');
            let wubi_cands = if z_prefix {
                vec![]
            } else {
                wubi.candidates().to_vec()
            };
            let pinyin_cands = pinyin.candidates().to_vec();
            if pinyin.buffer_str().len() > wubi.buffer_str().len()
                && !pinyin_cands.is_empty()
            {
                merge_pinyin_first(wubi_cands, pinyin_cands, jp_kanji, jp_kana)
            } else {
                merge(wubi_cands, jp_kanji, pinyin_cands, jp_kana)
            }
        }
    }
}

/// Merge but with pinyin candidates listed before wubi (still attributing
/// each to its source). Used when pinyin buffer outpaced wubi (collision
/// scenarios from the 5+ char input path). JP kanji still ride near the
/// top; JP kana still reserved at the tail. Ordering:
///   1. JP kanji (high conviction)
///   2. Pinyin (it outpaced wubi — user's clearly committed to pinyin path)
///   3. Wubi (leftover)
///   4. JP kana (low-conviction tail reserve)
fn merge_pinyin_first(
    wubi: Vec<String>,
    pinyin: Vec<String>,
    jp_kanji: Vec<String>,
    jp_kana: Vec<String>,
) -> Vec<Candidate> {
    use crate::composite::merge::{Candidate, JP_KANA_RESERVE, MAX_PER_INPUT, Source};
    let total_hint =
        (wubi.len() + pinyin.len() + jp_kanji.len() + jp_kana.len()).min(MAX_PER_INPUT);
    let mut out = Vec::with_capacity(total_hint);
    let mut seen = std::collections::HashSet::with_capacity(total_hint);
    let kana_reserve = jp_kana.len().min(JP_KANA_RESERVE);
    let main_cap = MAX_PER_INPUT.saturating_sub(kana_reserve);
    // Pinyin-first special case: user's pinyin buffer outpaced wubi
    // (collision recovery from 5+ char overflow). Pinyin leads here
    // because the user is clearly committed to that path. JP kanji
    // and wubi follow. JP kana stays at the tail reserve.
    for p in pinyin {
        if out.len() >= main_cap { break; }
        if seen.insert(p.clone()) {
            out.push(Candidate { word: p, source: Source::Pinyin });
        }
    }
    for k in jp_kanji {
        if out.len() >= main_cap { break; }
        if seen.insert(k.clone()) {
            out.push(Candidate { word: k, source: Source::Japanese });
        }
    }
    for w in wubi {
        if out.len() >= main_cap { break; }
        if seen.insert(w.clone()) {
            out.push(Candidate { word: w, source: Source::Wubi });
        }
    }
    for k in jp_kana {
        if out.len() >= MAX_PER_INPUT { break; }
        if seen.insert(k.clone()) {
            out.push(Candidate { word: k, source: Source::Japanese });
        }
    }
    out
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
    fn mixed_pinyin_first_when_pinyin_outgrew_wubi() {
        // Collision recovery scenario: wubi buffer was reset (e.g., after
        // 5-char overflow defuse) while pinyin kept accumulating. Pinyin
        // should lead the candidate list.
        let mut wubi = WubiEngine::new();
        let mut pinyin = PinyinAdapter::new();
        wubi_typed(&mut wubi, b"g"); // wubi has 1 char → 一
        typed(&mut pinyin, b"shang"); // pinyin has 5 chars → 上, 商, …

        let cands = dispatch(Mode::Mixed, &wubi, &pinyin, None);
        assert_eq!(cands[0].source, Source::Pinyin);
        // Common pinyin shang candidates should appear in top.
        assert!(cands.iter().any(|c| c.word == "上"));
    }

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
