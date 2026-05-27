//! `inputx-pinyin-data-bigrams` — embedded bigram FSAs for the
//! [`inputx-pinyin`](https://crates.io/crates/inputx-pinyin) engine.
//!
//! Pure data crate: two `pub const` byte slices via `include_bytes!`,
//! zero dependencies, `#![no_std]` clean. Split out of `inputx-pinyin`
//! in v1.4.7 sub-phase B (Strategy C) so the facade publishes light
//! and consumers can opt out via the facade's `bigrams` feature.
//!
//! Ships two FSAs:
//!
//! - [`EMBEDDED_BIGRAMS`] — inter-token word bigrams (`<prev_word>\0
//!   <next_word>` where both are distinct jieba tokens adjacent in
//!   the source corpus). Sole input to next-word prediction in the
//!   facade.
//! - [`EMBEDDED_BIGRAMS_INTRA`] — intra-token char bigrams
//!   (`<a>\0<b>` for adjacent chars *inside* one jieba token).
//!   Helps Viterbi composition prefer known phrases; never used for
//!   next-word prediction.

#![no_std]

/// Inter-token word bigram FSA, in the
/// [`inputx_fsa::Fsa`](https://docs.rs/inputx-fsa) binary format.
pub const EMBEDDED_BIGRAMS: &[u8] =
    include_bytes!("../data/bigrams.fsa");

/// Intra-token char bigram FSA, in the
/// [`inputx_fsa::Fsa`](https://docs.rs/inputx-fsa) binary format.
pub const EMBEDDED_BIGRAMS_INTRA: &[u8] =
    include_bytes!("../data/bigrams_intra.fsa");
