//! `inputx-wubi-cement` — Wubi-specific engine glue.
//!
//! Cement layer (see `.claude/PLAN-stones-extract.md` "Cement catalog")
//! sitting on top of the `inputx-wubi` facade. Consumers get a
//! stateful `WubiEngine` ready to be driven keystroke-by-keystroke,
//! plus L0 (per-user pin) export/import helpers and the auto-commit
//! policy enum.
//!
//! The split: `inputx-wubi` ships data + primitives (codec, decomp,
//! dict, jianma, layer, stroke, zigen) and is publish-quality for any
//! IME implementer. `inputx-wubi-cement` is Inputx-specific business
//! logic — buffer state machine, simcode promote rules, auto-commit
//! policies, L0 snapshot model — that any third-party Wubi consumer
//! probably wants to re-implement to suit their own ergonomics.
//!
//! # Public surface (1:1 with the pre-v1.4.5 `inputx_core::wubi`
//! module so the carve-out is a pure mechanical rename for consumers).

mod engine;
mod table;

pub use engine::{AutoCommitPolicy, WubiEngine};
pub use table::{
    export_l0, import_l0, is_displayable, lookup_with_scores, set_show_rare,
    show_rare, warmup,
};
/// Re-export of the wubi L0 snapshot type so hosts can build /
/// destructure it without depending on the `inputx-wubi` crate
/// directly.
pub use inputx_wubi::L0Snapshot;
