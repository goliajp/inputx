//! Composite (wubi + pinyin) dual-engine.
//!
//! Phase 4 of the iOS commercial-grade roadmap. Wraps the two sub-engines
//! (`WubiEngine` from `wubi/` + `PinyinAdapter` over the published
//! `golia_pinyin` crate) with a `Mode` switch (Mixed / WubiOnly /
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

pub use engine::CompositeEngine;
pub use japanese_adapter::JapaneseAdapter;
pub use merge::{Candidate, Source};
pub use mode::Mode;
pub use pinyin_adapter::PinyinAdapter;
