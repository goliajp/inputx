//! Composite (wubi + pinyin) dual-engine.
//!
//! Phase 4 of the iOS commercial-grade roadmap. Wraps the two sub-engines
//! (`WubiEngine` from `wubi/` + `PinyinAdapter` over the published
//! `inputx_pinyin` crate) with a `Mode` switch (Mixed / WubiOnly /
//! PinyinOnly) and a merged candidate list with `Source` attribution.
//!
//! Public API mirrors `WubiEngine` so the eventual transition in
//! `session.rs` (item 47) is mostly a type swap. FFI exposure (item 44)
//! adds `set_engine_mode` / `get_engine_mode` / `candidate_source`
//! getters.

mod dispatch;
mod engine;
mod japanese_adapter;
pub(crate) mod l0_json;
mod merge;
mod mode;
mod pinyin_adapter;
pub mod scoring;
// Candidate-quality TDD baseline (user 2026-05-24: 候选质量是非常容易
// 用 tdd 保障的). Pure test module; no production code.
#[cfg(test)]
mod baseline_quality_test;
// Comprehensive baseline — 200+ cases per user 10h autorun directive.
#[cfg(test)]
mod comprehensive_baseline;

pub use engine::CompositeEngine;
// JapaneseAdapter is internal to composite; sub-modules import via
// `super::japanese_adapter::JapaneseAdapter` directly. No external
// caller needs it via composite:: path.
pub use merge::{Candidate, ScoreComponents, Source};
pub use mode::Mode;
pub use pinyin_adapter::PinyinAdapter;
pub use pinyin_adapter::{
    PINYIN_DISABLE_ASSOCIATION, PINYIN_DISABLE_COMPOSE, PINYIN_DISABLE_FUZZY,
    PINYIN_DISABLE_PREDICTION,
};
