//! Japanese input engine — pluggable third-source for Inputx.
//!
//! # Scope (v0.1)
//! - Romaji → hiragana / katakana conversion (Hepburn primary, kunrei
//!   alternates accepted). Whole-buffer rendering: every keystroke
//!   re-renders the kana from scratch.
//! - Single-kanji lookup by on-yomi: when the user's romaji buffer is
//!   exactly a valid on-yomi reading (e.g. `ka`, `kou`), surface the
//!   kanji that read that way — restricted to kanji whose Unicode
//!   codepoint is identical to a Simplified-Chinese hanzi (the codepoint-
//!   identity check is baked into [`kanji::KANJI_TABLE`]; new entries
//!   must be hand-verified before adding).
//!
//! # Not in v0.1
//! - Compound conversion (e.g. `nihon` → 日本) — the engine produces
//!   hiragana/katakana for multi-syllable input but does not assemble
//!   kanji compounds.
//! - okurigana / verb inflection
//! - User-learning / pin / freq-tuning
//! - Per-kanji frequency ordering — candidates are returned in declaration
//!   order from `KANJI_TABLE`.
//!
//! # Plugin-style integration
//! This crate is self-contained — no dependency on `inputx-wubi` /
//! `inputx-pinyin`. The consumer (typically `inputx-core::composite`)
//! holds an optional [`JapaneseEngine`] and routes keystrokes to it
//! based on the `enable_japanese` flag. When disabled the engine is
//! never constructed and JP has zero runtime cost on the hot path.
//!
//! See [`engine::JapaneseEngine`] for the main entry point.

#![forbid(unsafe_code)]

pub mod engine;
pub mod jukugo;
pub mod kanji;
pub mod romaji;

pub use engine::{Candidate, JapaneseEngine, KanaKind};
