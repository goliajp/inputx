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

/// Slots reserved at the *end* of the merged list for the Japanese
/// plugin when it has candidates. Without this, common pinyin inputs
/// (e.g. `ka` → 50+ 卡/咖/喀/… candidates) would fill the cap before
/// JP ever got a turn, and the toggle would feel broken.
pub const JAPANESE_RESERVE: usize = 8;

/// Merge wubi + pinyin + japanese candidate lists.
///
/// Ranking — wubi strictly first (deliberate user intent), pinyin next,
/// japanese last. JP appended only when the caller passes a non-empty
/// slice (engine disables JP by handing in an empty vec). Duplicates by
/// `word` are dropped on second occurrence (keeps the first-seen source
/// attribution).
///
/// When JP has candidates, the Chinese-engine portion is capped to
/// `MAX_PER_INPUT - min(jp.len(), JAPANESE_RESERVE)` to guarantee JP
/// visibility. With JP toggle off, the full `MAX_PER_INPUT` is available
/// to wubi+pinyin (backward compatible).
pub fn merge(wubi: Vec<String>, pinyin: Vec<String>, japanese: Vec<String>) -> Vec<Candidate> {
    let total_hint = (wubi.len() + pinyin.len() + japanese.len()).min(MAX_PER_INPUT);
    let mut out = Vec::with_capacity(total_hint);
    let mut seen = std::collections::HashSet::with_capacity(total_hint);

    let jp_reserve = japanese.len().min(JAPANESE_RESERVE);
    let chinese_cap = MAX_PER_INPUT - jp_reserve;

    for w in wubi {
        if out.len() >= chinese_cap {
            break;
        }
        if seen.insert(w.clone()) {
            out.push(Candidate {
                word: w,
                source: Source::Wubi,
            });
        }
    }

    for p in pinyin {
        if out.len() >= chinese_cap {
            break;
        }
        if seen.insert(p.clone()) {
            out.push(Candidate {
                word: p,
                source: Source::Pinyin,
            });
        }
    }

    for j in japanese {
        if out.len() >= MAX_PER_INPUT {
            break;
        }
        if seen.insert(j.clone()) {
            out.push(Candidate {
                word: j,
                source: Source::Japanese,
            });
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_inputs_yield_empty() {
        assert!(merge(vec![], vec![], vec![]).is_empty());
    }

    #[test]
    fn wubi_only_tagged_wubi() {
        let m = merge(vec!["国".into(), "果".into()], vec![], vec![]);
        assert_eq!(m.len(), 2);
        assert!(m.iter().all(|c| c.source == Source::Wubi));
        assert_eq!(m[0].word, "国");
    }

    #[test]
    fn pinyin_only_tagged_pinyin() {
        let m = merge(vec![], vec!["中国".into(), "中过".into()], vec![]);
        assert_eq!(m.len(), 2);
        assert!(m.iter().all(|c| c.source == Source::Pinyin));
    }

    #[test]
    fn japanese_only_tagged_japanese() {
        let m = merge(vec![], vec![], vec!["か".into(), "カ".into(), "高".into()]);
        assert_eq!(m.len(), 3);
        assert!(m.iter().all(|c| c.source == Source::Japanese));
    }

    #[test]
    fn ranking_wubi_pinyin_japanese() {
        let m = merge(vec!["W".into()], vec!["P".into()], vec!["J".into()]);
        assert_eq!(m[0].source, Source::Wubi);
        assert_eq!(m[1].source, Source::Pinyin);
        assert_eq!(m[2].source, Source::Japanese);
    }

    #[test]
    fn duplicate_words_keep_first_source() {
        let m = merge(
            vec!["X".into(), "Y".into()],
            vec!["Y".into(), "Z".into()],
            vec!["Z".into(), "W".into()],
        );
        // X (W), Y (W via dedup), Z (P via dedup), W (J)
        assert_eq!(m.len(), 4);
        assert_eq!(m[0].source, Source::Wubi);
        assert_eq!(m[1].source, Source::Wubi);
        assert_eq!(m[2].source, Source::Pinyin);
        assert_eq!(m[3].source, Source::Japanese);
    }

    #[test]
    fn cap_at_max_per_input() {
        let many: Vec<String> = (0..MAX_PER_INPUT * 2).map(|i| i.to_string()).collect();
        let m = merge(many.clone(), many.clone(), many);
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
