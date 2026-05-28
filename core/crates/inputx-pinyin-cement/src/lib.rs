//! `inputx-pinyin-cement` — **DEPRECATED at v1.6 → use
//! [`inputx-pinyin-helpers`](https://crates.io/crates/inputx-pinyin-helpers)
//! instead.**
//!
//! Historical (v1.4.6–v1.5) home of pinyin embedded IDFv1 dict +
//! NGMv1 bigram blob + stateless helpers (bigram boost,
//! estimated_freq_from_log_prior, pinyin_idf_reader). The v1.5 D11
//! taxonomy correction reclassified "cement" as **application
//! source code, not a published crate** — this crate kept its name
//! for backward compat but the content is identical to (and
//! re-exported from) `inputx-pinyin-helpers` post v1.6.
//!
//! Existing consumers can keep depending on this crate indefinitely;
//! the re-export shim is transparent. New consumers should depend on
//! `inputx-pinyin-helpers` directly.

pub use inputx_pinyin_helpers::*;
