//! Built-in rule implementations (v3.0.2+ migration target).
//!
//! Each file in this module is one or more rules migrated out of the
//! inline `if`/`match` blocks in `composite/dispatch.rs`,
//! `composite/pinyin_adapter.rs`, etc. See `.claude/PLAN-rule-engine.md`
//! §3 for the full migration catalog.
//!
//! Status (2026-05-24): v3.0.2a — only `RepeatedLetterExpansion`
//! migrated, NOT YET wired into production (the inline path is still
//! authoritative). This module exists to establish the migration
//! pattern; each subsequent v3.0.2.x adds one rule + its tests.

pub mod repeated_letter;

pub use repeated_letter::{RepeatedLetterExpansion, REPEATED_LETTER_SCORE};
