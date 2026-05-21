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
/// Ranking rationale:
///   1. **Wubi** — deliberate user 字根 codes, top priority always.
///   2. **JP kanji** (jukugo compounds + on/kun single-kanji matches) —
///      promoted ABOVE pinyin because when JP is toggled on AND the
///      buffer resolves to a known kanji form, the user's intent is
///      clearly Japanese. Without this rank-boost, common JP words
///      (yama → 山, nihon → 日本) land below pinyin's fuzzy guesses
///      ("yama" → 亚麻色 / 牙买加 / …) which is visually buried.
///   3. **Pinyin** — Chinese fuzzy / phonetic matches, the bulk.
///   4. **JP kana** — hiragana + katakana renderings of whatever romaji
///      the user typed. Always present (mechanically derivable from any
///      buffer), low-conviction; reserved last so the bulk of pinyin
///      stays visible, but the kana form is always reachable.
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
    fn jp_kanji_appears_before_pinyin() {
        let m = merge(
            vec![],
            vec!["山".into()],
            vec!["亚麻色".into(), "牙买加".into()],
            vec!["やま".into(), "ヤマ".into()],
        );
        // Expected order: 山 (JP kanji), pinyin entries, やま, ヤマ.
        assert_eq!(m[0].word, "山");
        assert_eq!(m[0].source, Source::Japanese);
        assert_eq!(m[1].source, Source::Pinyin);
        assert_eq!(m[2].source, Source::Pinyin);
        // Kana lands at tail
        assert_eq!(m[3].word, "やま");
        assert_eq!(m[3].source, Source::Japanese);
        assert_eq!(m[4].word, "ヤマ");
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
