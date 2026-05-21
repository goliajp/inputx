//! Candidate merge — combine wubi + pinyin + (optionally) Japanese
//! candidate lists with source attribution and dedup.

/// Engine that produced a candidate. Surfaced to the iOS UI for the
/// W/P/J indicator dot; FFI returns this as `u8`.
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Wubi = 0,
    Pinyin = 1,
    Japanese = 2,
}

impl Source {
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Wubi),
            1 => Some(Self::Pinyin),
            2 => Some(Self::Japanese),
            _ => None,
        }
    }
}

/// One candidate with its source engine. The composite session exposes
/// `Vec<Candidate>` to the host; FFI splits into parallel arrays.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub word: String,
    pub source: Source,
}

/// Maximum candidates retained in the merged list. iOS candidate bar
/// paginates via swipe (item 55), but capping prevents pathological
/// inputs from blowing memory.
pub const MAX_PER_INPUT: usize = 50;

/// Slots reserved at the *end* of the merged list for low-conviction
/// JP kana candidates (always-available fallback regardless of whether
/// the buffer is a known JP word). 4 is enough for hiragana + katakana
/// + maybe small/voiced variants without crowding out pinyin bulk.
pub const JP_KANA_RESERVE: usize = 4;

/// Merge wubi + JP kanji + pinyin + JP kana into a single candidate list.
///
/// # Current ordering rule (Mixed mode, JP-enabled)
///
///   1. **Wubi** — the product is "Inputx 五笔"; wubi outputs lead.
///      Within wubi, jianma1 / jianma2 layer_base dominates per the
///      wubi dict's existing sort, so 一级 / 二级简码 always top.
///   2. **JP kanji** (jukugo compounds + on/kun single-kanji matches) —
///      full-buffer matches only, sits above pinyin bulk so common JP
///      words like `nihon` → 日本 don't get drowned in pinyin fuzz.
///   3. **Pinyin** — Chinese fuzzy / phonetic matches.
///   4. **JP kana** — hiragana + katakana renderings of whatever romaji
///      the user typed. Mechanically derivable from any buffer,
///      low-conviction; reserved last so pinyin bulk stays visible.
///
/// # Design note: future "unified score" v0.2 ↓ direction
///
/// The user's principle is **"公允的同数值化比较"** — a single normalized
/// score across all engines, sort by that, with wubi getting a
/// brand-loyalty boost (since "Inputx 五笔"). Today's hard layered
/// merge (wubi → JP kanji → pinyin → JP kana) is a *placeholder* for
/// that:
///
///   - Each engine produces (word, freq) with engine-specific scales
///     (wubi 0-50k, pinyin similar, JP currently has none).
///   - To unify: normalize freq to [0, 1] per engine (percentile or
///     log-rank), apply per-engine multiplier (wubi 1.2, pinyin 1.0,
///     JP 0.9, etc.), sort by `score = normalized * multiplier`.
///   - Wubi 一级 / 二级简码 floors stay enforced via layer_base in
///     the wubi dict (already the case) — they'd land on top of the
///     unified score by virtue of having highest absolute freq + the
///     wubi multiplier.
///
/// Not implemented yet because: (a) the JP plugin has no real freq
/// data (hand-curated tables, all entries weighted equally), (b)
/// cross-engine normalization is a calibration project that wants
/// the polish-log corpus to validate against. See PolishLog telemetry
/// on mac/iOS for the data-collection side.
///
/// Duplicates by `word` keep the first-seen source attribution.
///
/// Whichever vec the caller hands in empty (e.g. JP toggle off → both
/// jp_kanji and jp_kana empty) is a no-op for that group.
pub fn merge(
    wubi: Vec<String>,
    jp_kanji: Vec<String>,
    pinyin: Vec<String>,
    jp_kana: Vec<String>,
) -> Vec<Candidate> {
    let total_hint = (wubi.len() + jp_kanji.len() + pinyin.len() + jp_kana.len())
        .min(MAX_PER_INPUT);
    let mut out = Vec::with_capacity(total_hint);
    let mut seen = std::collections::HashSet::with_capacity(total_hint);

    // Reserve tail slots for jp_kana so the bulk of pinyin doesn't push
    // kana off the visible cap.
    let kana_reserve = jp_kana.len().min(JP_KANA_RESERVE);
    let main_cap = MAX_PER_INPUT.saturating_sub(kana_reserve);

    // HARD RULE: wubi outputs lead. Inputx-五笔 brand promise — wubi
    // 一级简码 / 二级简码 are non-negotiable top hits for their codes
    // (e → 有, go → 来, etc.). Within wubi the dict-internal layer_base
    // sort already enforces 简码 > 词组 > Auto.
    for w in wubi {
        if out.len() >= main_cap { break; }
        if seen.insert(w.clone()) {
            out.push(Candidate { word: w, source: Source::Wubi });
        }
    }
    for k in jp_kanji {
        if out.len() >= main_cap { break; }
        if seen.insert(k.clone()) {
            out.push(Candidate { word: k, source: Source::Japanese });
        }
    }
    for p in pinyin {
        if out.len() >= main_cap { break; }
        if seen.insert(p.clone()) {
            out.push(Candidate { word: p, source: Source::Pinyin });
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

    #[test]
    fn empty_inputs_yield_empty() {
        assert!(merge(vec![], vec![], vec![], vec![]).is_empty());
    }

    #[test]
    fn wubi_only_tagged_wubi() {
        let m = merge(vec!["国".into(), "果".into()], vec![], vec![], vec![]);
        assert_eq!(m.len(), 2);
        assert!(m.iter().all(|c| c.source == Source::Wubi));
        assert_eq!(m[0].word, "国");
    }

    #[test]
    fn pinyin_only_tagged_pinyin() {
        let m = merge(vec![], vec![], vec!["中国".into(), "中过".into()], vec![]);
        assert_eq!(m.len(), 2);
        assert!(m.iter().all(|c| c.source == Source::Pinyin));
    }

    #[test]
    fn wubi_strictly_first_in_merged_list() {
        // Hard rule: Inputx-五笔 brand promise. Wubi 简码 / 字根 outputs
        // lead even when JP has a high-conviction kanji match. Within
        // wubi, dict-internal layer_base ordering handles 一级 > 二级 >
        // 三级 > 字根 > 词组 > Auto.
        let m = merge(
            vec!["有".into()],
            vec!["会".into()],  // JP 会 (on-yomi "e") would match `e` too
            vec![],
            vec![],
        );
        assert_eq!(m[0].word, "有");
        assert_eq!(m[0].source, Source::Wubi);
        assert_eq!(m[1].word, "会");
        assert_eq!(m[1].source, Source::Japanese);
    }

    #[test]
    fn ranking_wubi_jpkanji_pinyin_jpkana() {
        let m = merge(
            vec!["W".into()],
            vec!["K".into()],
            vec!["P".into()],
            vec!["N".into()],
        );
        assert_eq!(m[0].source, Source::Wubi);
        assert_eq!(m[1].source, Source::Japanese);
        assert_eq!(m[1].word, "K");
        assert_eq!(m[2].source, Source::Pinyin);
        assert_eq!(m[3].source, Source::Japanese);
        assert_eq!(m[3].word, "N");
    }

    #[test]
    fn jp_kanji_empty_falls_through_to_wubi_first() {
        // JP off (kanji empty) → behaves like before: wubi → pinyin.
        let m = merge(
            vec!["中国".into()],
            vec![],
            vec!["zhongguo".into()],
            vec![],
        );
        assert_eq!(m[0].word, "中国");
        assert_eq!(m[0].source, Source::Wubi);
    }

    #[test]
    fn cap_at_max_per_input() {
        let many: Vec<String> = (0..MAX_PER_INPUT * 2).map(|i| i.to_string()).collect();
        let m = merge(many.clone(), many.clone(), many.clone(), many);
        assert_eq!(m.len(), MAX_PER_INPUT);
    }

    #[test]
    fn source_round_trip_u8() {
        assert_eq!(Source::Wubi.as_u8(), 0);
        assert_eq!(Source::Pinyin.as_u8(), 1);
        assert_eq!(Source::Japanese.as_u8(), 2);
        assert_eq!(Source::from_u8(0), Some(Source::Wubi));
        assert_eq!(Source::from_u8(1), Some(Source::Pinyin));
        assert_eq!(Source::from_u8(2), Some(Source::Japanese));
        assert_eq!(Source::from_u8(99), None);
    }
}
