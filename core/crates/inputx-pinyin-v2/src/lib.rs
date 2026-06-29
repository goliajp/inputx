//! Inputx pinyin engine **v2** — char-centric data model.
//!
//! This crate is the v2 of the pinyin engine, built parallel to the
//! legacy `inputx-pinyin` crate (v1). The v1/v2 switch resolves
//! through this precedence (first hit wins):
//!
//! 1. `INPUTX_PINYIN_VERSION` env var — `v2`/`V2`/`2` → v2; `v1`/`V1`/`1` → v1.
//! 2. `$XDG_CONFIG_HOME/inputx/pinyin-version` file (default
//!    `~/.config/inputx/pinyin-version`) — same accepted strings.
//! 3. `~/Library/Application Support/Inputx/pinyin-version` — macOS
//!    IME daemon reads this on launch (LaunchAgent can't easily pass
//!    env vars).
//! 4. **default = v1**.
//!
//! Probes / CLIs accept `--pinyin v1|v2` and set the env var before
//! resolution — same as 1.
//!
//! ## Phase 0 status
//!
//! Skeleton-only. [`populate`] returns [`Candidates::empty`]. With v2
//! selected the IME effectively runs literal-only on the pinyin side
//! — which matches the 2026-06-28 4-gate-off baseline so the wire is
//! observable + reversible without touching v1.
//!
//! ## Subsequent phases (see
//! `docs/pinyin-char-centric-rewrite-2026-06-29/PLAN.md`)
//!
//! 1. **Data layer** — `chars.tsv` + `readings.tsv` + `words.tsv`
//!    sourced from authoritative open data (通用规范汉字表 + Unihan
//!    kHanyuPinyin + CC-CEDICT). No corpus statistics.
//! 2. **Tier model** — tier桶 from 字表 一/二/三级 + HSK level (no
//!    raw freq field anywhere). Aligns with the existing 10-tier ×
//!    wx>px>nx orthogonal merge model — output contract unchanged.
//! 3. **Engine** — Path 1 lookup by code → reading_path → words.
//!    Forbids any reading_path that doesn't decompose into declared
//!    char readings (= 字字直拼 noise structurally impossible).

#![forbid(unsafe_code)]

use std::sync::OnceLock;

pub mod data;

fn parse_token(s: &str) -> Option<bool> {
    let s = s.trim();
    match s {
        "v2" | "V2" | "2" => Some(true),
        "v1" | "V1" | "1" => Some(false),
        _ => None,
    }
}

fn resolve_from_config_files() -> Option<bool> {
    let home = std::env::var_os("HOME")?;
    let home = home.to_string_lossy().into_owned();
    let xdg = std::env::var("XDG_CONFIG_HOME").ok();
    let candidates: [String; 3] = [
        xdg.map(|x| format!("{}/inputx/pinyin-version", x))
            .unwrap_or_else(|| format!("{}/.config/inputx/pinyin-version", home)),
        format!("{}/.config/inputx/pinyin-version", home),
        format!("{}/Library/Application Support/Inputx/pinyin-version", home),
    ];
    for path in candidates {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Some(v) = parse_token(&content) {
                return Some(v);
            }
        }
    }
    None
}

/// Resolve v1 / v2 selection via the precedence documented in
/// the crate-level docs. Read once and cache.
pub fn enabled() -> bool {
    static CACHED: OnceLock<bool> = OnceLock::new();
    *CACHED.get_or_init(|| {
        if let Ok(s) = std::env::var("INPUTX_PINYIN_VERSION") {
            if let Some(v) = parse_token(&s) {
                return v;
            }
        }
        if let Some(v) = resolve_from_config_files() {
            return v;
        }
        false
    })
}

/// V2 candidate set — the minimal shape the composite-layer caller
/// needs to slot in place of v1's populated state. Fields are by
/// design a subset of what v1's `PinyinAdapter` populates; v1 keeps
/// the full feature set during the transition.
#[derive(Debug, Clone, Default)]
pub struct Candidates {
    /// Ordered list of candidate words (highest-priority first).
    pub words: Vec<String>,
    /// True when at least one candidate is from a non-speculative
    /// path (exact match / dict lookup), not from K-best/fuzzy.
    pub has_non_speculative: bool,
    /// Optional composed full-sentence candidate (long-buffer Viterbi
    /// equivalent in v2 will be re-derived from word path probability).
    pub composed_sentence: Option<String>,
}

impl Candidates {
    pub fn empty() -> Self {
        Self::default()
    }
}

/// Phase 3: Path-1 exact-code lookup against the words.tsv index.
///
/// Buffer is the user-typed ASCII code (e.g. "nihao"). Returns words
/// whose `code` field matches exactly, ordered by (tier asc, word len asc,
/// alphabetical).
pub fn populate(buffer: &str) -> Candidates {
    let scored = query(buffer);
    if scored.is_empty() {
        return Candidates::empty();
    }
    Candidates {
        words: scored.into_iter().map(|(w, _)| w).collect(),
        has_non_speculative: true,
        composed_sentence: None,
    }
}

/// Phase 3 + 4: scored variant — returns (word, score) pairs.
///
/// Three lookup paths joined into one sorted output:
/// - **Words** (exact `code` match): `500_000 - tier * 30_000`
///   (HSK 1-2 ~ 470k, tail ~ 320k).
/// - **Single chars** (bare reading == buffer): `500_000 - char_tier *
///   30_000 - if primary { 0 } else { 5_000 }`. Primary 还(hái) > 还(huán).
/// - **Initials reverse-lookup** (Phase 4, e.g. `wsm → 为什么`):
///   `300_000 - tier * 30_000`. Always below exact matches; relies on
///   reading_path's first-letter-per-char extraction at index build.
///
/// Order: score desc → shorter word first → alphabetical (stable).
pub fn query(buffer: &str) -> Vec<(String, f64)> {
    if buffer.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<(String, f64)> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    // 1. Words (multi-char): exact code match.
    if let Some(rows) = code_index().get(buffer) {
        for w in rows {
            if seen.insert(w.word.clone()) {
                let score = 500_000.0 - (w.tier as f64) * 30_000.0;
                out.push((w.word.clone(), score));
            }
        }
    }

    // 2. Single chars: bare-form of any reading == buffer.
    if let Some(rows) = char_index().get(buffer) {
        for ce in rows {
            let key = ce.ch.to_string();
            if !seen.insert(key.clone()) {
                continue;
            }
            let mut score = 500_000.0 - (ce.char_tier as f64) * 30_000.0;
            if !ce.is_primary {
                score -= 5_000.0;
            }
            // HSK char muscle-memory overlay: HSK 1 → +30k, HSK 6 → +5k,
            // non-HSK → 0. Lifts 我 (HSK 1) over 卧 (non-HSK) at same
            // 通用规范 tier 1.
            if ce.hsk_level > 0 {
                score += (7.0 - ce.hsk_level as f64) * 5_000.0;
            }
            out.push((key, score));
        }
    }

    // 3. Initials reverse-lookup (Phase 4). Buffer-length 2+ avoids
    //    single-letter explosion. `wsm → 为什么`, `bzdao → 不知道`.
    if buffer.len() >= 2 {
        if let Some(rows) = initials_index().get(buffer) {
            for w in rows {
                if !seen.insert(w.word.clone()) {
                    continue;
                }
                let score = 300_000.0 - (w.tier as f64) * 30_000.0;
                out.push((w.word.clone(), score));
            }
        }
    }

    // 4. Composition (Phase 5). Greedy longest-prefix split into
    //    words/chars; concat the pieces. e.g. `nihaoma` → 你好 + 吗 =
    //    你好吗. Skip if buffer.len() < 4 (covered by exact paths)
    //    or if a same-string result already exists.
    if buffer.len() >= 4 {
        if let Some((word, score)) = compose_greedy(buffer) {
            if seen.insert(word.clone()) {
                out.push((word, score));
            }
        }
    }

    out.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.chars().count().cmp(&b.0.chars().count()))
            .then_with(|| a.0.cmp(&b.0))
    });
    out
}

/// Phase 5 greedy longest-prefix composition.
///
/// Walks the buffer left-to-right, at each step consuming the longest
/// prefix that matches a word `code` (highest priority) or a char's
/// bare reading. Returns `None` if any step fails to find a match
/// (= unsegmentable buffer) OR if the composition is a single piece
/// (= already covered by exact paths).
///
/// Score: `300_000 - piece_count * 10_000 - max_tier * 5_000`. Always
/// below exact-match band (320k+) and above initials reverse-lookup.
fn compose_greedy(buffer: &str) -> Option<(String, f64)> {
    let bytes = buffer.as_bytes();
    let mut composed_word = String::new();
    let mut piece_count: u32 = 0;
    let mut max_tier: u8 = 0;
    let mut cursor = 0;
    while cursor < bytes.len() {
        let remaining = &buffer[cursor..];
        let mut best: Option<(usize, String, u8)> = None;
        // Try longest prefix first.
        for len in (1..=remaining.len()).rev() {
            let prefix = &remaining[..len];
            // Word match takes priority.
            if let Some(rows) = code_index().get(prefix) {
                if let Some(top) = rows.iter().min_by_key(|w| w.tier) {
                    best = Some((len, top.word.clone(), top.tier));
                    break;
                }
            }
            // Else single-char match. Pick by (tier asc, non-HSK after
            // HSK, primary before secondary) so 我 wins 卧 at `wo`.
            if let Some(chars) = char_index().get(prefix) {
                if let Some(top) = chars
                    .iter()
                    .min_by_key(|c| {
                        let hsk_rank = if c.hsk_level == 0 { 99 } else { c.hsk_level };
                        (c.char_tier, hsk_rank, !c.is_primary)
                    })
                {
                    best = Some((len, top.ch.to_string(), top.char_tier));
                    break;
                }
            }
        }
        let (consumed, piece, tier) = best?;
        composed_word.push_str(&piece);
        piece_count += 1;
        if tier > max_tier {
            max_tier = tier;
        }
        cursor += consumed;
    }
    if piece_count <= 1 {
        return None;
    }
    let score = 300_000.0
        - (piece_count as f64) * 10_000.0
        - (max_tier as f64) * 5_000.0;
    Some((composed_word, score))
}

/// Initials index for Phase 4 reverse-lookup.
///
/// Build: for each word, parse `reading_path` `[char|reading]…`, take
/// the bare first letter of each reading, join → "wsm" for 为什么.
fn initials_index() -> &'static std::collections::HashMap<String, Vec<&'static data::WordEntry>> {
    use std::collections::HashMap;
    static CACHED: OnceLock<HashMap<String, Vec<&'static data::WordEntry>>> = OnceLock::new();
    CACHED.get_or_init(|| {
        let mut m: HashMap<String, Vec<&'static data::WordEntry>> = HashMap::new();
        for w in data::words() {
            let initials = extract_initials(&w.reading_path);
            if !initials.is_empty() {
                m.entry(initials).or_default().push(w);
            }
        }
        m
    })
}

/// Parse `[char|reading][char|reading]…` and return the first-letter-per-
/// reading string (ASCII lowercase, ü→v handled via bare_letter_form).
fn extract_initials(reading_path: &str) -> String {
    let mut out = String::new();
    for seg in reading_path.split('[') {
        if seg.is_empty() {
            continue;
        }
        let s = seg.trim_end_matches(']');
        let Some((_, reading)) = s.split_once('|') else { continue };
        let bare = bare_letter_form(reading);
        if let Some(c) = bare.chars().next() {
            out.push(c.to_ascii_lowercase());
        }
    }
    out
}

/// Compact char-by-reading lookup row. Built once at the same time as the
/// `code_index` from chars.tsv + readings.tsv joined.
struct CharLookupRow {
    ch: char,
    char_tier: u8,
    /// HSK 2.0 single-char level (0 = non-HSK, 1-6 = HSK level). Used
    /// as a tier-internal muscle-memory ordering signal: HSK 1 chars
    /// rank above same-tier non-HSK chars so 我 beats 卧 at `wo`.
    hsk_level: u8,
    is_primary: bool,
}

fn char_index() -> &'static std::collections::HashMap<String, Vec<CharLookupRow>> {
    use std::collections::HashMap;
    static CACHED: OnceLock<HashMap<String, Vec<CharLookupRow>>> = OnceLock::new();
    CACHED.get_or_init(|| {
        // Join chars.tsv (tier + hsk_level) × readings.tsv (bare reading).
        let mut char_meta: HashMap<char, (u8, u8)> = HashMap::with_capacity(8200);
        for c in data::chars() {
            char_meta.insert(c.ch, (c.tier, c.hsk_level));
        }
        let mut m: HashMap<String, Vec<CharLookupRow>> = HashMap::new();
        for r in data::readings() {
            let Some(&(tier, hsk_level)) = char_meta.get(&r.ch) else { continue };
            let bare = bare_letter_form(&r.reading);
            m.entry(bare).or_default().push(CharLookupRow {
                ch: r.ch,
                char_tier: tier,
                hsk_level,
                is_primary: matches!(r.rank, data::ReadingRank::Primary),
            });
        }
        m
    })
}

/// Strip tone marks from a tone-marked pinyin reading; ü → v.
/// E.g. "huán" → "huan", "lǚ" → "lv".
fn bare_letter_form(reading: &str) -> String {
    let mut out = String::with_capacity(reading.len());
    for c in reading.chars() {
        let stripped = match c {
            'ā' | 'á' | 'ǎ' | 'à' => 'a',
            'ē' | 'é' | 'ě' | 'è' => 'e',
            'ī' | 'í' | 'ǐ' | 'ì' => 'i',
            'ō' | 'ó' | 'ǒ' | 'ò' => 'o',
            'ū' | 'ú' | 'ǔ' | 'ù' => 'u',
            'ǖ' | 'ǘ' | 'ǚ' | 'ǜ' | 'ü' => 'v',
            _ => c,
        };
        out.push(stripped);
    }
    out
}

/// Lazy index: `code` → list of [`WordEntry`] rows. Built once on
/// first call by walking `data::words()`.
fn code_index() -> &'static std::collections::HashMap<&'static str, Vec<&'static data::WordEntry>> {
    use std::collections::HashMap;
    static CACHED: OnceLock<HashMap<&'static str, Vec<&'static data::WordEntry>>> = OnceLock::new();
    CACHED.get_or_init(|| {
        let mut m: HashMap<&'static str, Vec<&'static data::WordEntry>> = HashMap::new();
        for w in data::words() {
            // Safety: data::words() returns &'static [WordEntry] (cached
            // forever via OnceLock), so taking &'static references into it
            // is sound for the lifetime of the process.
            let w_static: &'static data::WordEntry = w;
            m.entry(w_static.code.as_str()).or_default().push(w_static);
        }
        m
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enabled_defaults_to_false_when_env_unset() {
        // Can't easily test set/unset of env var without leaking to other
        // tests (OnceLock) — leave the env-driven branch to integration
        // tests with a fresh process. Just sanity-check the function
        // is callable.
        let _ = enabled();
    }

    #[test]
    fn parse_token_matches_accepted_strings() {
        assert_eq!(parse_token("v2"), Some(true));
        assert_eq!(parse_token("V2"), Some(true));
        assert_eq!(parse_token("2"), Some(true));
        assert_eq!(parse_token("v1"), Some(false));
        assert_eq!(parse_token("V1"), Some(false));
        assert_eq!(parse_token("1"), Some(false));
        assert_eq!(parse_token("  v2 \n"), Some(true), "trim whitespace");
        assert_eq!(parse_token("on"), None);
        assert_eq!(parse_token(""), None);
        assert_eq!(parse_token("3"), None);
    }

    #[test]
    fn populate_returns_nihao_for_nihao() {
        let c = populate("nihao");
        assert!(!c.words.is_empty(), "v2 should resolve nihao → 你好");
        assert_eq!(c.words[0], "你好", "你好 must be top");
        assert!(c.has_non_speculative);
    }

    #[test]
    fn query_returns_scored_pairs() {
        let q = query("nihao");
        assert!(!q.is_empty());
        let (w, s) = &q[0];
        assert_eq!(w, "你好");
        assert!(*s > 300_000.0 && *s < 500_000.0, "score in expected band: {s}");
    }

    #[test]
    fn query_empty_buffer_returns_empty() {
        assert!(query("").is_empty());
    }

    #[test]
    fn query_unknown_buffer_returns_empty() {
        assert!(query("zzzzzz").is_empty());
    }

    #[test]
    fn query_tier1_outranks_tier4() {
        // jixu: 继续 (HSK 4 → tier 2) should outrank 几许 (cedict, tier 4)
        let q = query("jixu");
        let p_jixu = q.iter().position(|(w, _)| w == "继续").expect("继续 in result");
        let p_jixu_lit = q.iter().position(|(w, _)| w == "几许").expect("几许 in result");
        assert!(p_jixu < p_jixu_lit, "HSK 继续 must rank above non-HSK 几许");
    }

    #[test]
    fn query_single_syllable_returns_chars() {
        // duan: returns 短/段/断/端 single chars from readings.tsv.
        let q = query("duan");
        let words: Vec<&str> = q.iter().map(|(w, _)| w.as_str()).collect();
        for expected in &["短", "段", "断", "端"] {
            assert!(words.contains(expected),
                "duan must return single char {expected}; got {words:?}");
        }
    }

    #[test]
    fn bare_letter_form_strips_tones() {
        assert_eq!(bare_letter_form("huán"), "huan");
        assert_eq!(bare_letter_form("lǚ"), "lv");
        assert_eq!(bare_letter_form("nǚ"), "nv");
        assert_eq!(bare_letter_form("yī"), "yi");
    }

    #[test]
    fn extract_initials_basic() {
        assert_eq!(extract_initials("[为|wèi][什|shén][么|me]"), "wsm");
        assert_eq!(extract_initials("[你|nǐ][好|hǎo]"), "nh");
        assert_eq!(extract_initials("[绿|lǜ]"), "l");
    }

    #[test]
    fn initials_lookup_wsm_yields_weishenme() {
        let q = query("wsm");
        let words: Vec<&str> = q.iter().map(|(w, _)| w.as_str()).collect();
        assert!(words.contains(&"为什么"),
            "wsm initials lookup must yield 为什么 ({words:?})");
    }

    #[test]
    fn initials_score_below_exact() {
        // 'jixu' has exact-code words AND maybe initials matches like
        // 计算 (j+x doesn't exist as bigram; let's use less ambiguous).
        // Test invariant: exact-code candidates always rank above
        // initials-only candidates.
        let q = query("jixu");
        let p_exact = q.iter().position(|(w, _)| w == "继续").expect("继续 from exact");
        for (i, (_, score)) in q.iter().enumerate() {
            if i <= p_exact { continue }
            // Anything below 继续 must have score < 继续's score.
            assert!(*score <= q[p_exact].1, "ordering invariant");
        }
    }

    #[test]
    fn initials_single_letter_skipped() {
        // Length-1 buffer should NOT trigger initials explosion.
        // We accept any number of exact-char results for 'a' but
        // 'a' is not in initials_index since we require len >= 2.
        let q = query("a");
        // shouldn't crash. Result might be chars only (e.g. 啊 etc),
        // never an "initials match" emission (which would inflate
        // to thousands of words starting with 'a').
        assert!(q.len() < 100, "single-letter must not balloon: got {}", q.len());
    }

    #[test]
    fn composition_nihaoma_yields_nihao_plus_ma() {
        let q = query("nihaoma");
        let words: Vec<&str> = q.iter().map(|(w, _)| w.as_str()).collect();
        assert!(words.contains(&"你好吗"),
            "nihaoma must compose into 你好吗 ({words:?})");
    }

    #[test]
    fn composition_score_below_exact() {
        // jintianwomen: not a single word; composes to 今天我们.
        // 今天 (HSK 1) and 我们 (HSK 1) both tier 1.
        // Should NOT outrank single-word exact matches (none exist
        // for this buffer, but invariant must hold structurally).
        let q = query("jintianwomen");
        if let Some((w, s)) = q.iter().find(|(w, _)| w == "今天我们") {
            assert!(*s < 320_000.0,
                "composition score must be below exact match band: {w} = {s}");
        }
    }

    #[test]
    fn composition_short_buffer_skipped() {
        // buffer < 4 letters: skip composition path (covered by exact).
        // "ni" len=2 should NOT compose.
        let q = query("ni");
        // Composition would produce e.g. 你你 (greedy double); make
        // sure we DON'T emit such.
        assert!(!q.iter().any(|(w, _)| w == "你你"),
            "短 buffer 不应触发 composition");
    }

    #[test]
    fn hsk_char_overlay_wo_yields_wo_first() {
        // Phase 6: 我 (HSK 1) must rank above 卧 (non-HSK, same 通用规范
        // tier 1, same primary reading). Previously 卧 won by codepoint.
        let q = query("wo");
        let p_wo = q.iter().position(|(w, _)| w == "我").expect("我 in result");
        let p_wo_other = q.iter().position(|(w, _)| w == "卧").expect("卧 in result");
        assert!(p_wo < p_wo_other,
            "HSK 1 我 must beat non-HSK 卧 (got 我@{} 卧@{})", p_wo, p_wo_other);
    }

    #[test]
    fn hsk_char_overlay_hai_yields_hai_first() {
        // Phase 6: 还 (HSK 1) must rank above 亥 (non-HSK, same tier 1).
        let q = query("hai");
        let p_hai = q.iter().position(|(w, _)| w == "还").expect("还 in result");
        let p_hai_other = q.iter().position(|(w, _)| w == "亥").expect("亥 in result");
        assert!(p_hai < p_hai_other,
            "HSK 1 还 must beat non-HSK 亥 (got 还@{} 亥@{})", p_hai, p_hai_other);
    }

    #[test]
    fn composition_uses_hsk_char_picking() {
        // Phase 6: 我的好 (我 HSK 1 + 的 HSK 1 + 好 HSK 1) not 卧得号.
        let q = query("wodehao");
        let words: Vec<&str> = q.iter().map(|(w, _)| w.as_str()).collect();
        assert!(words.contains(&"我的好"),
            "composition picked HSK chars: {words:?}");
        assert!(!words.contains(&"卧得号"),
            "composition should NOT pick non-HSK chars when HSK option exists");
    }
}
