//! Wubi 86 engine.

mod engine;
mod table;

pub use engine::{AutoCommitPolicy, WubiEngine};
/// L0 persistence — re-exported for host wiring (Session also exposes them
/// as instance methods for cleaner call sites).
pub use table::{
    export_l0, import_l0, is_displayable, lookup_with_scores, set_show_rare, show_rare, warmup,
};
/// Re-export of the wubi snapshot type so hosts can build/destructure it
/// without depending on the `inputx-wubi` crate directly.
pub use inputx_wubi::L0Snapshot;
