//! `inputx-pinyin-cement` — Pinyin-specific consumer-engine cement.
//!
//! Cement layer (see `.claude/PLAN-stones-extract.md` "Cement catalog")
//! built on top of the [`inputx_pinyin`] facade plus
//! [`inputx_dict_format`] (IDFv1 dict) + [`inputx_ngram`] (NGMv1
//! bigram). Consumers get:
//!
//! - `bigram_boost_from_ngm` — runtime helper that reads a NGMv1
//!   [`NgramTable`] and returns a Q4 log-space bigram bonus
//!   compatible with the [`inputx_scoring`] schema.
//! - `PinyinIdfLookup` (v1.4.6 sub-phase C work) — IdfReader-driven
//!   lookup adapter that replaces direct `PinyinDict::lookup_into`
//!   calls in the composite pinyin adapter.
//! - L0 JSON export/import for pinyin pins (carved from
//!   inputx-core/composite/l0_json.rs).
//!
//! The split: [`inputx_pinyin`] facade ships data + lookup primitives
//! (codec / dict / syllable / fuzzy / engine) and is publish-quality
//! for any IME implementer. `inputx-pinyin-cement` is Inputx-specific
//! plumbing — anything wiring the facade into a stateful IME engine
//! that any third-party Pinyin consumer probably wants to re-implement
//! to suit their own ergonomics.
//!
//! pinyin_adapter (the stateful state machine driven by composite
//! engine) lives in inputx-core/composite — it depends on cross-engine
//! rules + mode + scoring modules and is the **composite root cement**
//! per PLAN-stones-extract.md.

pub mod ngram;

pub use ngram::{bigram_boost_from_ngm, legacy_bigram_boost_from_ngm};

/// Embedded NGMv1 bigram blob for the pinyin engine, sourced from
/// `data/private-dict/v0.0.1/pinyin/bigrams.ngm` at compile time.
///
/// Composite hot path (v1.4.6 sub-phase C2 onwards) loads this once
/// via `inputx_ngram::NgramTable::from_bytes(EMBEDDED_BIGRAMS_NGM)` to
/// replace the per-call `PinyinDict::bigram_boost` lookup against the
/// facade's bundled `bigrams.fsa`.
pub const EMBEDDED_BIGRAMS_NGM: &[u8] =
    include_bytes!("../../../../data/private-dict/v0.0.1/pinyin/bigrams.ngm");
