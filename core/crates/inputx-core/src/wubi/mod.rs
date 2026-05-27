//! Backwards-compat re-export shim for the v1.4.5 wubi cement carve-out.
//!
//! All wubi engine code (WubiEngine state machine, table lookups,
//! L0 export/import, auto-commit policy) now lives in the dedicated
//! `inputx-wubi-cement` crate. This shim re-exports the same surface
//! so `inputx_core::wubi::*` continues to work for downstream consumers
//! (Session, composite engine, baseline tests) without touching every
//! callsite during the v1.4.5 mechanical carve.
//!
//! Direct consumers may import from `inputx_wubi_cement` instead; the
//! shim stays for one cycle while we audit downstreams, then deletes
//! in v1.4.6 cement cutover.

pub use inputx_wubi_cement::*;
