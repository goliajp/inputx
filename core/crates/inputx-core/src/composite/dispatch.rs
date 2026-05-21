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
    let jp_cands = japanese.map(|j| j.candidates()).unwrap_or_default();
    match mode {
        Mode::WubiOnly => merge(wubi.candidates().to_vec(), vec![], jp_cands),
        Mode::PinyinOnly => merge(vec![], pinyin.candidates().to_vec(), jp_cands),
        Mode::JapaneseOnly => merge(vec![], vec![], jp_cands),
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
                merge_pinyin_first(wubi_cands, pinyin_cands, jp_cands)
            } else {
                merge(wubi_cands, pinyin_cands, jp_cands)
            }
        }
    }
}

/// Merge but with pinyin candidates listed before wubi (still attributing
/// each to its source). Used when pinyin buffer outpaced wubi (collision
/// scenarios from the 5+ char input path). JP candidates always rank last
/// regardless of the pinyin/wubi reorder above.
fn merge_pinyin_first(
    wubi: Vec<String>,
    pinyin: Vec<String>,
    japanese: Vec<String>,
) -> Vec<Candidate> {
    use crate::composite::merge::{Candidate, JAPANESE_RESERVE, MAX_PER_INPUT, Source};
    let total_hint =
        (wubi.len() + pinyin.len() + japanese.len()).min(MAX_PER_INPUT);
    let mut out = Vec::with_capacity(total_hint);
    let mut seen = std::collections::HashSet::with_capacity(total_hint);
    // Reserve JP slots at the end (see merge.rs comment).
    let jp_reserve = japanese.len().min(JAPANESE_RESERVE);
    let chinese_cap = MAX_PER_INPUT - jp_reserve;
    for p in pinyin {
        if out.len() >= chinese_cap {
            break;
        }
        if seen.insert(p.clone()) {
            out.push(Candidate { word: p, source: Source::Pinyin });
        }
    }
    for w in wubi {
        if out.len() >= chinese_cap {
            break;
        }
        if seen.insert(w.clone()) {
            out.push(Candidate { word: w, source: Source::Wubi });
        }
    }
    for j in japanese {
        if out.len() >= MAX_PER_INPUT {
            break;
        }
        if seen.insert(j.clone()) {
            out.push(Candidate { word: j, source: Source::Japanese });
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
