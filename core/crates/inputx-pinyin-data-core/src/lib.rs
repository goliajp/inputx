//! `inputx-pinyin-data-core` — embedded core Mandarin Pinyin dict
//! blob for the [`inputx-pinyin`](https://crates.io/crates/inputx-pinyin)
//! engine.
//!
//! Pure data crate: one `pub const` byte slice via `include_bytes!`,
//! zero dependencies, `#![no_std]` clean. Sized so the published
//! crate stays comfortably under crates.io's default per-crate upload
//! limit while being byte-equivalent to the previous in-facade
//! `data/pinyin.dict` blob.
//!
//! Split out of `inputx-pinyin` in v1.4.7 sub-phase B (Strategy C —
//! 3 `inputx-pinyin-data-*` stones + facade umbrella) so the facade
//! crate publishes light and consumers can opt out of the heavier
//! `bigrams` / `trigrams` data via the facade's feature flags
//! without dragging this required core dict along.

#![no_std]

/// Embedded core Pinyin dict, in the
/// [`inputx_fsa::Dict`](https://docs.rs/inputx-fsa) binary format.
/// Pass directly to `inputx_pinyin::PinyinDict::embedded` (which is
/// wired to read this constant when the facade's default `bigrams`
/// / `trigrams` features are off, and always when they're on too —
/// the facade depends on this crate unconditionally).
pub const EMBEDDED_PINYIN_DICT: &[u8] =
    include_bytes!("../data/pinyin.dict");
