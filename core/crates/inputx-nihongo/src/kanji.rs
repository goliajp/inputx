//! Kanji table — JP kanji with on/kun readings + relative frequency.
//!
//! Post-治理 2026-06-03: source of truth is `data/library.tsv` (rows
//! with type=kanji). Parsed once via OnceLock on first access. Was a
//! ~850-line const array in this file pre-治理; the const has been
//! retired per the no-special-list policy.
//!
//! Schema: each entry is one (reading, kanji, freq) tuple. The legacy
//! one-row-per-kanji-with-readings-vec structure has been FLATTENED
//! per reading — same shape as `idf-from-nihongo-kanji` always did at
//! IDF gen time. Multi-reading kanji like 人 (jin/nin/hito) appear as
//! 3 entries here.

#[derive(Copy, Clone, Debug)]
pub struct KanjiEntry {
    pub reading: &'static str,
    pub kanji: char,
    pub freq: u32,
}

const LIBRARY_TSV: &str = include_str!("../data/library.tsv");

fn entries() -> &'static [KanjiEntry] {
    static CACHE: std::sync::OnceLock<Vec<KanjiEntry>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let mut out = Vec::new();
        for raw in LIBRARY_TSV.lines() {
            let line = raw.trim_end_matches(['\r', '\n']);
            if line.is_empty() || line.starts_with('#') { continue; }
            let mut parts = line.split('\t');
            let (Some(code), Some(word), Some(ty), Some(freq_s)) =
                (parts.next(), parts.next(), parts.next(), parts.next()) else { continue };
            if ty.trim() != "kanji" { continue; }
            let Ok(freq) = freq_s.trim().parse::<u32>() else { continue };
            let Some(kanji) = word.chars().next() else { continue };
            if word.chars().count() != 1 { continue; }
            out.push(KanjiEntry { reading: code, kanji, freq });
        }
        out
    }).as_slice()
}

/// Lookup: given a Hepburn romaji string, return all (kanji, freq) tuples
/// whose readings match. Linear scan; ~1.6k entries.
pub fn lookup_by_reading(romaji: &str) -> impl Iterator<Item = (char, u32)> + '_ {
    entries().iter().filter_map(move |e| {
        if e.reading == romaji { Some((e.kanji, e.freq)) } else { None }
    })
}

/// Public access for IDF-snapshot tools (idf-from-nihongo-kanji).
pub fn all_entries() -> &'static [KanjiEntry] {
    entries()
}
