//! `inputx-pinyin-data-trigrams` — embedded word-trigram dict for
//! the [`inputx-pinyin`](https://crates.io/crates/inputx-pinyin)
//! engine.
//!
//! Pure data crate: one `pub const` byte slice via `include_bytes!`,
//! zero dependencies, `#![no_std]` clean. Split out of `inputx-pinyin`
//! in v1.4.7 sub-phase B (Strategy C) so the facade publishes light
//! and consumers who only need exact-syllable / bigram-boost lookup
//! can opt out via the facade's `trigrams` feature.
//!
//! Ships the inter-token word-trigram FST: keys are `<a>\0<b>\0<c>`
//! where all three are distinct jieba tokens adjacent in the
//! corpus. Sole input to the facade's
//! `PinyinDict::predict_next_words_context` API, which feeds the
//! "联想 v1.0" sentence-level coherent next-word prediction path.

#![no_std]

/// Embedded word-trigram dict, in the
/// [`inputx_fsa::Dict`](https://docs.rs/inputx-fsa) binary format.
pub const EMBEDDED_TRIGRAMS: &[u8] = include_bytes!("../data/trigrams.dict");
