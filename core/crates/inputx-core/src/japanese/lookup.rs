//! Cement-side jukugo / kanji lookup helpers backed by the IDF
//! readers in [`inputx_nihongo_cement`].
//!
//! API matches `inputx_nihongo::jukugo` / `inputx_nihongo::kanji`
//! shape (returning the same `(word, freq)` / `(char, freq)` tuples)
//! so the composite-side [`crate::japanese::compose`] +
//! [`crate::japanese::engine`] can reuse the facade engine's
//! refresh-candidates logic line-for-line — only the lookup primitive
//! source changes (const table → IDF reader).
//!
//! IDF entries carry `raw_freq: u32` (post-v1.4.7 A4 step 1 schema
//! bump) — `idf-from-nihongo-{jukugo,kanji}` write each
//! `JUKUGO_TABLE.freq` / `KANJI_TABLE.freq` straight into that field,
//! so byte-equivalent values come back out.

use inputx_nihongo_data_jukugo::nihongo_jukugo_idf_reader;
use inputx_nihongo_data_kanji::nihongo_kanji_idf_reader;

/// Owned `(kanji, freq)` rows whose jukugo reading exactly matches
/// `romaji`. Multi-reading kanji return one row per (reading, kanji)
/// pair; multi-reading kanji entries also fan out at IDF write time.
pub fn lookup_jukugo_by_reading(romaji: &str) -> Vec<(String, u32)> {
    let reader = nihongo_jukugo_idf_reader();
    let mut out: Vec<(String, u32)> = Vec::new();
    for entry in reader.lookup(romaji.as_bytes()) {
        out.push((entry.word.to_string(), entry.raw_freq));
    }
    out
}

/// Owned `(kanji, freq, reading_byte_len)` rows whose jukugo reading
/// strictly extends `romaji` (i.e. `entry.code.len() > romaji.len()`).
/// Used by the prefix-prediction path (CP-A); proximity decay is the
/// caller's responsibility.
///
/// Walks the IDF reader's FST via `prefix_for_each_entry` so the visit
/// cost is O(matching codes) rather than full entry scan.
pub fn lookup_jukugo_by_reading_prefix(romaji: &str) -> Vec<(String, u32, usize)> {
    let reader = nihongo_jukugo_idf_reader();
    let prefix_len = romaji.len();
    let mut out: Vec<(String, u32, usize)> = Vec::new();
    reader.prefix_for_each_entry(romaji.as_bytes(), |e| {
        if e.code.len() <= prefix_len {
            return;
        }
        out.push((e.word.to_string(), e.raw_freq, e.code.len()));
    });
    out
}

/// Owned `(kanji_char, freq)` rows whose kanji reading exactly matches
/// `romaji`. The IDF stores `word: &str` (1-char kanji as UTF-8
/// bytes); we decode the first `char` here.
pub fn lookup_kanji_by_reading(romaji: &str) -> Vec<(char, u32)> {
    let reader = nihongo_kanji_idf_reader();
    let mut out: Vec<(char, u32)> = Vec::new();
    for entry in reader.lookup(romaji.as_bytes()) {
        if let Some(ch) = entry.word.chars().next() {
            out.push((ch, entry.raw_freq));
        }
    }
    out
}
