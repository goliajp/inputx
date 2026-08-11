//! Composite-side Japanese engine (v1.5.1 WU-κ carve-out).
//!
//! Pre-v1.5.1, [`crate::composite::japanese_adapter::JapaneseAdapter`]
//! wrapped [`inputx_nihongo::JapaneseEngine`] directly and called
//! `engine.candidates()` — but the facade engine's `refresh_candidates`
//! reads `JUKUGO_TABLE` / `KANJI_TABLE` const tables internally,
//! violating PLAN-v1.4.md L4 trigger (b) "engine 只读 .idf" for the
//! nihongo path.
//!
//! v1.5.1 lifts the facade engine's state machine + candidate
//! generation into this composite-side module, swapping the
//! `JUKUGO_TABLE` / `KANJI_TABLE` reads for cement IDF readers
//! (`inputx_nihongo_cement::nihongo_{jukugo,kanji}_idf_reader`). The
//! facade `inputx_nihongo::JapaneseEngine` is intentionally left
//! untouched — it remains the public surface for direct-facade
//! consumers (notably `inputx-nihongo-wasm`). The composite engine
//! emits identical [`inputx_nihongo::Candidate`] values so downstream
//! `composite/japanese_adapter` filter / score logic is byte-
//! equivalent.
//!
//! Pattern mirrors [`crate::composite::pinyin_adapter`] and the
//! v1.5.2 wubi cement carve-out: cement = application source under
//! `inputx-core/`, not a separately-published crate.

mod compose;
mod engine;
mod lookup;

pub use engine::JapaneseEngine;
