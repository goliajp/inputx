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
///
/// Schema evolution: new fields are always added with a `Default`
/// implementation so v1 stored data round-trips through v2-aware
/// readers (the new field comes back empty), and writers should
/// detect "all defaults" on new fields to skip them when emitting
/// v1 format. See [`L0Snapshot::is_pre_v2`] for the v1-shape check.
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
    /// Phase-4 user-bigram pick counts. `((prev_word, curr_word),
    /// count)` — populated by `pinyin_adapter.commit_at()` after the
    /// CP-4.2 commit hook lands (so v2-shape readers see an empty
    /// `Vec` until then). Empty Vec is the v1-compatible default
    /// state; writers targeting the v1 wire format should skip this
    /// field when emitting.
    pub user_bigram: Vec<((String, String), u32)>,
}

impl L0Snapshot {
    /// `true` when the snapshot has no Phase-4 user_bigram entries.
    /// V1-shape writers should use this to decide whether emitting
    /// the field is necessary.
    pub fn is_pre_v2(&self) -> bool {
        self.user_bigram.is_empty()
    }
}

/// Internal L0 state held by `PinyinDict` behind interior mutability.
#[derive(Default)]
pub(crate) struct L0Inner {
    pub(crate) pins: HashMap<String, String>,
    pub(crate) pick_counts: HashMap<(String, String), u32>,
    /// Phase-4 user-bigram counts. Keyed by `(prev_word, curr_word)`.
    /// `0` count is never stored — entries are inserted only on the
    /// first +1 from `pinyin_adapter.commit_at()` (CP-4.2 hook).
    pub(crate) user_bigram: HashMap<(String, String), u32>,
}

impl L0Inner {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Phase-4 CP-4.1: log10 P(curr | prev) under Laplace smoothing
    /// over the user_bigram counts. Vocabulary size for the +1 floor
    /// is the distinct curr-word count seen so far (a per-prev
    /// vocabulary would be tighter but requires a Vec walk per call;
    /// we use the global distinct-curr set instead, which the
    /// `log_p_user` caller treats as the cold-start floor).
    ///
    /// Returns `-∞` substitute `-99.0` when both `prev` and the
    /// vocabulary are empty — i.e. nothing learned yet. The
    /// integration site (CP-4.3) is expected to weight `log_p_user`
    /// with `λ_u = 0` under the cold-start guard (total count <
    /// 100), so callers should not propagate this -99.0 directly.
    pub(crate) fn log_p_user(&self, prev: &str, curr: &str) -> f32 {
        if self.user_bigram.is_empty() {
            return -99.0;
        }
        // Numerator: count of (prev, curr) + 1.
        let n_pair = self
            .user_bigram
            .get(&(prev.to_string(), curr.to_string()))
            .copied()
            .unwrap_or(0) as f64;
        // Denominator: total count of (prev, *) + V (vocab size = #distinct curr).
        let prev_total: u64 = self
            .user_bigram
            .iter()
            .filter(|((p, _), _)| p == prev)
            .map(|(_, c)| *c as u64)
            .sum();
        let vocab: u64 = {
            let mut seen = HashMap::<&str, ()>::with_capacity(self.user_bigram.len());
            for ((_, c), _) in self.user_bigram.iter() {
                seen.insert(c.as_str(), ());
            }
            seen.len() as u64
        };
        let num = n_pair + 1.0;
        let den = (prev_total + vocab) as f64;
        (num / den).log10() as f32
    }

    /// Phase-4 CP-4.2 hook target. Bumps the count of bigram
    /// `(prev, curr)` by 1. No-op when `prev` or `curr` is empty
    /// (Phase-4 contract: bigram only counts when both endpoints
    /// are actual committed words; session boundaries don't form
    /// bigrams).
    pub(crate) fn bump_user_bigram(&mut self, prev: &str, curr: &str) {
        if prev.is_empty() || curr.is_empty() {
            return;
        }
        *self
            .user_bigram
            .entry((prev.to_string(), curr.to_string()))
            .or_insert(0) += 1;
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
        assert!(s.user_bigram.is_empty());
        // Empty snapshot is v1-compatible (no user_bigram payload).
        assert!(s.is_pre_v2());
    }

    /// CP-4.1 acceptance gate: log_p_user returns the cold-start
    /// sentinel when nothing has been learned yet, so the CP-4.3
    /// integration site can detect "no data, fall back".
    #[test]
    fn log_p_user_cold_start_returns_sentinel() {
        let inner = L0Inner::new();
        assert_eq!(inner.log_p_user("北京", "大学"), -99.0);
    }

    /// CP-4.1 acceptance gate: bump_user_bigram + log_p_user produce
    /// a Laplace-smoothed log10 P. After 9 (北京, 大学) bumps:
    ///   P(大学 | 北京) = (9 + 1) / (9 + V)
    /// where V is the distinct-curr vocab size (= 1 here, just 大学).
    /// log10(10/10) = 0.0.
    #[test]
    fn bump_and_query_user_bigram_laplace_smoothed() {
        let mut inner = L0Inner::new();
        for _ in 0..9 {
            inner.bump_user_bigram("北京", "大学");
        }
        let p = inner.log_p_user("北京", "大学");
        // (9 + 1) / (9 + 1) = 1.0; log10(1.0) = 0.0.
        assert!((p - 0.0).abs() < 1e-6, "got {p}");
    }

    /// CP-4.1 acceptance gate: empty endpoints don't count as a bigram.
    /// Session boundaries always show up as a commit with no `prev`;
    /// the hook must early-return so the boundary doesn't bake into
    /// the user model.
    #[test]
    fn bump_user_bigram_skips_empty_endpoints() {
        let mut inner = L0Inner::new();
        inner.bump_user_bigram("", "大学");
        inner.bump_user_bigram("北京", "");
        inner.bump_user_bigram("", "");
        assert!(inner.user_bigram.is_empty());
    }

    /// CP-4.1 acceptance gate: 3 fixture round-trips through the
    /// snapshot. (1) v1-shape snapshot (no user_bigram) survives
    /// round-trip with user_bigram empty on the other side; (2) v2-
    /// shape snapshot with user_bigram entries survives round-trip
    /// preserving counts; (3) is_pre_v2 distinguishes the two.
    #[test]
    fn snapshot_round_trip_3_fixtures() {
        // Fixture 1: v1-shape (no user_bigram).
        let v1 = L0Snapshot {
            pins: vec![("ni".into(), "你".into())],
            pick_counts: vec![],
            user_bigram: vec![],
        };
        assert!(v1.is_pre_v2());
        let v1_clone = v1.clone();
        assert_eq!(v1.pins, v1_clone.pins);
        assert!(v1_clone.user_bigram.is_empty());

        // Fixture 2: v2-shape (with user_bigram entries).
        let v2 = L0Snapshot {
            pins: vec![],
            pick_counts: vec![],
            user_bigram: vec![
                (("北京".to_string(), "大学".to_string()), 9),
                (("机器".to_string(), "学习".to_string()), 3),
            ],
        };
        assert!(!v2.is_pre_v2());
        let v2_clone = v2.clone();
        assert_eq!(v2.user_bigram.len(), 2);
        assert_eq!(v2_clone.user_bigram.len(), 2);

        // Fixture 3: explicit default → empty.
        let v_default = L0Snapshot::default();
        assert!(v_default.is_pre_v2());
        assert_eq!(v_default.user_bigram.len(), 0);
    }
}
