#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

//! `wubi` — self-developed Wubi 86 encoder + dictionary.
//!
//! Phase 1 deliverable: a self-contained, zero-allocation encoder
//! ([`encode_into`], [`fn@encode`]) backed by a pluggable lookup closure.
//!
//! Public modules:
//! - [`codec`] — pure algorithm + types; no external imports; usable from
//!   `build.rs` via `#[path]`.
//! - [`stroke`] — backward-compat re-exports from [`codec`].
//! - [`decomp`] — owned `Decomp` and seed-file parser.
//! - [`mod@encode`] — high-level encoder API (uses runtime PHF/HashMap tables).
//! - [`zigen`] — 字根 → letter lookup.
//! - [`jianma`] — 一级简码 lookup.
//!
//! ```ignore
//! use wubi::{encode, embedded_seed};
//! for (ch, decomp) in embedded_seed() {
//!     let code = encode(&decomp).unwrap();
//!     println!("{ch}\t{code}");
//! }
//! ```

pub mod codec;
pub mod decomp;
pub mod dict;
pub mod encode;
pub mod jianma;
pub mod layer;
pub mod stroke;
pub mod zigen;

pub use codec::{DecompRef, EncodeError, Shape, Stroke, encode_with_lookup};
pub use decomp::{Decomp, embedded_seed};
pub use dict::{L0Snapshot, WubiDict, PROMOTE_THRESHOLD};
pub use encode::{EncodedCode, encode, encode_into};
pub use jianma::{iter_jianma1, lookup_jianma1};
pub use layer::{DEFAULT_LAYER_PREFS, LAYER_BASE, LAYER_COUNT, Layer};
pub use zigen::iter as iter_zigen;
pub use zigen::lookup as lookup_zigen;
