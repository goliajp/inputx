//! `inputx-nihongo-data-jukugo` — embedded Japanese jukugo (熟語)
//! IDFv1 dict blob + IdfReader for the
//! [`inputx-nihongo`](https://crates.io/crates/inputx-nihongo)
//! engine.
//!
//! Successor to [`inputx-nihongo-cement`](https://crates.io/crates/inputx-nihongo-cement)'s
//! jukugo half under the v1.5 D11 taxonomy correction (cement =
//! application source, NOT a published crate). The old `-cement`
//! crate is deprecated and re-exports from this crate for backward
//! compat.
//!
//! Pure data + stateless lookup helper. No application glue, no
//! per-session state.
//!
//! ## What's in the box
//!
//! - [`EMBEDDED_NIHONGO_JUKUGO_IDF`] — IDFv1 binary blob (~1.1 MB)
//!   carrying the 27,380-entry `JUKUGO_TABLE` (byte-equivalent to
//!   the facade's const table; `idf-from-nihongo-jukugo` sources
//!   both from the same data).
//! - [`nihongo_jukugo_idf_reader`] — process-global
//!   `OnceLock<IdfReader>`; the ~1 MB parse + sha256 verify
//!   amortizes once across the process lifetime.

use std::sync::OnceLock;

use inputx_dict_format::IdfReader;

/// Embedded IDFv1 jukugo dict blob.
pub const EMBEDDED_NIHONGO_JUKUGO_IDF: &[u8] =
    include_bytes!("../data/jukugo.idf");

/// Process-global [`IdfReader`] over [`EMBEDDED_NIHONGO_JUKUGO_IDF`].
pub fn nihongo_jukugo_idf_reader() -> &'static IdfReader<&'static [u8]> {
    static READER: OnceLock<IdfReader<&'static [u8]>> = OnceLock::new();
    READER.get_or_init(|| {
        IdfReader::from_bytes(EMBEDDED_NIHONGO_JUKUGO_IDF)
            .expect("inputx-nihongo-data-jukugo EMBEDDED_NIHONGO_JUKUGO_IDF must be a valid IDFv1 blob")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jukugo_idf_reader_parses_and_supports_exact_lookup() {
        let r = nihongo_jukugo_idf_reader();
        assert!(r.entry_count() > 20_000);
        let hits = r.lookup(b"shinjuku");
        assert!(!hits.is_empty(), "shinjuku must have a jukugo entry");
        let words: Vec<&str> = hits.iter().map(|e| e.word).collect();
        assert!(words.contains(&"新宿"), "shinjuku → 新宿 expected, got {words:?}");
    }
}
