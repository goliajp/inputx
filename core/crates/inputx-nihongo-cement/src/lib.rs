//! `inputx-nihongo-cement` — Japanese-specific consumer-engine cement.
//!
//! Cement layer (see `.claude/PLAN-stones-extract.md` "Cement catalog")
//! built on top of the [`inputx_nihongo`] facade plus
//! [`inputx_dict_format`] (IDFv1 dict). Public surface:
//!
//! - `EMBEDDED_NIHONGO_JUKUGO_IDF` / `EMBEDDED_NIHONGO_KANJI_IDF`
//!   (v1.4.7 sub-phase A4 step 3) — embedded IDFv1 blobs sourced
//!   from `data/private-dict/v0.0.1/nihongo/{jukugo,kanji}.idf` at
//!   compile time. Same data as the facade's `JUKUGO_TABLE` /
//!   `KANJI_TABLE` const tables (generated from them by
//!   `idf-from-nihongo-jukugo` / `idf-from-nihongo-kanji`).
//! - `nihongo_jukugo_idf_reader()` / `nihongo_kanji_idf_reader()` —
//!   process-global `IdfReader` OnceLocks, FST-indexed lookups, ready
//!   for the cement-driven candidate-generation path that's planned
//!   for a v1.4.8 facade refactor.
//!
//! The split: [`inputx_nihongo`] facade ships data + primitives
//! (romaji / kanji / jukugo / brands / counters / engine / session
//! state machine) and is publish-quality for any JP IME implementer.
//! `inputx-nihongo-cement` is Inputx-specific plumbing.
//!
//! ## v1.4.7 A4 step 3 scope clarification
//!
//! `japanese_adapter` (state machine driven by composite engine
//! keystrokes) lives in `inputx-core/composite/` — it's **composite
//! root cement** because it shares cross-engine `mode` + `scoring` +
//! `merge` modules with wubi/pinyin paths. The adapter currently
//! wraps `inputx_nihongo::JapaneseEngine` directly and calls
//! `engine.candidates()`; the engine's candidate generation
//! (`jukugo::lookup_by_reading` + `kanji::lookup_by_reading` +
//! `compose_sentence` + kana fallback + chouonpu plumbing, ~500 LOC
//! across `engine.rs` lines 180-410) bundles corpus lookup with
//! state-machine logic in a way pinyin / wubi do not — pinyin /
//! wubi cement performs candidate fill at the cement layer (and
//! v1.4.7 A4 step 1+2 routed those fills through IDF), but
//! nihongo's lookup lives behind the facade engine boundary.
//!
//! **A4 step 3 ships the cement-side IDF readers + boilerplate**
//! (so the v1.4.8 facade refactor that lifts candidate generation
//! out of `JapaneseEngine` can plug straight in) **but does not
//! cut the runtime path itself**. PLAN.md L4 v1.4.6→v1.4.7 trigger
//! (b) "engine 只读 .idf" is honored for pinyin + wubi composite
//! paths; nihongo's facade-internal lookup is acknowledged as a
//! v1.4.8 refactor item, not a v1.4.7 regression. Runtime behavior
//! is unchanged — the IDF jukugo / kanji blobs are byte-equivalent
//! to the facade const tables (same `idf-from-nihongo-*` build
//! tooling sources both).

use std::sync::OnceLock;

use inputx_dict_format::IdfReader;

/// Embedded IDFv1 jukugo dict blob, sourced from
/// `data/private-dict/v0.0.1/nihongo/jukugo.idf` at compile time. The
/// 1.1 MB file carries the 27,380-entry `JUKUGO_TABLE` (the same
/// table the facade's `jukugo::lookup_by_reading` scans) re-encoded
/// in IDFv1 with FST code index for O(|romaji|) lookups.
pub const EMBEDDED_NIHONGO_JUKUGO_IDF: &[u8] =
    include_bytes!("../data/jukugo.idf");

/// Embedded IDFv1 kanji dict blob, sourced from
/// `data/private-dict/v0.0.1/nihongo/kanji.idf` at compile time. The
/// 40 KB file carries 1,666 `(reading, kanji)` pairs (multi-reading
/// expansion of 813 source kanji from `KANJI_TABLE`) in IDFv1.
pub const EMBEDDED_NIHONGO_KANJI_IDF: &[u8] =
    include_bytes!("../data/kanji.idf");

/// Process-global [`IdfReader`] over [`EMBEDDED_NIHONGO_JUKUGO_IDF`].
/// Lazy-init via `OnceLock`; the ~1 MB parse + sha256 verify cost
/// amortizes across the whole process.
///
/// Wired in v1.4.7 A4 step 3 as preparation for the v1.4.8 nihongo
/// facade refactor that will lift jukugo lookup out of
/// `JapaneseEngine`; the composite hot path currently still calls
/// `JapaneseEngine.candidates()` (which uses the facade
/// `JUKUGO_TABLE`) for runtime parity. The data is byte-equivalent.
pub fn nihongo_jukugo_idf_reader() -> &'static IdfReader<&'static [u8]> {
    static READER: OnceLock<IdfReader<&'static [u8]>> = OnceLock::new();
    READER.get_or_init(|| {
        IdfReader::from_bytes(EMBEDDED_NIHONGO_JUKUGO_IDF)
            .expect("inputx-nihongo-cement EMBEDDED_NIHONGO_JUKUGO_IDF must be a valid IDFv1 blob")
    })
}

/// Process-global [`IdfReader`] over [`EMBEDDED_NIHONGO_KANJI_IDF`].
/// Lazy-init via `OnceLock`; the 40 KB blob parses in microseconds.
pub fn nihongo_kanji_idf_reader() -> &'static IdfReader<&'static [u8]> {
    static READER: OnceLock<IdfReader<&'static [u8]>> = OnceLock::new();
    READER.get_or_init(|| {
        IdfReader::from_bytes(EMBEDDED_NIHONGO_KANJI_IDF)
            .expect("inputx-nihongo-cement EMBEDDED_NIHONGO_KANJI_IDF must be a valid IDFv1 blob")
    })
}

#[cfg(test)]
mod cement_tests {
    use super::*;

    #[test]
    fn jukugo_idf_reader_parses_and_supports_exact_lookup() {
        let r = nihongo_jukugo_idf_reader();
        assert!(r.entry_count() > 20_000);
        // `shinjuku` is a canonical polish-log seed; expected to ship
        // in the v0.0.1 jukugo .idf.
        let hits = r.lookup(b"shinjuku");
        assert!(!hits.is_empty(), "shinjuku must have a jukugo entry");
        let words: Vec<&str> = hits.iter().map(|e| e.word).collect();
        assert!(words.contains(&"新宿"), "shinjuku → 新宿 expected, got {words:?}");
    }

    #[test]
    fn kanji_idf_reader_parses_and_supports_exact_lookup() {
        let r = nihongo_kanji_idf_reader();
        assert!(r.entry_count() > 1_000);
        // `nichi` is a common kanji reading.
        let hits = r.lookup(b"nichi");
        assert!(!hits.is_empty(), "nichi must have a kanji entry");
    }
}
