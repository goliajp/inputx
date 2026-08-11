//! Composite-side wubi engine (v1.5.2 WU-λ carve-out).
//!
//! Pre-v1.4.5 the WubiEngine state machine + table helpers lived
//! directly inside `inputx-core::wubi`. v1.4.5 WU-ζ moved everything
//! out to `inputx-wubi-cement` and left this file as a re-export
//! shim. v1.5.2 WU-λ moves the state machine back IN per the v1.5
//! D11 taxonomy correction (cement = application source, NOT a
//! published crate); data + low-level lookup helpers
//! (`EMBEDDED_WUBI_IDF`, `wubi_idf_reader`, `layer_from_idf_tag`,
//! `lookup` / `lookup_with_scores` / `lookup_with_layer` /
//! `lookup_with_freq_layer` / `prefix_predictions` / `record_pick` /
//! `export_l0` / `import_l0` / `set_show_rare` / `show_rare` /
//! `warmup` / `is_displayable`) stay in `inputx-wubi-cement` as
//! stone-quality reusable helpers.
//!
//! Public surface remains 1:1 with the v1.4.5 carve so downstream
//! consumers (Session, composite engine, baseline tests) don't see
//! any rename / move noise.

mod engine;

pub use engine::{AutoCommitPolicy, WubiEngine};
pub use inputx_wubi_data::{
    L0Snapshot, export_l0, import_l0, is_displayable, lookup_with_scores, set_show_rare, show_rare,
    warmup,
};
