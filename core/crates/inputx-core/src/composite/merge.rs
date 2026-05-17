//! Candidate merge — combine wubi + pinyin candidate lists with source
//! attribution and dedup.

/// Engine that produced a candidate. Surfaced to the iOS UI for the W/P
/// indicator dot; FFI returns this as `u8`.
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Wubi = 0,
    Pinyin = 1,
}

impl Source {
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Wubi),
            1 => Some(Self::Pinyin),
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

/// Merge wubi + pinyin candidate lists.
///
/// Wubi candidates strictly first within the merged list (they're typically
/// more deliberate user intent — explicit 4-letter codes); pinyin
/// candidates appended. Duplicates by `word` are dropped on second
/// occurrence (keeps the first-seen source attribution).
pub fn merge(wubi: Vec<String>, pinyin: Vec<String>) -> Vec<Candidate> {
    let mut out = Vec::with_capacity((wubi.len() + pinyin.len()).min(MAX_PER_INPUT));
    let mut seen = std::collections::HashSet::with_capacity(out.capacity());

    for w in wubi {
        if out.len() >= MAX_PER_INPUT {
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
        if out.len() >= MAX_PER_INPUT {
            break;
        }
        if seen.insert(p.clone()) {
            out.push(Candidate {
                word: p,
                source: Source::Pinyin,
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
        assert!(merge(vec![], vec![]).is_empty());
    }

    #[test]
    fn wubi_only_tagged_wubi() {
        let m = merge(vec!["国".into(), "果".into()], vec![]);
        assert_eq!(m.len(), 2);
        assert!(m.iter().all(|c| c.source == Source::Wubi));
        assert_eq!(m[0].word, "国");
    }

    #[test]
    fn pinyin_only_tagged_pinyin() {
        let m = merge(vec![], vec!["中国".into(), "中过".into()]);
        assert_eq!(m.len(), 2);
        assert!(m.iter().all(|c| c.source == Source::Pinyin));
    }

    #[test]
    fn wubi_strictly_first_in_mixed() {
        let m = merge(vec!["A".into()], vec!["B".into()]);
        assert_eq!(m[0].word, "A");
        assert_eq!(m[0].source, Source::Wubi);
        assert_eq!(m[1].word, "B");
        assert_eq!(m[1].source, Source::Pinyin);
    }

    #[test]
    fn duplicate_words_keep_first_source() {
        let m = merge(vec!["X".into(), "Y".into()], vec!["Y".into(), "Z".into()]);
        // X (W), Y (W), Z (P) — Y dedup keeps wubi tag.
        assert_eq!(m.len(), 3);
        assert_eq!(
            m[0],
            Candidate {
                word: "X".into(),
                source: Source::Wubi
            }
        );
        assert_eq!(
            m[1],
            Candidate {
                word: "Y".into(),
                source: Source::Wubi
            }
        );
        assert_eq!(
            m[2],
            Candidate {
                word: "Z".into(),
                source: Source::Pinyin
            }
        );
    }

    #[test]
    fn cap_at_max_per_input() {
        let many: Vec<String> = (0..MAX_PER_INPUT * 2).map(|i| i.to_string()).collect();
        let m = merge(many.clone(), many);
        assert_eq!(m.len(), MAX_PER_INPUT);
    }

    #[test]
    fn source_round_trip_u8() {
        assert_eq!(Source::Wubi.as_u8(), 0);
        assert_eq!(Source::Pinyin.as_u8(), 1);
    }
}
