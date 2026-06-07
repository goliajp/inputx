//! `inputx-nihongo-data-kanji` — embedded Japanese kanji multi-
//! reading IDFv1 dict blob + IdfReader for the
//! [`inputx-nihongo`](https://crates.io/crates/inputx-nihongo)
//! engine.
//!
//! Successor to [`inputx-nihongo-cement`](https://crates.io/crates/inputx-nihongo-cement)'s
//! kanji half under the v1.5 D11 taxonomy correction.
//!
//! Pure data + stateless lookup helper.
//!
//! ## What's in the box
//!
//! - [`EMBEDDED_NIHONGO_KANJI_IDF`] — IDFv1 binary blob (~40 KB)
//!   carrying 1,666 `(reading, kanji)` pairs (multi-reading
//!   expansion of 813 source kanji from `KANJI_TABLE`).
//! - [`nihongo_kanji_idf_reader`] — process-global
//!   `OnceLock<IdfReader>`.

use std::sync::OnceLock;

use inputx_dict_format::IdfReader;

/// Embedded IDFv1 kanji dict blob.
pub const EMBEDDED_NIHONGO_KANJI_IDF: &[u8] = include_bytes!("../data/kanji.idf");

/// Process-global [`IdfReader`] over [`EMBEDDED_NIHONGO_KANJI_IDF`].
pub fn nihongo_kanji_idf_reader() -> &'static IdfReader<&'static [u8]> {
    static READER: OnceLock<IdfReader<&'static [u8]>> = OnceLock::new();
    READER.get_or_init(|| {
        IdfReader::from_bytes(EMBEDDED_NIHONGO_KANJI_IDF).expect(
            "inputx-nihongo-data-kanji EMBEDDED_NIHONGO_KANJI_IDF must be a valid IDFv1 blob",
        )
    })
}

/// Sum of `raw_freq` across all entries in
/// [`EMBEDDED_NIHONGO_KANJI_IDF`]. Corpus-total denominator for
/// [`inputx_scoring::log_prob_corpus_from_freq`] on the JP kanji
/// engine path. Process-global `OnceLock`.
pub fn nihongo_kanji_corpus_total() -> u64 {
    static TOTAL: OnceLock<u64> = OnceLock::new();
    *TOTAL.get_or_init(|| {
        nihongo_kanji_idf_reader()
            .entries()
            .map(|e| e.raw_freq as u64)
            .sum()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kanji_idf_reader_parses_and_supports_exact_lookup() {
        let r = nihongo_kanji_idf_reader();
        assert!(r.entry_count() > 1_000);
        let hits = r.lookup(b"nichi");
        assert!(!hits.is_empty(), "nichi must have a kanji entry");
    }
}
