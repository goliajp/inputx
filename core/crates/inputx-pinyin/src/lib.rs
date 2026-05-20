#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

//! `golia-pinyin` — self-developed Mandarin Pinyin input method engine.
//!
//! Engine surface ✓ (segmenter, fuzzy, FST dict, encode, session) +
//! 919k-entry corpus-derived dict (Unihan + jieba + Leipzig + SUBTLEX) +
//! L0 user-learning ranking (3-pick auto-pin). The published crate
//! version stays at `0.1.0` per the publish strategy in lab8-ime ROADMAP
//! item 35; internal milestone names (v0.2-data, v0.3-l0) refer to data +
//! feature readiness. See [workspace ROADMAP](https://github.com/goliajp/pinyin/blob/main/ROADMAP.md).
//!
//! Sibling library: [`wubi`](https://crates.io/crates/wubi) — same
//! architectural pattern (PHF static tables, FST main dict, zero-alloc hot
//! path).
//!
//! # Quickstart
//!
//! ```no_run
//! use golia_pinyin::{PinyinEngine, Session};
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

pub mod dict;
pub mod encode;
pub mod engine;
pub mod fuzzy;
pub mod ranking;
pub mod segmenter;
pub mod session;
pub mod syllable;

pub use dict::PinyinDict;
pub use encode::{char_to_pinyin, covered_char_count};
pub use engine::PinyinEngine;
pub use fuzzy::FuzzyConfig;
pub use ranking::{L0Snapshot, PROMOTE_THRESHOLD};
pub use segmenter::{Segmentation, segment};
pub use session::Session;
pub use syllable::{VALID_SYLLABLES, count as syllable_count, is_valid as is_valid_syllable};
