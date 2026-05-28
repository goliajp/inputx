//! `inputx-nihongo-cement` — **DEPRECATED at v1.6 → use
//! [`inputx-nihongo-data-jukugo`](https://crates.io/crates/inputx-nihongo-data-jukugo)
//! +
//! [`inputx-nihongo-data-kanji`](https://crates.io/crates/inputx-nihongo-data-kanji)
//! instead.**
//!
//! Historical (v1.4.7–v1.5) home of Japanese embedded IDFv1
//! jukugo + kanji dict blobs + their `OnceLock<IdfReader>` helpers.
//! The v1.5 D11 taxonomy correction reclassified "cement" as
//! **application source code, not a published crate** — this crate
//! kept its name for backward compat but the content is identical
//! to (and re-exported from) the two `inputx-nihongo-data-*` stones
//! post v1.6.
//!
//! Existing consumers can keep depending on this crate indefinitely;
//! the re-export shim is transparent. New consumers should depend on
//! `inputx-nihongo-data-jukugo` / `inputx-nihongo-data-kanji`
//! directly per the use case.

pub use inputx_nihongo_data_jukugo::*;
pub use inputx_nihongo_data_kanji::*;
