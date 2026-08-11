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

use std::borrow::Cow;
use std::sync::{Arc, OnceLock};

use arc_swap::ArcSwap;
use inputx_dict_format::IdfReader;

/// Embedded IDFv1 kanji dict blob.
pub const EMBEDDED_NIHONGO_KANJI_IDF: &[u8] = include_bytes!("../data/kanji.idf");

/// Byte container for the process-global reader: borrowed from
/// [`EMBEDDED_NIHONGO_KANJI_IDF`] at startup, owned after a hot-reload.
pub type IdfBytes = Cow<'static, [u8]>;

/// Process-global [`IdfReader`] over an [`IdfBytes`] — the embedded
/// blob at startup, replaceable by [`set_nihongo_kanji_idf_bytes`] so a
/// running IME picks up freshly-baked polish without exiting. Hold the
/// returned `Arc` for the duration of a keystroke to guarantee a
/// consistent snapshot across a concurrent swap.
pub fn nihongo_kanji_idf_reader() -> Arc<IdfReader<IdfBytes>> {
    idf_reader_slot().load_full()
}

fn idf_reader_slot() -> &'static ArcSwap<IdfReader<IdfBytes>> {
    static SLOT: OnceLock<ArcSwap<IdfReader<IdfBytes>>> = OnceLock::new();
    SLOT.get_or_init(|| {
        let reader = IdfReader::from_bytes(Cow::Borrowed(EMBEDDED_NIHONGO_KANJI_IDF)).expect(
            "inputx-nihongo-data-kanji EMBEDDED_NIHONGO_KANJI_IDF must be a valid IDFv1 blob",
        );
        ArcSwap::from_pointee(reader)
    })
}

/// Replace the process-global kanji `IdfReader` with one parsed from
/// `bytes` (owned). Returns the previous vs. new entry count. On parse
/// failure the slot is left untouched. Mirrors
/// `inputx_pinyin_helpers::set_pinyin_idf_bytes`.
pub fn set_nihongo_kanji_idf_bytes(bytes: Vec<u8>) -> Result<IdfReloadReport, IdfReloadError> {
    let reader = IdfReader::from_bytes(Cow::Owned(bytes)).map_err(IdfReloadError::Parse)?;
    let new_count = reader.entry_count();
    let old = idf_reader_slot().swap(Arc::new(reader));
    corpus_total_slot().store(Arc::new(None));
    Ok(IdfReloadReport {
        old_entry_count: old.entry_count(),
        new_entry_count: new_count,
    })
}

/// Outcome of [`set_nihongo_kanji_idf_bytes`]: before/after entry count.
#[derive(Debug, Clone, Copy)]
pub struct IdfReloadReport {
    pub old_entry_count: u32,
    pub new_entry_count: u32,
}

/// Failure modes for [`set_nihongo_kanji_idf_bytes`].
#[derive(Debug)]
pub enum IdfReloadError {
    Parse(inputx_dict_format::reader::OpenError),
}

impl std::fmt::Display for IdfReloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "nihongo kanji IdfReader parse failed: {e:?}"),
        }
    }
}

impl std::error::Error for IdfReloadError {}

/// Sum of `raw_freq` across all entries in the CURRENT process-global
/// kanji IDF. Corpus-total denominator for
/// [`inputx_scoring::log_prob_corpus_from_freq`] on the JP kanji engine
/// path. Memoized via [`ArcSwap`]; recomputed after
/// [`set_nihongo_kanji_idf_bytes`] invalidates the slot.
pub fn nihongo_kanji_corpus_total() -> u64 {
    let slot = corpus_total_slot();
    if let Some(total) = **slot.load() {
        return total;
    }
    let computed: u64 = nihongo_kanji_idf_reader()
        .entries()
        .map(|e| e.raw_freq as u64)
        .sum();
    slot.store(Arc::new(Some(computed)));
    computed
}

fn corpus_total_slot() -> &'static ArcSwap<Option<u64>> {
    static SLOT: OnceLock<ArcSwap<Option<u64>>> = OnceLock::new();
    SLOT.get_or_init(|| ArcSwap::from_pointee(None))
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

    #[test]
    fn set_kanji_idf_bytes_round_trip_preserves_lookup() {
        let pre = nihongo_kanji_idf_reader();
        let pre_count = pre.entry_count();
        let pre_nichi: Vec<String> = pre.lookup(b"nichi").iter().map(|e| e.word.into()).collect();
        assert!(!pre_nichi.is_empty());

        let report = set_nihongo_kanji_idf_bytes(EMBEDDED_NIHONGO_KANJI_IDF.to_vec())
            .expect("re-parsing the embedded bytes must succeed");
        assert_eq!(report.old_entry_count, pre_count);
        assert_eq!(report.new_entry_count, pre_count);

        // The pre-swap Arc still answers — ArcSwap only flips what NEW
        // callers see, which is what makes a mid-keystroke reload safe.
        let on_old: Vec<String> = pre.lookup(b"nichi").iter().map(|e| e.word.into()).collect();
        assert_eq!(on_old, pre_nichi);

        let post = nihongo_kanji_idf_reader();
        let post_nichi: Vec<String> = post
            .lookup(b"nichi")
            .iter()
            .map(|e| e.word.into())
            .collect();
        assert_eq!(post_nichi, pre_nichi);
    }

    #[test]
    fn set_kanji_idf_bytes_rejects_garbage_and_keeps_old_reader() {
        let pre_count = nihongo_kanji_idf_reader().entry_count();
        assert!(set_nihongo_kanji_idf_bytes(b"not an idf blob".to_vec()).is_err());
        assert_eq!(
            nihongo_kanji_idf_reader().entry_count(),
            pre_count,
            "a rejected reload must leave the previous dict in place"
        );
    }
}
