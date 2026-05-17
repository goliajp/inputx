//! inputx-core: shared engine for the Inputx IME (macOS + iOS).

mod composite;
mod ffi;
mod locale;
mod session;
mod wubi;

pub use composite::{Candidate, CompositeEngine, Mode as EngineMode, PinyinAdapter, Source};
pub use session::Session;
pub use wubi::{AutoCommitPolicy, L0Snapshot, WubiEngine, export_l0, import_l0};
