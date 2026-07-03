#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]
// Historic clippy allowlist — these lints fire on legacy code paths
// (log-prob constants transcribed to full f64 precision, wide iterator
// signatures, needless_range_loop for beam-pruning). Not correctness
// issues; adjusting the code would obscure intent. Silence at crate
// level so CI's `-D warnings` doesn't gate on them.
#![allow(clippy::excessive_precision)]
#![allow(clippy::type_complexity)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::neg_multiply)]
#![allow(clippy::manual_contains)]
#![allow(clippy::manual_clamp)]

//! `inputx-pinyin` — self-developed Mandarin Pinyin input method engine.
//!
//! Engine surface ✓ (segmenter, fuzzy, FST dict, encode, session) +
//! 414k-entry corpus-derived dict (Unihan kHanyuPinlu + jieba + pypinyin
//! + Leipzig + SUBTLEX) + L0 user-learning ranking (3-pick auto-pin).
//!
//! Sibling library: [`inputx-wubi`](https://crates.io/crates/inputx-wubi)
//! — same architectural pattern (PHF static tables, FST main dict,
//! zero-alloc hot path).
//!
//! # Quickstart
//!
//! ```no_run
//! use inputx_pinyin::{PinyinEngine, Session};
//! let engine = PinyinEngine::new();
//! let mut session = Session::new(&engine);
//! for c in "zhongguo".chars() {
//!     session.input_char(c);
//! }
//! let cands = session.candidates();
//! assert_eq!(cands.first().map(String::as_str), Some("中国"));
//! ```
//!
//! # Module map
//! - [`syllable`] — 403 valid Mandarin syllable inventory (PHF set)
//! - [`fuzzy`] — toggleable fuzzy-pair expansion (`z↔zh` etc.)
//! - [`segmenter`] — DP segmentation of continuous pinyin strings
//! - [`dict`] — FST-backed `pinyin → words` lookup with L0 user-learning
//! - [`encode`] — `char → readings` reverse lookup
//! - [`engine`] — immutable [`PinyinEngine`] (dict + fuzzy)
//! - [`session`] — mutable [`Session`] holding the user's input buffer
//! - [`ranking`] — L0 snapshot type for host-side persistence

pub mod abbrev_channel;
pub mod bigram_lm;
pub mod cell_dict;
pub mod dict;
pub mod encode;
pub mod engine;
pub mod fuzzy;
pub mod keyboard_adjacency;
pub mod lattice;
pub mod ranking;
pub mod segmenter;
pub mod session;
pub mod syllable;

pub use dict::{PinyinDict, normalize_lookup_key};
pub use encode::{char_to_pinyin, covered_char_count};
pub use engine::PinyinEngine;
pub use fuzzy::FuzzyConfig;
pub use ranking::{L0Snapshot, PROMOTE_THRESHOLD};
pub use segmenter::{Segmentation, segment};
pub use session::Session;
pub use syllable::{
    VALID_SYLLABLES, count as syllable_count, is_valid as is_valid_syllable,
    longest_valid_syllable_prefix,
};
