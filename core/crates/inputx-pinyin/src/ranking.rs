//! L0 ranking — user-learning layer on top of the immutable dict.
//!
//! # Model
//!
//! - **L1** is the immutable lexicon: the embedded FST, ranked by
//!   corpus-derived `freq_score` (higher = more common).
//! - **L0** is a thin per-user override layer:
//!   - **Pinned candidates** — `pinyin → preferred_word`. A pin moves that
//!     word to position 0 in `lookup`'s output, regardless of L1
//!     freq_score.
//!   - **Pick counters** — `(pinyin, word) → u32`.
//!     [`crate::dict::PinyinDict::record_pick`] increments the counter;
//!     once it reaches [`PROMOTE_THRESHOLD`], the word is auto-pinned and
//!     all counters for that pinyin are reset (so a later, different pick
//!     has to earn its 3 votes from scratch — prevents thrashing).
//!
//! Pinyin v0.2 has no layer concept (wubi has 字根 / 简码 / 词组 layers
//! that are wubi-encoding specific). If we ever need layer prefs (e.g.,
//! demote single-char results in favor of phrases), v0.3+ can extend the
//! FST value format `(layer << 56) | freq_score` like wubi does.

use std::collections::HashMap;

/// Number of consecutive picks of the same `(pinyin, word)` required before
/// L0 auto-pins it. Defaults to 3; can be overridden at build time via the
/// `PINYIN_PROMOTE_THRESHOLD` env var (developer escape hatch — not
/// exposed to end users).
pub const PROMOTE_THRESHOLD: u32 = parse_threshold_const();

const fn parse_threshold_const() -> u32 {
    match option_env!("PINYIN_PROMOTE_THRESHOLD") {
        Some(s) => parse_u32_const(s),
        None => 3,
    }
}

const fn parse_u32_const(s: &str) -> u32 {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        panic!("PINYIN_PROMOTE_THRESHOLD must not be empty");
    }
    let mut i = 0;
    let mut n: u32 = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b < b'0' || b > b'9' {
            panic!("PINYIN_PROMOTE_THRESHOLD must be ASCII digits");
        }
        n = n * 10 + (b - b'0') as u32;
        i += 1;
    }
    if n == 0 {
        panic!("PINYIN_PROMOTE_THRESHOLD must be >= 1");
    }
    n
}

/// Persistent state of the L0 layer. Caller serializes / deserializes this
/// however it likes (TOML, JSON, MessagePack, sqlite, …) — the crate
/// intentionally has no `serde` dependency.
#[derive(Debug, Clone, Default)]
pub struct L0Snapshot {
    /// `(pinyin, word)` pairs the user has pinned (manually or via
    /// `record_pick` reaching threshold).
    pub pins: Vec<(String, String)>,
    /// `(pinyin, word, count)` — pending pick counts that haven't yet
    /// reached `PROMOTE_THRESHOLD`. Snapshot semantics are best-effort;
    /// a count of `threshold - 1` restored after restart needs only one
    /// more pick to promote.
    pub pick_counts: Vec<(String, String, u32)>,
}

/// Internal L0 state held by `PinyinDict` behind interior mutability.
#[derive(Default)]
pub(crate) struct L0Inner {
    pub(crate) pins: HashMap<String, String>,
    pub(crate) pick_counts: HashMap<(String, String), u32>,
}

impl L0Inner {
    pub(crate) fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn promote_threshold_is_at_least_one() {
        const _GUARD: () = assert!(PROMOTE_THRESHOLD >= 1);
    }

    #[test]
    fn snapshot_default_is_empty() {
        let s = L0Snapshot::default();
        assert!(s.pins.is_empty());
        assert!(s.pick_counts.is_empty());
    }
}
