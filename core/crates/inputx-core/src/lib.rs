// Historic clippy allowlist (mirrors inputx-pinyin/v2 crate — legacy
// idioms; not correctness/perf issues).
#![allow(clippy::collapsible_if)]
#![allow(clippy::excessive_precision)]
#![allow(clippy::type_complexity)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::neg_multiply)]
#![allow(clippy::manual_contains)]
#![allow(clippy::manual_clamp)]
#![allow(clippy::manual_range_contains)]

//! inputx-core: platform-agnostic composite engine for the Inputx IME.
//!
//! Pure Rust — no `extern "C"`, no `wasm-bindgen`. The C ABI lives in the
//! sibling `inputx-core-ffi` crate (used by iOS / Mac / Linux / Windows
//! native hosts via cbindgen-emitted `inputx_core.h`). A future
//! `inputx-core-wasm` crate will expose the same surface to JavaScript
//! via `wasm-bindgen`.
//!
//! This layered structure keeps `inputx-core` usable from any Rust
//! context — CLI testbeds, server-side IME services, or other Rust apps
//! — without dragging in C-ABI or wasm-bindgen machinery they don't need.

mod composite;
mod input_mode;
mod session;
// v3.0 rule engine — scaffolding only at v3.0.1 (no rules migrated
// yet, existing inline rules continue to execute). Public so tests
// and the future inputx-debug binary can build rule fixtures.
pub mod rules;

// `locale` and `wubi` are `pub` so the sibling `inputx-core-ffi` crate can
// reach `inputx_core::locale::punct::*`, `inputx_core::wubi::set_show_rare`,
// etc. Direct Rust consumers should prefer the curated re-exports below.
pub mod japanese;
pub mod locale;
pub mod wubi;

pub use crate::wubi::{AutoCommitPolicy, L0Snapshot, WubiEngine, export_l0, import_l0};
pub use composite::{
    Candidate, CompositeEngine, Mode as EngineMode, PINYIN_DISABLE_ASSOCIATION,
    PINYIN_DISABLE_COMPOSE, PINYIN_DISABLE_FUZZY, PINYIN_DISABLE_PREDICTION, PinyinAdapter,
    ScoreComponents, Source,
};
pub use input_mode::InputMode;
pub use locale::punct::{SmartQuoteState, ascii_to_cjk_punct};
pub use locale::width::{full_width, half_width};
pub use session::{PinyinReloadError, Session};

/// v1.15 hot-reload wire points, re-exported so the FFI crate can
/// touch them without pulling `inputx-pinyin-helpers` as a direct
/// dep — inputx-core already links it.
pub mod hot_reload {
    pub use crate::composite::{set_bigrams_ngm_bytes, set_inter_bigrams_ngm_bytes};
    pub use inputx_pinyin_helpers::{IdfReloadError, IdfReloadReport, set_pinyin_idf_bytes};
    // v1.16: v2 engine's polish overlay TSVs — same lifecycle as the
    // v1 pinyin.dict / words.idf swap. Re-exported so inputx-core-ffi
    // doesn't need to add its own dep on inputx-pinyin-v2.
    pub use inputx_pinyin_v2::set_polish_data_dir as set_v2_polish_data_dir;
}
