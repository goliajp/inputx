//! Phase 1 data tables — char + reading layer from authoritative sources.
//!
//! Embedded via `include_str!` so the crate has no runtime fs dependency.
//! Sources documented in `docs/pinyin-char-centric-rewrite-2026-06-29/PLAN.md`
//! and regeneratable via `tools/v2-ingest/build-chars-readings.py`.

use std::sync::OnceLock;

const CHARS_TSV: &str = include_str!("../data/chars.tsv");
const READINGS_TSV: &str = include_str!("../data/readings.tsv");
const WORDS_TSV: &str = include_str!("../data/words.tsv");

// Polish overlay files. Same files v1 consumes, so polish edits apply
// uniformly to v1 and v2 (per [[ranking-orthogonal-table-model]]:
// per-(buffer, word) overrides are the only sanctioned data surface).
const TIER_OVERLAY_TSV: &str = include_str!("../../../../tools/scoring/data/polish/tier_overlay.tsv");
const QUICKFIX_BOOST_TSV: &str = include_str!("../../../../tools/scoring/data/polish/quickfix_boost.tsv");
const EXCLUSIONS_TSV: &str = include_str!("../../../../tools/scoring/data/polish/exclusions_v1.tsv");
const PRIOR_CORRECTIONS_TSV: &str = include_str!("../../../../tools/scoring/data/polish/prior_corrections_v1.tsv");

/// One row of `chars.tsv`.
#[derive(Debug, Clone)]
pub struct CharEntry {
    pub ch: char,
    pub codepoint: u32,
    /// 1 = 通用规范汉字表 一级 (常用 3500),
    /// 2 = 二级 (3000), 3 = 三级 (1605).
    pub tier: u8,
    /// HSK 2.0 single-char level overlay: 0 = non-HSK, 1-6 = HSK level.
    /// Used by single-char ranking to push muscle-memory chars
    /// (我 / 你 / 好 / 的 ...) above same-tier non-HSK chars.
    pub hsk_level: u8,
    /// `kMandarin_8105.txt` canonical reading (e.g. "yī" for 一).
    pub canonical_reading: String,
}

/// One row of `readings.tsv`.
#[derive(Debug, Clone)]
pub struct ReadingEntry {
    pub ch: char,
    pub reading: String,
    pub rank: ReadingRank,
    /// Added to the char's tier when this reading is used to surface
    /// a word — primary = 0, others = +1.
    pub tier_offset: i8,
    /// Source TSV column (kMandarin / kHanyuPinyin / kXHC1983).
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadingRank {
    /// 通用规范汉字表 + mozillazg/pinyin-data kMandarin_8105 主读.
    Primary,
    /// 汉语大字典 kHanyuPinyin 收录的额外读音 (副读 / 文白异读 …).
    Secondary,
    /// 仅新华字典 1983 kXHC1983 收录,未在 kHanyuPinyin 重出现 —
    /// 补充读音 (rare / supplementary).
    Supplementary,
}

impl ReadingRank {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "primary" => Self::Primary,
            "secondary" => Self::Secondary,
            "supplementary" => Self::Supplementary,
            _ => return None,
        })
    }
}

fn parse_chars_tsv(text: &str) -> Vec<CharEntry> {
    let mut out = Vec::with_capacity(8200);
    for ln in text.lines() {
        if ln.is_empty() || ln.starts_with('#') {
            continue;
        }
        let mut it = ln.split('\t');
        let ch_str = it.next();
        let cp_hex = it.next();
        let tier_s = it.next();
        let hsk_s = it.next();
        let canonical = it.next();
        if let (Some(ch_str), Some(cp_hex), Some(tier_s), Some(hsk_s), Some(canonical)) =
            (ch_str, cp_hex, tier_s, hsk_s, canonical)
        {
            let Some(ch) = ch_str.chars().next() else { continue };
            let Ok(cp) = u32::from_str_radix(cp_hex.trim(), 16) else { continue };
            let Ok(tier) = tier_s.trim().parse::<u8>() else { continue };
            let hsk_level = hsk_s.trim().parse::<u8>().unwrap_or(0);
            out.push(CharEntry {
                ch,
                codepoint: cp,
                tier,
                hsk_level,
                canonical_reading: canonical.trim().to_owned(),
            });
        }
    }
    out
}

fn parse_readings_tsv(text: &str) -> Vec<ReadingEntry> {
    let mut out = Vec::with_capacity(12_500);
    for ln in text.lines() {
        if ln.is_empty() || ln.starts_with('#') {
            continue;
        }
        let mut it = ln.split('\t');
        let ch_str = it.next();
        let reading = it.next();
        let rank_s = it.next();
        let offset_s = it.next();
        let source = it.next();
        if let (Some(ch_str), Some(reading), Some(rank_s), Some(offset_s), Some(source)) =
            (ch_str, reading, rank_s, offset_s, source)
        {
            let Some(ch) = ch_str.chars().next() else { continue };
            let Some(rank) = ReadingRank::parse(rank_s.trim()) else { continue };
            let Ok(offset) = offset_s.trim().parse::<i8>() else { continue };
            out.push(ReadingEntry {
                ch,
                reading: reading.trim().to_owned(),
                rank,
                tier_offset: offset,
                source: source.trim().to_owned(),
            });
        }
    }
    out
}

/// Parsed `chars.tsv` (lazy, cached).
pub fn chars() -> &'static [CharEntry] {
    static CACHED: OnceLock<Vec<CharEntry>> = OnceLock::new();
    CACHED.get_or_init(|| parse_chars_tsv(CHARS_TSV))
}

/// Parsed `readings.tsv` (lazy, cached).
pub fn readings() -> &'static [ReadingEntry] {
    static CACHED: OnceLock<Vec<ReadingEntry>> = OnceLock::new();
    CACHED.get_or_init(|| parse_readings_tsv(READINGS_TSV))
}

/// One row of `words.tsv`.
#[derive(Debug, Clone)]
pub struct WordEntry {
    pub code: String,
    pub word: String,
    /// `[char|reading][char|reading]…` — exact reading-path per char.
    pub reading_path: String,
    pub tier: u8,
    /// `cedict` / `cedict+hsk1` / … / `polish-A` (future).
    pub source: String,
}

fn parse_words_tsv(text: &str) -> Vec<WordEntry> {
    let mut out = Vec::with_capacity(90_000);
    for ln in text.lines() {
        if ln.is_empty() || ln.starts_with('#') {
            continue;
        }
        let mut it = ln.split('\t');
        let code = it.next();
        let word = it.next();
        let rp = it.next();
        let tier_s = it.next();
        let source = it.next();
        if let (Some(code), Some(word), Some(rp), Some(tier_s), Some(source)) =
            (code, word, rp, tier_s, source)
        {
            let Ok(tier) = tier_s.trim().parse::<u8>() else { continue };
            out.push(WordEntry {
                code: code.to_owned(),
                word: word.to_owned(),
                reading_path: rp.to_owned(),
                tier,
                source: source.to_owned(),
            });
        }
    }
    out
}

/// Parsed `words.tsv` (lazy, cached).
pub fn words() -> &'static [WordEntry] {
    static CACHED: OnceLock<Vec<WordEntry>> = OnceLock::new();
    CACHED.get_or_init(|| parse_words_tsv(WORDS_TSV))
}

// ─── Polish overlay loaders ────────────────────────────────────

/// `tier_overlay.tsv` row: (buffer, word) → override_tier.
pub fn tier_overlay() -> &'static std::collections::HashMap<(String, String), u8> {
    use std::collections::HashMap;
    static CACHED: OnceLock<HashMap<(String, String), u8>> = OnceLock::new();
    CACHED.get_or_init(|| {
        let mut m = HashMap::new();
        for ln in TIER_OVERLAY_TSV.lines() {
            if ln.is_empty() || ln.starts_with('#') { continue; }
            let mut it = ln.split('\t');
            let buffer = it.next();
            let word = it.next();
            let tier_s = it.next();
            if let (Some(b), Some(w), Some(t)) = (buffer, word, tier_s) {
                if let Ok(tier) = t.trim().parse::<u8>() {
                    m.insert((b.to_owned(), w.to_owned()), tier);
                }
            }
        }
        m
    })
}

/// `quickfix_boost.tsv` row: (buffer, word) → boost_freq.
pub fn quickfix_boost() -> &'static std::collections::HashMap<(String, String), u32> {
    use std::collections::HashMap;
    static CACHED: OnceLock<HashMap<(String, String), u32>> = OnceLock::new();
    CACHED.get_or_init(|| {
        let mut m = HashMap::new();
        for ln in QUICKFIX_BOOST_TSV.lines() {
            if ln.is_empty() || ln.starts_with('#') { continue; }
            let mut it = ln.split('\t');
            let buffer = it.next();
            let word = it.next();
            let freq_s = it.next();
            if let (Some(b), Some(w), Some(f)) = (buffer, word, freq_s) {
                if let Ok(freq) = f.trim().parse::<u32>() {
                    m.insert((b.to_owned(), w.to_owned()), freq);
                }
            }
        }
        m
    })
}

/// `prior_corrections_v1.tsv` word → Q4 log-prior boost. Applies globally
/// to that word regardless of buffer (lifts e.g. 继续 over 积蓄 anywhere
/// they compete).
pub fn prior_corrections() -> &'static std::collections::HashMap<String, i32> {
    use std::collections::HashMap;
    static CACHED: OnceLock<HashMap<String, i32>> = OnceLock::new();
    CACHED.get_or_init(|| {
        let mut m = HashMap::new();
        for ln in PRIOR_CORRECTIONS_TSV.lines() {
            if ln.is_empty() || ln.starts_with('#') { continue; }
            let mut it = ln.split('\t');
            let word = it.next();
            let boost_s = it.next();
            if let (Some(w), Some(b)) = (word, boost_s) {
                if let Ok(boost) = b.trim().parse::<i32>() {
                    m.insert(w.to_owned(), boost);
                }
            }
        }
        m
    })
}

/// `exclusions_v1.tsv` set of (code, word) — entries hidden from Path-1
/// top display (still kept for K-best / reverse-lookup in v1; in v2
/// they're filtered from `query` output).
pub fn exclusions() -> &'static std::collections::HashSet<(String, String)> {
    use std::collections::HashSet;
    static CACHED: OnceLock<HashSet<(String, String)>> = OnceLock::new();
    CACHED.get_or_init(|| {
        let mut s = HashSet::new();
        for ln in EXCLUSIONS_TSV.lines() {
            if ln.is_empty() || ln.starts_with('#') { continue; }
            let mut it = ln.split('\t');
            let code = it.next();
            let word = it.next();
            if let (Some(c), Some(w)) = (code, word) {
                s.insert((c.to_owned(), w.to_owned()));
            }
        }
        s
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chars_table_has_8105_entries() {
        let cs = chars();
        assert_eq!(cs.len(), 8105, "通用规范汉字表 2013 total = 3500+3000+1605");
    }

    #[test]
    fn chars_tier_distribution() {
        let cs = chars();
        let t1 = cs.iter().filter(|c| c.tier == 1).count();
        let t2 = cs.iter().filter(|c| c.tier == 2).count();
        let t3 = cs.iter().filter(|c| c.tier == 3).count();
        assert_eq!(t1, 3500, "一级");
        assert_eq!(t2, 3000, "二级");
        assert_eq!(t3, 1605, "三级");
    }

    #[test]
    fn chars_canonical_reading_present_for_all() {
        let cs = chars();
        let missing = cs.iter().filter(|c| c.canonical_reading.is_empty()).count();
        assert_eq!(missing, 0, "all 8105 chars must have kMandarin canonical reading");
    }

    #[test]
    fn chars_spotcheck_known_entries() {
        let cs = chars();
        let yi = cs.iter().find(|c| c.ch == '一').unwrap();
        assert_eq!(yi.tier, 1);
        assert_eq!(yi.hsk_level, 1, "一 is HSK 1");
        assert_eq!(yi.canonical_reading, "yī");
        let ding = cs.iter().find(|c| c.ch == '丁').unwrap();
        assert_eq!(ding.tier, 1);
        assert_eq!(ding.hsk_level, 5, "丁 is HSK 5");
        assert_eq!(ding.canonical_reading, "dīng");
    }

    #[test]
    fn hsk_char_overlay_count() {
        let cs = chars();
        let hsk: Vec<&CharEntry> = cs.iter().filter(|c| c.hsk_level > 0).collect();
        // 696 单字 in HSK 1-6 (per ingest log).
        assert_eq!(hsk.len(), 696, "HSK char overlay total");
        // Sanity: each HSK level non-empty.
        for level in 1..=6 {
            let n = hsk.iter().filter(|c| c.hsk_level == level).count();
            assert!(n > 0, "HSK level {level} has no chars in overlay");
        }
    }

    #[test]
    fn readings_table_size_within_expected_band() {
        let rs = readings();
        // Phase-1 ingest produced 12,149 rows.  Hard-pin to the same
        // count so future regenerations that drift are caught.
        assert_eq!(rs.len(), 12_149);
    }

    #[test]
    fn readings_polyphone_chars_count() {
        let rs = readings();
        // Phase-1: 2,712 chars have ≥2 readings.
        use std::collections::HashMap;
        let mut by_char: HashMap<char, usize> = HashMap::new();
        for r in rs {
            *by_char.entry(r.ch).or_insert(0) += 1;
        }
        let polyphone = by_char.values().filter(|&&n| n >= 2).count();
        assert_eq!(polyphone, 2_712);
    }

    #[test]
    fn readings_spotcheck_polyphone() {
        let rs = readings();
        let yi_readings: Vec<&ReadingEntry> = rs.iter().filter(|r| r.ch == '一').collect();
        // 一 should have at least the primary "yī" + a polyphone "yí" (一会儿)
        assert!(yi_readings.iter().any(|r| r.reading == "yī" && r.rank == ReadingRank::Primary));
        assert!(yi_readings.iter().any(|r| r.reading == "yí"));
    }

    #[test]
    fn readings_primary_has_offset_zero() {
        let rs = readings();
        for r in rs.iter().filter(|r| r.rank == ReadingRank::Primary) {
            assert_eq!(r.tier_offset, 0, "primary reading must be tier_offset 0 ({} {})", r.ch, r.reading);
        }
    }

    #[test]
    fn words_table_size_within_expected_band() {
        let ws = words();
        // Phase-2 ingest (post Phase 4 HSK-cap relax) → 88,135 words.
        assert_eq!(ws.len(), 88_135);
    }

    #[test]
    fn words_tier_distribution() {
        let ws = words();
        let mut by_t = [0usize; 10];
        for w in ws { by_t[w.tier as usize] += 1; }
        // Hard pins per ingest output (CC-CEDICT 2026-06-22 + HSK 2.0
        // with capitalized HSK pinyin accepted, e.g. 中国/北京/中文):
        assert_eq!(by_t[1], 150,   "tier 1 (HSK 1-2 multi-char)");
        assert_eq!(by_t[2], 702,   "tier 2 (HSK 3-4 multi-char)");
        assert_eq!(by_t[3], 3493,  "tier 3 (HSK 5-6 multi-char)");
        assert_eq!(by_t[4], 49734, "tier 4 (cedict 2-char non-HSK)");
        assert_eq!(by_t[5], 31357, "tier 5 (cedict 3-4-char non-HSK)");
        assert_eq!(by_t[6], 2699,  "tier 6 (cedict 5+-char non-HSK)");
    }

    #[test]
    fn words_spotcheck_known_entries() {
        let ws = words();
        let nihao: Vec<&WordEntry> = ws.iter().filter(|w| w.word == "你好").collect();
        assert!(!nihao.is_empty());
        let nh = nihao[0];
        assert_eq!(nh.code, "nihao");
        assert_eq!(nh.reading_path, "[你|nǐ][好|hǎo]");
        let xiuxi: Vec<&WordEntry> = ws.iter().filter(|w| w.word == "休息").collect();
        assert!(!xiuxi.is_empty(), "休息 HSK 2 polyphone-neutral case must be in");
        assert_eq!(xiuxi[0].tier, 1, "休息 is HSK 2 → tier 1");
    }

    #[test]
    fn words_reading_path_has_brackets_for_every_char() {
        let ws = words();
        for w in ws.iter().take(1000) {
            let bracket_count = w.reading_path.matches('|').count();
            let char_count = w.word.chars().count();
            assert_eq!(bracket_count, char_count,
                "reading_path must have one |-separator per char (word={}, path={})",
                w.word, w.reading_path);
        }
    }
}
