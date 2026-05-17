//! Locale layer — CJK punctuation + full-width mapping.
//!
//! Phase 9 of the iOS commercial-grade roadmap. Pure functions; no state
//! held here. The Swift / FFI side decides when to apply (based on user
//! settings + active locale) and threads the mapped chars back to
//! `textDocumentProxy.insertText`.
//!
//! Module location: `inputx-core/src/locale/` (per ROADMAP item 80
//! decision). Promote to a standalone `golia-locale` crate only if the
//! Mac shell unfreezes and shares the layer.

pub mod punct;
pub mod width;
