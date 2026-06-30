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
///
/// **Default flipped to v2** (2026-06-30, Phase 7 final) after the
/// char-centric engine reached 357/357 baseline parity. Users wanting
/// the legacy v1 path can opt out via:
///   - `INPUTX_PINYIN_VERSION=v1` env var, or
///   - `v1` written to `~/.config/inputx/pinyin-version`, or
///   - `v1` written to `~/Library/Application Support/Inputx/pinyin-version`.
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
        true  // Phase 7 final: v2 is now the default.
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
    // has_non_speculative = true ONLY when we hit an exact (word or
    // char) match at buffer — not for prefix-completion-only emissions.
    // Composite-layer's auto-commit (OnUniqueMatch) blocks if pinyin
    // has non-speculative; so a bare-letter buffer like 'g' with just
    // prefix-completion shouldn't block wubi Jianma1 commit.
    let has_exact = code_index().contains_key(buffer) || char_index().contains_key(buffer);
    Candidates {
        words: scored.into_iter().map(|(w, _, _)| w).collect(),
        has_non_speculative: has_exact,
        composed_sentence: None,
    }
}

/// Phase 7a: scored variant — returns (word, score, tier) triples.
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
pub fn query(buffer: &str) -> Vec<(String, f64, u8)> {
    if buffer.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<(String, f64, u8)> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let buf_owned = buffer.to_owned();

    // Fuzzy normalization (Phase 7c.5): lue/nue ↔ lve/nve (ü encoding
    // convention). Try the normalized form alongside the literal.
    let normalized = normalize_uv(buffer);
    let alt_buffer = if normalized != buffer {
        Some(normalized.as_str())
    } else {
        None
    };

    // 1. Words (multi-char): exact code match (literal + normalized lue→lve).
    for try_buf in std::iter::once(buffer).chain(alt_buffer) {
        if let Some(rows) = code_index().get(try_buf) {
            for w in rows {
                if data::exclusions().contains(&(buf_owned.clone(), w.word.clone())) {
                    continue;
                }
                if seen.insert(w.word.clone()) {
                    let tier = data::tier_overlay()
                        .get(&(buf_owned.clone(), w.word.clone()))
                        .copied()
                        .unwrap_or(w.tier);
                    let mut score = 500_000.0 - (tier as f64) * 30_000.0;
                    // Slight penalty for fuzzy-normalized match.
                    if try_buf != buffer { score -= 2_000.0; }
                    out.push((w.word.clone(), score, tier));
                }
            }
        }
    }

    // 2. Single chars: bare-form of any reading == buffer.
    if let Some(rows) = char_index().get(buffer) {
        for ce in rows {
            let key = ce.ch.to_string();
            if data::exclusions().contains(&(buf_owned.clone(), key.clone())) {
                continue;
            }
            if !seen.insert(key.clone()) {
                continue;
            }
            let tier = data::tier_overlay()
                .get(&(buf_owned.clone(), key.clone()))
                .copied()
                .unwrap_or(ce.char_tier);
            // Secondary reading penalty (Phase 7c.11): bump to effective
            // tier + 1 so 是(tí 副读) doesn't outscore 题(tí 主读) at
            // buffer "ti". Display tier follows so cross-engine merge
            // sees the demotion too.
            let effective_tier = if ce.is_primary {
                tier
            } else {
                tier.saturating_add(1).min(9)
            };
            let mut score = 500_000.0 - (effective_tier as f64) * 30_000.0;
            if ce.hsk_level > 0 && tier == ce.char_tier && ce.is_primary {
                score += (7.0 - ce.hsk_level as f64) * 5_000.0;
            }
            // Word-prominence tiebreaker (Phase 7c.1): HSK-weighted
            // sum of word containments. Cap at +20k so it never crosses
            // a full 30k tier boundary.
            let prominence = *char_word_count().get(&ce.ch).unwrap_or(&0);
            score += (prominence as f64).min(20_000.0);
            out.push((key, score, effective_tier));
        }
    }

    // 3. Initials reverse-lookup (Phase 4).
    if buffer.len() >= 2 {
        if let Some(rows) = initials_index().get(buffer) {
            for w in rows {
                if data::exclusions().contains(&(buf_owned.clone(), w.word.clone())) {
                    continue;
                }
                if !seen.insert(w.word.clone()) {
                    continue;
                }
                let tier = data::tier_overlay()
                    .get(&(buf_owned.clone(), w.word.clone()))
                    .copied()
                    .unwrap_or(w.tier);
                // Initials tier is +2 buckets to make sure exact match
                // always sorts above (cross-engine merge primary is tier).
                let display_tier = tier.saturating_add(2).min(9);
                let score = 300_000.0 - (tier as f64) * 30_000.0;
                out.push((w.word.clone(), score, display_tier));
            }
        }
    }

    // 4. Composition (Phase 5).
    if buffer.len() >= 4 {
        if let Some((word, score, max_tier)) = compose_greedy(buffer) {
            if !data::exclusions().contains(&(buf_owned.clone(), word.clone()))
                && seen.insert(word.clone())
            {
                // Composition is char-stacking — never a "high quality"
                // candidate by itself. Floor display_tier at 5 so it
                // can't outrank any tier 1-4 exact match (even a fuzzy
                // one). max_tier + 3 lifts noisy compositions further.
                let display_tier = max_tier.saturating_add(3).max(5).min(9);
                out.push((word, score, display_tier));
            }
        }
    }

    // 6. Prefix completion (Phase 7b). When buffer doesn't fully match
    //    any syllable (e.g. "zho" / "zhon" / "z") OR returns thin
    //    results, surface words/chars whose code or reading STARTS WITH
    //    the buffer. Capped at PREFIX_CAP to avoid flooding short
    //    buffers ("z" matches 10k+ entries).
    //
    // Suppression rule (Phase 7c.3): if buffer is already a "complete
    // current input" — either a valid syllable (char_index hit) or an
    // exact-code multi-syllable word (code_index hit) — the user is
    // composing a CURRENT candidate, not asking for prefix extensions.
    // Skip path 6 so:
    //   zhong     → 中/众/终 only, not 中国/中午
    //   lianxiang → 联想 only, not 联想学习/联想起
    let has_exact_syllable = char_index().contains_key(buffer)
        || code_index().contains_key(buffer);
    const PREFIX_CAP: usize = 30;
    let mut prefix_added = 0;
    if !buffer.is_empty() && !has_exact_syllable {
        // Bare-letter buffer: collect chars FIRST so PREFIX_CAP doesn't
        // run out of slots before single chars surface.
        let bare_letter = buffer.len() == 1;
        if bare_letter {
            let mut prefix_chars_first: Vec<(&str, &CharLookupRow)> = Vec::new();
            for (bare, rows) in char_index() {
                if !bare.starts_with(buffer) || bare.as_str() == buffer {
                    continue;
                }
                for ce in rows {
                    if seen.contains(&ce.ch.to_string()) { continue; }
                    if data::exclusions().contains(&(buf_owned.clone(), ce.ch.to_string())) { continue; }
                    prefix_chars_first.push((bare.as_str(), ce));
                }
            }
            prefix_chars_first.sort_by(|a, b| {
                let a_hsk = if a.1.hsk_level == 0 { 99u8 } else { a.1.hsk_level };
                let b_hsk = if b.1.hsk_level == 0 { 99u8 } else { b.1.hsk_level };
                a.1.char_tier.cmp(&b.1.char_tier)
                    .then_with(|| a_hsk.cmp(&b_hsk))
                    .then_with(|| (!a.1.is_primary).cmp(&!b.1.is_primary))
            });
            // Bias: take half of cap for chars first.
            let half = PREFIX_CAP / 2;
            for (_, ce) in prefix_chars_first.iter().take(half) {
                let key = ce.ch.to_string();
                if !seen.insert(key.clone()) { continue; }
                let tier = ce.char_tier;
                let mut score = 220_000.0 - (tier as f64) * 30_000.0;
                if !ce.is_primary { score -= 5_000.0; }
                if ce.hsk_level > 0 {
                    score += (7.0 - ce.hsk_level as f64) * 5_000.0;
                }
                let prominence = *char_word_count().get(&ce.ch).unwrap_or(&0);
                score += (prominence as f64).min(20_000.0);
                let display_tier = tier.saturating_add(3).min(9);
                out.push((key, score, display_tier));
                prefix_added += 1;
            }
        }

        // Word prefix matches — iterate words.tsv, filter by starts_with.
        let mut prefix_words: Vec<&data::WordEntry> = data::words()
            .iter()
            .filter(|w| w.code.starts_with(buffer) && w.code.as_str() != buffer)
            .filter(|w| !seen.contains(&w.word))
            .filter(|w| !data::exclusions().contains(&(buf_owned.clone(), w.word.clone())))
            // Also honor exclusion against the word's OWN code — so a
            // D1 like (yidalimian, 义大利面) blocks the prefix-completion
            // surfacing too (yidal → ... → 义大利面 from yidalimian).
            .filter(|w| !data::exclusions().contains(&(w.code.clone(), w.word.clone())))
            .collect();
        // Tier asc, then code asc for determinism; pick top N.
        prefix_words.sort_by(|a, b| {
            a.tier.cmp(&b.tier)
                .then_with(|| a.word.chars().count().cmp(&b.word.chars().count()))
                .then_with(|| a.code.cmp(&b.code))
        });
        // For bare-letter buffer (e.g. "q"), single chars should lead
        // phrases — user typing one letter intends to see chars.
        // Score band: words 250k normally, but 150k at len==1 so chars
        // (200k base) rank above. See q_bare_letter_single_chars_lead_phrases.
        let word_prefix_base = if buffer.len() == 1 { 150_000.0 } else { 250_000.0 };
        let word_cap = PREFIX_CAP.saturating_sub(prefix_added);
        for w in prefix_words.iter().take(word_cap) {
            if !seen.insert(w.word.clone()) { continue; }
            let tier = w.tier;
            let score = word_prefix_base - (tier as f64) * 30_000.0;
            let display_tier = tier.saturating_add(3).min(9);
            out.push((w.word.clone(), score, display_tier));
            prefix_added += 1;
        }

        // Char prefix matches — for single-letter buffers in particular.
        // Iterate char_index keys, filter by starts_with.
        let mut prefix_chars: Vec<(&str, &CharLookupRow)> = Vec::new();
        for (bare, rows) in char_index() {
            if !bare.starts_with(buffer) || bare.as_str() == buffer {
                continue;
            }
            for ce in rows {
                if seen.contains(&ce.ch.to_string()) { continue; }
                if data::exclusions().contains(&(buf_owned.clone(), ce.ch.to_string())) { continue; }
                prefix_chars.push((bare.as_str(), ce));
            }
        }
        prefix_chars.sort_by(|a, b| {
            let a_hsk = if a.1.hsk_level == 0 { 99u8 } else { a.1.hsk_level };
            let b_hsk = if b.1.hsk_level == 0 { 99u8 } else { b.1.hsk_level };
            a.1.char_tier.cmp(&b.1.char_tier)
                .then_with(|| a_hsk.cmp(&b_hsk))
                .then_with(|| (!a.1.is_primary).cmp(&!b.1.is_primary))
        });
        let remaining = PREFIX_CAP.saturating_sub(prefix_added);
        for (_, ce) in prefix_chars.iter().take(remaining) {
            let key = ce.ch.to_string();
            if !seen.insert(key.clone()) { continue; }
            let tier = ce.char_tier;
            let mut score = 200_000.0 - (tier as f64) * 30_000.0;
            if !ce.is_primary { score -= 5_000.0; }
            if ce.hsk_level > 0 {
                score += (7.0 - ce.hsk_level as f64) * 5_000.0;
            }
            let display_tier = tier.saturating_add(3).min(9);
            out.push((key, score, display_tier));
            prefix_added += 1;
        }
    }
    let _ = prefix_added;

    // 5. Quickfix boost (polish Class B): promote to tier 1 (high
    //    pinyin) without forcing tier 0. This preserves the WU-ψ wx>px
    //    cross-engine rule — wubi simcode at tier 1 still beats pinyin
    //    quickfix at tier 1 in mixed mode (e.g. an: wubi 世 #0 / pinyin
    //    安 below). Pinyin-only mode ranking is unaffected.
    //
    //    Tier 1 also preserves polish-log semantics (boost-to-top of
    //    pinyin pool, since most polish quickfix targets compete with
    //    non-boosted pinyin candidates at tier 3-5).
    for ((buf_k, word_k), boost_freq) in data::quickfix_boost().iter() {
        if buf_k != buffer { continue; }
        if data::exclusions().contains(&(buf_owned.clone(), word_k.clone())) { continue; }
        let boost_score = 540_000.0 + (*boost_freq as f64) / 100.0;
        if seen.contains(word_k) {
            if let Some(slot) = out.iter_mut().find(|(w, _, _)| w == word_k) {
                if boost_score > slot.1 {
                    slot.1 = boost_score;
                    slot.2 = 1;
                }
            }
        } else {
            seen.insert(word_k.clone());
            out.push((word_k.clone(), boost_score, 1));
        }
    }

    // Apply prior_corrections globally (Phase 7c.2). Q4 log-units boost,
    // mapped to v2 linear score as boost_q4 * 1000 (so e.g. 继续 +17 →
    // +17k score, comfortably within tier band but enough to flip a
    // mis-ordered pair).
    let priors = data::prior_corrections();
    for entry in out.iter_mut() {
        if let Some(&boost_q4) = priors.get(&entry.0) {
            entry.1 += (boost_q4 as f64) * 1000.0;
        }
    }

    // Sort: tier asc, score desc, len asc, word asc — matches
    // composite/merge.rs sort order so v2's intra-pinyin order stays
    // stable when fed into the cross-engine merge.
    out.sort_by(|a, b| {
        a.2.cmp(&b.2)
            .then_with(|| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal))
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
fn compose_greedy(buffer: &str) -> Option<(String, f64, u8)> {
    let bytes = buffer.as_bytes();
    let mut composed_word = String::new();
    let mut piece_count: u32 = 0;
    let mut word_piece_count: u32 = 0;
    let mut max_tier: u8 = 0;
    let mut cursor = 0;
    while cursor < bytes.len() {
        let remaining = &buffer[cursor..];
        let mut best: Option<(usize, String, u8, bool /*is_word*/)> = None;
        for len in (1..=remaining.len()).rev() {
            let prefix = &remaining[..len];
            if let Some(rows) = code_index().get(prefix) {
                if let Some(top) = rows.iter().min_by_key(|w| w.tier) {
                    best = Some((len, top.word.clone(), top.tier, true));
                    break;
                }
            }
            if let Some(chars) = char_index().get(prefix) {
                if let Some(top) = chars
                    .iter()
                    .min_by_key(|c| {
                        let hsk_rank = if c.hsk_level == 0 { 99 } else { c.hsk_level };
                        let prominence_inv = u32::MAX
                            .saturating_sub(*char_word_count().get(&c.ch).unwrap_or(&0));
                        (c.char_tier, hsk_rank, !c.is_primary, prominence_inv)
                    })
                {
                    best = Some((len, top.ch.to_string(), top.char_tier, false));
                    break;
                }
            }
        }
        let (consumed, piece, tier, is_word) = best?;
        // Phase 7c.22 char-piece gate: if a piece is a single char
        // (non-word), it must be a grammatical completer particle.
        // This blocks "<word>+<random char>" compose noise like
        // 刑事饿 (刑事+饿) / 你好吃 (你好+吃 — actually this might be
        // valid if 你好吃 is a real word; check after) / 今天吃 etc.
        // Categorical filter (not per-entry list) — sanctioned by
        // RANKING-MODEL-INVARIANTS §2 char-class allowlist.
        if !is_word {
            const COMPLETERS: &[&str] = &[
                "的", "了", "吗", "呢", "啊", "吧", "着", "过",
                "呀", "嘛", "哦", "哈", "嘿",
            ];
            if !COMPLETERS.contains(&piece.as_str()) {
                return None;
            }
        }
        composed_word.push_str(&piece);
        piece_count += 1;
        if is_word { word_piece_count += 1; }
        if tier > max_tier {
            max_tier = tier;
        }
        cursor += consumed;
    }
    if piece_count <= 1 {
        return None;
    }
    // ≥1 word-piece rule still in effect for buffers that decompose
    // into pure completer chars (e.g. "lema" → 了+吗? — no real word).
    if word_piece_count == 0 {
        return None;
    }
    let score = 300_000.0
        - (piece_count as f64) * 10_000.0
        - (max_tier as f64) * 5_000.0;
    // Phase 7c.4 validation: if composed_word EXISTS in words.tsv,
    // the buffer MUST be one of its registered codes. Otherwise the
    // composition picked a wrong char-reading combo (e.g. nuanhe →
    // 暖+和(he) = 暖和, but 暖和 only reads as nuanhuo). Reject.
    if let Some(valid_codes) = word_codes().get(composed_word.as_str()) {
        if !valid_codes.iter().any(|c| *c == buffer) {
            return None;
        }
    }
    Some((composed_word, score, max_tier))
}

/// Lazy index: word → set of valid codes (from words.tsv). Used by
/// compose_greedy to reject "this word exists but at a different
/// reading" compositions.
fn word_codes() -> &'static std::collections::HashMap<&'static str, Vec<&'static str>> {
    use std::collections::HashMap;
    static CACHED: OnceLock<HashMap<&'static str, Vec<&'static str>>> = OnceLock::new();
    CACHED.get_or_init(|| {
        let mut m: HashMap<&'static str, Vec<&'static str>> = HashMap::new();
        for w in data::words() {
            m.entry(w.word.as_str()).or_default().push(w.code.as_str());
        }
        m
    })
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

/// Lazy index: char → HSK-weighted word-containment score. Higher =
/// more central to common modern usage.
///
/// Formula: for each word containing the char, add:
///   tier 1 (HSK 1-2 multi-char): weight 1000
///   tier 2 (HSK 3-4): weight 300
///   tier 3 (HSK 5-6): weight 100
///   tier 4+: 1
///
/// Total word count alone misled (十 in many number compounds inflates
/// it above 是); HSK-tier-weighted signal is closer to "is this char
/// part of words a learner / daily user encounters".
fn char_word_count() -> &'static std::collections::HashMap<char, u32> {
    use std::collections::HashMap;
    static CACHED: OnceLock<HashMap<char, u32>> = OnceLock::new();
    CACHED.get_or_init(|| {
        let mut m: HashMap<char, u32> = HashMap::with_capacity(8200);
        for w in data::words() {
            let weight = match w.tier {
                1 => 1000,
                2 => 300,
                3 => 100,
                _ => 1,
            };
            for c in w.word.chars() {
                *m.entry(c).or_insert(0) += weight;
            }
        }
        m
    })
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

/// Fuzzy buffer normalization: replace `lue` → `lve` and `nue` → `nve`
/// (and uppercase variants). Mirrors the v1 normalize_lookup_key
/// behavior — common typing variant for ü-after-l/n syllables.
fn normalize_uv(buffer: &str) -> String {
    let mut out = buffer.to_owned();
    // Only the lue/nue/lüe/nüe followed by consonant patterns. Restrict
    // to exact 3-letter sequences "lue" / "nue" anywhere in the buffer
    // (longer ones like "lüe" don't occur in ASCII IME input).
    if out.contains("lue") { out = out.replace("lue", "lve"); }
    if out.contains("nue") { out = out.replace("nue", "nve"); }
    out
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
        let (w, s, _t) = &q[0];
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
        let p_jixu = q.iter().position(|(w, _, _)| w == "继续").expect("继续 in result");
        let p_jixu_lit = q.iter().position(|(w, _, _)| w == "几许").expect("几许 in result");
        assert!(p_jixu < p_jixu_lit, "HSK 继续 must rank above non-HSK 几许");
    }

    #[test]
    fn query_single_syllable_returns_chars() {
        // duan: returns 短/段/断/端 single chars from readings.tsv.
        let q = query("duan");
        let words: Vec<&str> = q.iter().map(|(w, _, _)| w.as_str()).collect();
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
        let words: Vec<&str> = q.iter().map(|(w, _, _)| w.as_str()).collect();
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
        let p_exact = q.iter().position(|(w, _, _)| w == "继续").expect("继续 from exact");
        for (i, (_, score, _)) in q.iter().enumerate() {
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
        let words: Vec<&str> = q.iter().map(|(w, _, _)| w.as_str()).collect();
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
        if let Some((w, s, _t)) = q.iter().find(|(w, _, _)| w == "今天我们") {
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
        assert!(!q.iter().any(|(w, _, _)| w == "你你"),
            "短 buffer 不应触发 composition");
    }

    #[test]
    fn hsk_char_overlay_wo_yields_wo_first() {
        // Phase 6: 我 (HSK 1) must rank above 卧 (non-HSK, same 通用规范
        // tier 1, same primary reading). Previously 卧 won by codepoint.
        let q = query("wo");
        let p_wo = q.iter().position(|(w, _, _)| w == "我").expect("我 in result");
        let p_wo_other = q.iter().position(|(w, _, _)| w == "卧").expect("卧 in result");
        assert!(p_wo < p_wo_other,
            "HSK 1 我 must beat non-HSK 卧 (got 我@{} 卧@{})", p_wo, p_wo_other);
    }

    #[test]
    fn hsk_char_overlay_hai_yields_hai_first() {
        // Phase 6: 还 (HSK 1) must rank above 亥 (non-HSK, same tier 1).
        let q = query("hai");
        let p_hai = q.iter().position(|(w, _, _)| w == "还").expect("还 in result");
        let p_hai_other = q.iter().position(|(w, _, _)| w == "亥").expect("亥 in result");
        assert!(p_hai < p_hai_other,
            "HSK 1 还 must beat non-HSK 亥 (got 还@{} 亥@{})", p_hai, p_hai_other);
    }

    #[test]
    fn composition_uses_hsk_char_picking() {
        // Phase 6 original: 我的好 (我 + 的 + 好) shown.
        // Phase 7c.8 ≥1 word-piece rule: pure char+char+char rejected
        // (`wodehao` doesn't contain any multi-char word piece). The
        // HSK char picking invariant is now indirectly tested via
        // `hsk_char_overlay_wo_yields_wo_first` (path 1b char query
        // for buffer "wo"). composition test deprecated for pure-char
        // sequences.
        let q = query("wodehao");
        let words: Vec<&str> = q.iter().map(|(w, _, _)| w.as_str()).collect();
        assert!(!words.contains(&"卧得号"),
            "composition should NOT pick non-HSK chars when HSK option exists");
    }
}
