//! Phase 1 data tables — char + reading layer from authoritative sources.
//!
//! Embedded via `include_str!` so the crate has no runtime fs dependency.
//! Sources documented in `docs/pinyin-char-centric-rewrite-2026-06-29/PLAN.md`
//! and regeneratable via `tools/v2-ingest/build-chars-readings.py`.

use std::sync::{Arc, OnceLock};

use arc_swap::ArcSwap;

const CHARS_TSV: &str = include_str!("../data/chars.tsv");
const READINGS_TSV: &str = include_str!("../data/readings.tsv");
const WORDS_TSV: &str = include_str!("../data/words.tsv");
const MODERN_FREQ_TSV: &str = include_str!("../data/modern_freq.tsv");

// Polish overlay files. Same files v1 consumes, so polish edits apply
// uniformly to v1 and v2 (per [[ranking-orthogonal-table-model]]:
// per-(buffer, word) overrides are the only sanctioned data surface).
//
// v1.16 hot-reload: these bytes live behind `ArcSwap<String>` so a
// SIGUSR1 signal can replace them at runtime and the derived caches
// (`words()` / `tier_overlay()` / `quickfix_boost()` / `exclusions()`
// / `prior_corrections()`) rebuild against the new bytes without a
// binary swap. Cold path still starts from the compile-time embedded
// content so tests + probe binaries have zero setup.
const EMBEDDED_TIER_OVERLAY_TSV: &str =
    include_str!("../../../../tools/scoring/data/polish/tier_overlay.tsv");
const EMBEDDED_QUICKFIX_BOOST_TSV: &str =
    include_str!("../../../../tools/scoring/data/polish/quickfix_boost.tsv");
const EMBEDDED_EXCLUSIONS_TSV: &str =
    include_str!("../../../../tools/scoring/data/polish/exclusions_v1.tsv");
const EMBEDDED_PRIOR_CORRECTIONS_TSV: &str =
    include_str!("../../../../tools/scoring/data/polish/prior_corrections_v1.tsv");
const EMBEDDED_MODERN_VOCAB_TSV: &str =
    include_str!("../../../../tools/scoring/data/polish/modern_vocab_v1.tsv");
const EMBEDDED_CORPUS_GARBAGE_FILTER_TSV: &str =
    include_str!("../../../../tools/scoring/data/polish/corpus_garbage_filter_v1.tsv");

// Version counter incremented on every polish-TSV swap. Derived-cache
// slots below store `(version, Arc<data>)` and rebuild lazily when
// they see a newer version. Cheap `Ordering::Relaxed` compare on the
// hot lookup path.
static POLISH_VERSION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn bump_polish_version() {
    POLISH_VERSION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

fn current_polish_version() -> u64 {
    POLISH_VERSION.load(std::sync::atomic::Ordering::Relaxed)
}

fn polish_tsv_slot(embedded: &'static str) -> ArcSwap<String> {
    ArcSwap::from_pointee(embedded.to_owned())
}

fn tier_overlay_tsv_slot() -> &'static ArcSwap<String> {
    static SLOT: OnceLock<ArcSwap<String>> = OnceLock::new();
    SLOT.get_or_init(|| polish_tsv_slot(EMBEDDED_TIER_OVERLAY_TSV))
}

fn quickfix_boost_tsv_slot() -> &'static ArcSwap<String> {
    static SLOT: OnceLock<ArcSwap<String>> = OnceLock::new();
    SLOT.get_or_init(|| polish_tsv_slot(EMBEDDED_QUICKFIX_BOOST_TSV))
}

fn exclusions_tsv_slot() -> &'static ArcSwap<String> {
    static SLOT: OnceLock<ArcSwap<String>> = OnceLock::new();
    SLOT.get_or_init(|| polish_tsv_slot(EMBEDDED_EXCLUSIONS_TSV))
}

fn prior_corrections_tsv_slot() -> &'static ArcSwap<String> {
    static SLOT: OnceLock<ArcSwap<String>> = OnceLock::new();
    SLOT.get_or_init(|| polish_tsv_slot(EMBEDDED_PRIOR_CORRECTIONS_TSV))
}

fn modern_vocab_tsv_slot() -> &'static ArcSwap<String> {
    static SLOT: OnceLock<ArcSwap<String>> = OnceLock::new();
    SLOT.get_or_init(|| polish_tsv_slot(EMBEDDED_MODERN_VOCAB_TSV))
}

fn corpus_garbage_filter_tsv_slot() -> &'static ArcSwap<String> {
    static SLOT: OnceLock<ArcSwap<String>> = OnceLock::new();
    SLOT.get_or_init(|| polish_tsv_slot(EMBEDDED_CORPUS_GARBAGE_FILTER_TSV))
}

/// v1.16 hot-reload driver: replace all 6 polish-overlay TSVs at
/// once. `dir` is expected to contain the six file names
/// `tier_overlay.tsv`, `quickfix_boost.tsv`, `exclusions_v1.tsv`,
/// `prior_corrections_v1.tsv`, `modern_vocab_v1.tsv`,
/// `corpus_garbage_filter_v1.tsv`. Any file that's missing is left
/// on its previous bytes (i.e. embedded or the last successful
/// reload) — non-fatal so a partial polish snapshot still swaps
/// what it has.
///
/// The parse of each replacement TSV is deferred to the first
/// accessor call after the swap (version counter guards); a broken
/// TSV therefore surfaces as a slower first lookup, not as an
/// immediate error. Callers wanting parse validation should call
/// `words()` / `tier_overlay()` / etc. once after swap and check
/// the returned data.
#[cfg(not(target_arch = "wasm32"))]
pub fn set_polish_data_dir(dir: &std::path::Path) {
    let files: &[(&str, &ArcSwap<String>)] = &[
        ("tier_overlay.tsv", tier_overlay_tsv_slot()),
        ("quickfix_boost.tsv", quickfix_boost_tsv_slot()),
        ("exclusions_v1.tsv", exclusions_tsv_slot()),
        ("prior_corrections_v1.tsv", prior_corrections_tsv_slot()),
        ("modern_vocab_v1.tsv", modern_vocab_tsv_slot()),
        (
            "corpus_garbage_filter_v1.tsv",
            corpus_garbage_filter_tsv_slot(),
        ),
    ];
    let mut swapped = false;
    for (name, slot) in files {
        if let Ok(bytes) = std::fs::read_to_string(dir.join(name)) {
            slot.store(Arc::new(bytes));
            swapped = true;
        }
    }
    if swapped {
        bump_polish_version();
    }
}

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
            let Some(ch) = ch_str.chars().next() else {
                continue;
            };
            let Ok(cp) = u32::from_str_radix(cp_hex.trim(), 16) else {
                continue;
            };
            let Ok(tier) = tier_s.trim().parse::<u8>() else {
                continue;
            };
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
            let Some(ch) = ch_str.chars().next() else {
                continue;
            };
            let Some(rank) = ReadingRank::parse(rank_s.trim()) else {
                continue;
            };
            let Ok(offset) = offset_s.trim().parse::<i8>() else {
                continue;
            };
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
            let Ok(tier) = tier_s.trim().parse::<u8>() else {
                continue;
            };
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

/// Parsed `words.tsv` (lazy, cached) — main CC-CEDICT-derived list +
/// modern_vocab_v1 supplemental (Phase 7c.7, 2026-06-30).
///
/// modern_vocab entries are polish-A additions (user-curated modern /
/// network / colloquial words missing from CC-CEDICT). Format
/// `<code>\t<word>\t<freq>`. Tier mapping from freq:
///   freq >= 50000 → tier 2 (lifted 2026-06-30 — let polish compete with HSK 3-4 words)
///   freq >= 30000 → tier 3
///   freq >= 15000 → tier 4
///   freq <  15000 → tier 5
pub fn words() -> Arc<Vec<WordEntry>> {
    versioned_cache(words_cache_slot(), build_words)
}

fn words_cache_slot() -> &'static ArcSwap<(u64, Arc<Vec<WordEntry>>)> {
    static SLOT: OnceLock<ArcSwap<(u64, Arc<Vec<WordEntry>>)>> = OnceLock::new();
    SLOT.get_or_init(|| ArcSwap::from_pointee((u64::MAX, Arc::new(Vec::new()))))
}

fn build_words() -> Vec<WordEntry> {
    let mut out = parse_words_tsv(WORDS_TSV);
    let mut existing: std::collections::HashSet<(String, String)> = out
        .iter()
        .map(|w| (w.code.clone(), w.word.clone()))
        .collect();
    let modern_vocab_tsv = modern_vocab_tsv_slot().load_full();
    for ln in modern_vocab_tsv.lines() {
        if ln.is_empty() || ln.starts_with('#') {
            continue;
        }
        let mut it = ln.split('\t');
        let code = it.next();
        let word = it.next();
        let freq_s = it.next();
        if let (Some(c), Some(w), Some(f)) = (code, word, freq_s) {
            if let Ok(freq) = f.trim().parse::<u32>() {
                if existing.contains(&(c.to_owned(), w.to_owned())) {
                    continue;
                }
                let tier: u8 = if freq >= 50_000 {
                    2
                } else if freq >= 30_000 {
                    3
                } else if freq >= 15_000 {
                    4
                } else {
                    5
                };
                out.push(WordEntry {
                    code: c.to_owned(),
                    word: w.to_owned(),
                    reading_path: format!("[{}]", w), // path not validated for supplements
                    tier,
                    source: "modern_vocab".to_owned(),
                });
                existing.insert((c.to_owned(), w.to_owned()));
            }
        }
    }
    out
}

/// Version-guarded lazy cache: reads the current polish-TSV version,
/// compares against the cached version, and rebuilds if newer. Kept
/// tiny + generic so all 5 polish-derived caches share the same
/// swap semantics. `pub(crate)` so `lib.rs::code_index` can share
/// the same version epoch — code_index builds on top of words()
/// and must invalidate whenever polish adds new words.
pub(crate) fn versioned_cache<T, F>(slot: &'static ArcSwap<(u64, Arc<T>)>, builder: F) -> Arc<T>
where
    F: FnOnce() -> T,
{
    let current = current_polish_version();
    let snap = slot.load_full();
    if snap.0 == current {
        return snap.1.clone();
    }
    let built = Arc::new(builder());
    slot.store(Arc::new((current, built.clone())));
    built
}

// ─── Polish overlay loaders ────────────────────────────────────

/// `tier_overlay.tsv` row: (buffer, word) → override_tier.
pub fn tier_overlay() -> Arc<std::collections::HashMap<(String, String), u8>> {
    versioned_cache(tier_overlay_cache_slot(), build_tier_overlay)
}

fn tier_overlay_cache_slot()
-> &'static ArcSwap<(u64, Arc<std::collections::HashMap<(String, String), u8>>)> {
    static SLOT: OnceLock<ArcSwap<(u64, Arc<std::collections::HashMap<(String, String), u8>>)>> =
        OnceLock::new();
    SLOT.get_or_init(|| {
        ArcSwap::from_pointee((u64::MAX, Arc::new(std::collections::HashMap::new())))
    })
}

fn build_tier_overlay() -> std::collections::HashMap<(String, String), u8> {
    use std::collections::HashMap;
    let mut m = HashMap::new();
    let tsv = tier_overlay_tsv_slot().load_full();
    for ln in tsv.lines() {
        if ln.is_empty() || ln.starts_with('#') {
            continue;
        }
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
}

/// `quickfix_boost.tsv` row: (buffer, word) → boost_freq.
pub fn quickfix_boost() -> Arc<std::collections::HashMap<(String, String), u32>> {
    versioned_cache(quickfix_boost_cache_slot(), build_quickfix_boost)
}

fn quickfix_boost_cache_slot()
-> &'static ArcSwap<(u64, Arc<std::collections::HashMap<(String, String), u32>>)> {
    static SLOT: OnceLock<ArcSwap<(u64, Arc<std::collections::HashMap<(String, String), u32>>)>> =
        OnceLock::new();
    SLOT.get_or_init(|| {
        ArcSwap::from_pointee((u64::MAX, Arc::new(std::collections::HashMap::new())))
    })
}

fn build_quickfix_boost() -> std::collections::HashMap<(String, String), u32> {
    use std::collections::HashMap;
    let mut m = HashMap::new();
    let tsv = quickfix_boost_tsv_slot().load_full();
    for ln in tsv.lines() {
        if ln.is_empty() || ln.starts_with('#') {
            continue;
        }
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
}

/// `prior_corrections_v1.tsv` word → Q4 log-prior boost. Applies globally
/// to that word regardless of buffer (lifts e.g. 继续 over 积蓄 anywhere
/// they compete).
pub fn prior_corrections() -> Arc<std::collections::HashMap<String, i32>> {
    versioned_cache(prior_corrections_cache_slot(), build_prior_corrections)
}

fn prior_corrections_cache_slot()
-> &'static ArcSwap<(u64, Arc<std::collections::HashMap<String, i32>>)> {
    static SLOT: OnceLock<ArcSwap<(u64, Arc<std::collections::HashMap<String, i32>>)>> =
        OnceLock::new();
    SLOT.get_or_init(|| {
        ArcSwap::from_pointee((u64::MAX, Arc::new(std::collections::HashMap::new())))
    })
}

fn build_prior_corrections() -> std::collections::HashMap<String, i32> {
    use std::collections::HashMap;
    let mut m = HashMap::new();
    let tsv = prior_corrections_tsv_slot().load_full();
    for ln in tsv.lines() {
        if ln.is_empty() || ln.starts_with('#') {
            continue;
        }
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
}

/// `modern_freq.tsv` word/char → percentile-rank score (0..25000) from
/// jieba's modern Chinese corpus. Used as **same-tier tiebreaker** in v2
/// query — corpus frequency cannot cross tier boundaries (cap < one tier
/// step 30k). v2 dict (words.tsv) remains authority of what exists;
/// jieba only influences how to rank within tier.
///
/// Words not in jieba (古汉语 / 罕用) get score 0 → demoted within tier.
/// See docs/pinyin-dogfood-2026-06-30/MODERN-FREQ-DESIGN.md.
pub fn modern_freq() -> &'static std::collections::HashMap<String, u16> {
    use std::collections::HashMap;
    static CACHED: OnceLock<HashMap<String, u16>> = OnceLock::new();
    CACHED.get_or_init(|| {
        let mut m = HashMap::with_capacity(96_000);
        for ln in MODERN_FREQ_TSV.lines() {
            if ln.is_empty() || ln.starts_with('#') {
                continue;
            }
            let mut it = ln.split('\t');
            let word = it.next();
            let score_s = it.next();
            if let (Some(w), Some(s)) = (word, score_s) {
                if let Ok(score) = s.trim().parse::<u16>() {
                    m.insert(w.to_owned(), score);
                }
            }
        }
        m
    })
}

/// Runtime exclusion set — (code, word) pairs filtered from v2 output.
/// Composition: exclusions_v1.tsv ∪ corpus_garbage_filter_v1.tsv,
/// MINUS quickfix_boost entries (those are explicit resurrections —
/// e.g. user D1-deleted 洞洞 then later added a quickfix_boost to
/// bring it back).
pub fn exclusions() -> Arc<std::collections::HashSet<(String, String)>> {
    versioned_cache(exclusions_cache_slot(), build_exclusions)
}

fn exclusions_cache_slot()
-> &'static ArcSwap<(u64, Arc<std::collections::HashSet<(String, String)>>)> {
    static SLOT: OnceLock<ArcSwap<(u64, Arc<std::collections::HashSet<(String, String)>>)>> =
        OnceLock::new();
    SLOT.get_or_init(|| {
        ArcSwap::from_pointee((u64::MAX, Arc::new(std::collections::HashSet::new())))
    })
}

fn build_exclusions() -> std::collections::HashSet<(String, String)> {
    use std::collections::HashSet;
    let mut s: HashSet<(String, String)> = HashSet::new();
    let excl_tsv = exclusions_tsv_slot().load_full();
    for ln in excl_tsv.lines() {
        if ln.is_empty() || ln.starts_with('#') {
            continue;
        }
        let mut it = ln.split('\t');
        let code = it.next();
        let word = it.next();
        if let (Some(c), Some(w)) = (code, word) {
            s.insert((c.to_owned(), w.to_owned()));
        }
    }
    let garbage_tsv = corpus_garbage_filter_tsv_slot().load_full();
    for ln in garbage_tsv.lines() {
        if ln.is_empty() || ln.starts_with('#') {
            continue;
        }
        let mut it = ln.split('\t');
        let code = it.next();
        let word = it.next();
        if let (Some(c), Some(w)) = (code, word) {
            s.insert((c.to_owned(), w.to_owned()));
        }
    }
    // Remove entries that have an explicit quickfix_boost (= user
    // wanted them back). Reads quickfix_boost directly to avoid
    // a dep cycle.
    let quickfix_tsv = quickfix_boost_tsv_slot().load_full();
    for ln in quickfix_tsv.lines() {
        if ln.is_empty() || ln.starts_with('#') {
            continue;
        }
        let mut it = ln.split('\t');
        let code = it.next();
        let word = it.next();
        if let (Some(c), Some(w)) = (code, word) {
            s.remove(&(c.to_owned(), w.to_owned()));
        }
    }
    s
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
        assert_eq!(
            missing, 0,
            "all 8105 chars must have kMandarin canonical reading"
        );
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
        assert!(
            yi_readings
                .iter()
                .any(|r| r.reading == "yī" && r.rank == ReadingRank::Primary)
        );
        assert!(yi_readings.iter().any(|r| r.reading == "yí"));
    }

    #[test]
    fn readings_primary_has_offset_zero() {
        let rs = readings();
        for r in rs.iter().filter(|r| r.rank == ReadingRank::Primary) {
            assert_eq!(
                r.tier_offset, 0,
                "primary reading must be tier_offset 0 ({} {})",
                r.ch, r.reading
            );
        }
    }

    #[test]
    fn words_table_size_within_expected_band() {
        let ws = words();
        // Soft pin: base + supplements. Allow growth as polish-A
        // adds words to modern_vocab_v1. Reject pathological doubling.
        let n = ws.len();
        // 2026-07-09: band re-set from 88k-90k → 110k-120k to reflect
        // the ~24k modern_vocab entries accumulated through polish
        // adds since the pin was last tuned.
        assert!(
            (110_000..120_000).contains(&n),
            "words.tsv len drift outside expected band: {}",
            n
        );
    }

    #[test]
    fn words_tier_distribution() {
        let ws = words();
        let mut by_t = [0usize; 10];
        for w in ws.iter() {
            by_t[w.tier as usize] += 1;
        }
        // Tier 1-2 use `>=` since polish adds accrete over time.
        assert!(
            by_t[1] >= 150,
            "tier 1 (HSK 1-2 multi-char) ≥ 150 (got {})",
            by_t[1]
        );
        assert!(
            by_t[2] >= 702,
            "tier 2 (HSK 3-4 + polish-A) ≥ 702 (got {})",
            by_t[2]
        );
        // tier 3 was 3493 base + modern_vocab freq>=60k entries
        assert!(by_t[3] >= 3493, "tier 3 ≥ base 3493 ({} got)", by_t[3]);
        assert!(by_t[4] >= 49734, "tier 4 ≥ base 49734 ({} got)", by_t[4]);
        assert!(by_t[5] >= 31357, "tier 5 ≥ base 31357 ({} got)", by_t[5]);
        assert!(by_t[6] >= 2699, "tier 6 ≥ base 2699 ({} got)", by_t[6]);
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
        assert!(
            !xiuxi.is_empty(),
            "休息 HSK 2 polyphone-neutral case must be in"
        );
        assert_eq!(xiuxi[0].tier, 1, "休息 is HSK 2 → tier 1");
    }

    #[test]
    fn words_reading_path_has_brackets_for_every_char() {
        let ws = words();
        for w in ws.iter().take(1000) {
            let bracket_count = w.reading_path.matches('|').count();
            let char_count = w.word.chars().count();
            assert_eq!(
                bracket_count, char_count,
                "reading_path must have one |-separator per char (word={}, path={})",
                w.word, w.reading_path
            );
        }
    }
}
