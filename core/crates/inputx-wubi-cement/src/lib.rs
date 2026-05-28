//! `inputx-wubi-cement` — **DEPRECATED at v1.6 → use
//! [`inputx-wubi-data`](https://crates.io/crates/inputx-wubi-data)
//! instead.**
//!
//! Historical (v1.4.5–v1.5) home of wubi data + IDF reader +
//! Layer-from-EntryFlags helpers. The v1.5 D11 taxonomy correction
//! reclassified "cement" as **application source code, not a
//! published crate** — this crate kept its name for backward compat
//! but the content is identical to (and re-exported from)
//! `inputx-wubi-data` post v1.6.
//!
//! Existing consumers can keep depending on this crate indefinitely;
//! the re-export shim is transparent. New consumers should depend on
//! `inputx-wubi-data` directly.

pub use inputx_wubi_data::*;
