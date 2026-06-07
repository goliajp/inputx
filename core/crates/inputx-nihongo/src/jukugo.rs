//! Jukugo (熟語) table — JP compound words with reading + freq.
//!
//! Post-治理 2026-06-03: source of truth is `data/library.tsv` (rows
//! with type=jukugo). Parsed once via OnceLock on first access. Was a
//! ~27k-line const array in this file pre-治理 (the biggest "special
//! list" in the codebase per the no-special-list policy
//! `.claude/RANKING-MODEL-INVARIANTS.md` §2); the const has been
//! retired.

#[derive(Copy, Clone, Debug)]
pub struct JukugoEntry {
    pub reading: &'static str,
    pub kanji: &'static str,
    pub freq: u32,
}

const LIBRARY_TSV: &str = include_str!("../data/library.tsv");

fn entries() -> &'static [JukugoEntry] {
    static CACHE: std::sync::OnceLock<Vec<JukugoEntry>> = std::sync::OnceLock::new();
    CACHE
        .get_or_init(|| {
            let mut out = Vec::new();
            for raw in LIBRARY_TSV.lines() {
                let line = raw.trim_end_matches(['\r', '\n']);
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let mut parts = line.split('\t');
                let (Some(code), Some(word), Some(ty), Some(freq_s)) =
                    (parts.next(), parts.next(), parts.next(), parts.next())
                else {
                    continue;
                };
                if ty.trim() != "jukugo" {
                    continue;
                }
                let Ok(freq) = freq_s.trim().parse::<u32>() else {
                    continue;
                };
                out.push(JukugoEntry {
                    reading: code,
                    kanji: word,
                    freq,
                });
            }
            out
        })
        .as_slice()
}

pub fn lookup_by_reading(romaji: &str) -> impl Iterator<Item = (&'static str, u32)> + '_ {
    entries().iter().filter_map(move |e| {
        if e.reading == romaji {
            Some((e.kanji, e.freq))
        } else {
            None
        }
    })
}

/// Prefix-prediction lookup: jukugo whose reading STARTS WITH `romaji` but is
/// longer (the user is mid-typing toward it — shinjuk → 新宿/しんじゅく). Yields
/// `(kanji, freq, reading_len_bytes)` so the composite layer can score by
/// proximity = typed_len / reading_len. Excludes exact matches (== reading),
/// which `lookup_by_reading` already covers.
pub fn lookup_by_reading_prefix(
    romaji: &str,
) -> impl Iterator<Item = (&'static str, u32, usize)> + '_ {
    entries().iter().filter_map(move |e| {
        if e.reading.len() > romaji.len() && e.reading.starts_with(romaji) {
            Some((e.kanji, e.freq, e.reading.len()))
        } else {
            None
        }
    })
}

/// Public access for IDF-snapshot tools (idf-from-nihongo-jukugo).
pub fn all_entries() -> &'static [JukugoEntry] {
    entries()
}
