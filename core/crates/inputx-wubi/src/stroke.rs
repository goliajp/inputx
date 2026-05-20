//! Backward-compatible re-exports from [`crate::codec`].
//!
//! The actual definitions moved into `codec.rs` so they can be shared with
//! `build.rs` via `#[path]`. This module exists as a stable name path for
//! the public API — adopt directly importing from `crate::codec` going
//! forward.

pub use crate::codec::{Shape, Stroke, region_letter, shibie_ma};
