//! FST-backed pinyin dictionary with a two-tier ranking model.
//!
//! # Storage
//!
//! Same key/value shape as `wubi::WubiDict`:
//! - **key**: `{pinyin_lowercase}\x00{word_utf8}` — the 0x00 separator lets
//!   `range().ge(prefix\x00).lt(prefix\x01)` cleanly enumerate all words for
//!   a given pinyin without false-positive prefix matches.
//! - **value**: `u64` corpus-derived frequency score. v0.2 uses
//!   single-pass min-max normalized counts (capped at `max_freq_score` from
//!   `tools/weights/rules.toml`); pinyin v0.2 has no layer concept (wubi has
//!   字根/简码/词组 layers wubi-encoding-specific). If layers ever become
//!   useful, the value can pack `(layer << 56) | freq_score` like wubi.
//!
//! # Ranking (v0.3)
//!
//! - **L1** = the immutable embedded FST, ordered by `freq_score` desc.
//! - **L0** = a per-user mutable layer (pins + pick counters):
//!   - 3 picks of the same `(pinyin, word)` auto-pin it (per-pinyin
//!     counters reset on promotion to prevent thrashing).
//!   - `record_pick` / `pin` / `forget` mutate L0 via interior `RwLock`
//!     so a single shared `PinyinDict` can feed many concurrent sessions.
//!   - `export_l0` / `import_l0` round-trip the L0 state for host-side
//!     persistence (no `serde` dep on the lib).

use std::sync::RwLock;

use fst::{IntoStreamer, Map, Streamer};

use crate::ranking::{L0Inner, L0Snapshot, PROMOTE_THRESHOLD};

// Default = full pinyin dict from the committed `data/pinyin.fst` (15 MB,
// pre-built by `tools/build_fst.rs` from `data/weights/weights.tsv` —
// maintainer regenerates after data changes; see workspace ROADMAP item 23).
// `bootstrap_only` feature swaps to the tiny ~125-entry bootstrap FST built
// at compile time from `data/bootstrap.tsv` (1.7 KB).
//
// Why pre-built vs build.rs-generated: keeps the published crate under
// crates.io's size cap by letting us exclude the heavy intermediate TSV
// files (weights.tsv 23 MB, readings.tsv 13 MB, etc.) from the package.
#[cfg(not(feature = "bootstrap_only"))]
const DICT_BYTES: &[u8] = include_bytes!("../data/pinyin.fst");

#[cfg(feature = "bootstrap_only")]
const DICT_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/bootstrap.fst"));

/// The pinyin dictionary: an embedded FST plus a mutable L0 layer for
/// per-user preference learning.
///
/// All read methods take `&self`. L0 mutations (`record_pick`, `pin`,
/// `forget`, `import_l0`) also take `&self` — interior mutability via
/// `RwLock` lets a single shared instance feed every concurrent IME /
/// WASM session without exposing the lock to the caller.
pub struct PinyinDict {
    map: Map<&'static [u8]>,
    l0: RwLock<L0Inner>,
}

impl PinyinDict {
    /// Construct from the embedded FST. Cheap (validates the FST header
    /// and initializes an empty L0). Callers should still cache the
    /// instance and reuse it for the program lifetime.
    pub fn embedded() -> Self {
        Self {
            map: Map::new(DICT_BYTES).expect("invalid embedded pinyin FST"),
            l0: RwLock::new(L0Inner::new()),
        }
    }

    /// Number of L0 pinned pinyins.
    pub fn l0_pin_count(&self) -> usize {
        self.l0.read().map(|g| g.pins.len()).unwrap_or(0)
    }

    /// Number of distinct `(pinyin, word)` pairs with pending pick counters.
    pub fn l0_pending_count(&self) -> usize {
        self.l0.read().map(|g| g.pick_counts.len()).unwrap_or(0)
    }

    /// Total number of (pinyin, word) entries in the FST. Not the number of
    /// distinct pinyin strings.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// `true` iff the dictionary is empty.
    pub fn is_empty(&self) -> bool {
        self.map.len() == 0
    }

    /// All words exactly matching `pinyin`, ordered by frequency desc, then
    /// FST byte order as a stable tiebreaker.
    ///
    /// Allocates a fresh `Vec`. Hot-loop callers should use
    /// [`Self::lookup_into`] to reuse a caller-owned buffer.
    pub fn lookup(&self, pinyin: &str) -> Vec<String> {
        let mut out = Vec::new();
        self.lookup_into(pinyin, &mut out);
        out
    }

    /// Same as [`Self::lookup`] but writes into a caller-owned buffer
    /// (cleared on entry, capacity preserved). The IME calls this many
    /// times per keystroke; reusing the buffer eliminates allocator
    /// pressure.
    ///
    /// Result ordering:
    ///   1. L0 pin (if any) at index 0,
    ///   2. then `freq_score` desc,
    ///   3. then FST byte order (stable tiebreaker via sort_by_key stability).
    pub fn lookup_into(&self, pinyin: &str, out: &mut Vec<String>) {
        out.clear();

        let lower = pinyin.to_ascii_lowercase();
        let mut prefix = lower.into_bytes();
        let prefix_len = prefix.len();
        prefix.push(0u8);

        let mut upper = prefix.clone();
        let last = upper.len() - 1;
        upper[last] = 0x01;

        // Score during scan; reuse small scratch buffer.
        let mut scratch: Vec<(String, u64)> = Vec::with_capacity(8);
        let mut stream = self
            .map
            .range()
            .ge(prefix.as_slice())
            .lt(upper.as_slice())
            .into_stream();
        while let Some((key, value)) = stream.next() {
            if key.len() <= prefix_len + 1 {
                continue;
            }
            let word_bytes = &key[prefix_len + 1..];
            if let Ok(s) = core::str::from_utf8(word_bytes) {
                scratch.push((s.to_string(), value));
            }
        }
        scratch.sort_by_key(|s| std::cmp::Reverse(s.1));

        out.reserve(scratch.len());
        for (w, _) in scratch.drain(..) {
            out.push(w);
        }

        // L0 pin: pull to position 0 if present.
        if let Ok(l0) = self.l0.read()
            && let Some(pref) = l0.pins.get(&lower_str(pinyin))
            && let Some(idx) = out.iter().position(|w| w == pref)
            && idx > 0
        {
            let p = out.remove(idx);
            out.insert(0, p);
        }
    }

    /// All `(pinyin, word)` pairs with pinyin starting with `prefix`. Ordered
    /// by (pinyin asc, word asc) — useful for prefix completion suggestions.
    pub fn prefix(&self, prefix: &str) -> Vec<(String, String)> {
        let lower = prefix.to_ascii_lowercase();
        let lo = lower.into_bytes();
        let hi = bump_last(&lo);

        let mut stream = self
            .map
            .range()
            .ge(lo.as_slice())
            .lt(hi.as_slice())
            .into_stream();

        let mut results: Vec<(String, String)> = Vec::new();
        while let Some((key, _value)) = stream.next() {
            let Some(sep) = key.iter().position(|b| *b == 0u8) else {
                continue;
            };
            let (pinyin_bytes, rest) = key.split_at(sep);
            let word_bytes = &rest[1..];
            if let (Ok(pinyin), Ok(word)) = (
                core::str::from_utf8(pinyin_bytes),
                core::str::from_utf8(word_bytes),
            ) {
                results.push((pinyin.to_string(), word.to_string()));
            }
        }
        results.sort();
        results
    }

    /// Streaming version of `prefix_with_freq` — invokes `visit(pinyin, word,
    /// freq)` for each matching entry without allocating a full `Vec`. Use
    /// this for hot per-keystroke paths where the caller only keeps a small
    /// top-K subset: `Vec<(String, String, u64)>` allocation for short prefixes
    /// (e.g., `"z"` matches ~50k entries) is the dominant cost otherwise.
    /// The slices passed to `visit` are tied to the underlying FST stream
    /// and only valid for the duration of each call — callers must `.to_owned()`
    /// any data they want to keep.
    pub fn prefix_for_each<F>(&self, prefix: &str, mut visit: F)
    where
        F: FnMut(&str, &str, u64),
    {
        self.prefix_for_each_raw(prefix, |pinyin_bytes, word_bytes, freq| {
            if let (Ok(pinyin), Ok(word)) = (
                core::str::from_utf8(pinyin_bytes),
                core::str::from_utf8(word_bytes),
            ) {
                visit(pinyin, word, freq);
            }
        });
    }

    /// Raw byte-slice variant of [`prefix_for_each`](Self::prefix_for_each) —
    /// skips the utf8 validation on each entry. FST keys are written from
    /// validated Rust `String`s in `build.rs`, so re-validating per-entry is
    /// pure overhead on hot paths.
    ///
    /// On short prefixes (`"z"` matches ~50k entries) skipping utf8 decode
    /// saves ~2ms vs `prefix_for_each`. Use only when the caller doesn't
    /// need `&str` for downstream operations and is willing to assume the
    /// invariant; otherwise `prefix_for_each` is the safer default.
    pub fn prefix_for_each_raw<F>(&self, prefix: &str, mut visit: F)
    where
        F: FnMut(&[u8], &[u8], u64),
    {
        let lower = prefix.to_ascii_lowercase();
        let lo = lower.into_bytes();
        let hi = bump_last(&lo);

        let mut stream = self
            .map
            .range()
            .ge(lo.as_slice())
            .lt(hi.as_slice())
            .into_stream();

        while let Some((key, value)) = stream.next() {
            let Some(sep) = key.iter().position(|b| *b == 0u8) else {
                continue;
            };
            let (pinyin_bytes, rest) = key.split_at(sep);
            let word_bytes = &rest[1..];
            visit(pinyin_bytes, word_bytes, value);
        }
    }

    /// All `(pinyin, word, freq_score)` triples with pinyin starting with
    /// `prefix`. Returned in raw FST byte order (pinyin asc, then word asc as
    /// stored). The `freq_score` is the same `u64` value used by `lookup_into`
    /// for ordering, so callers can replicate the same frequency ordering when
    /// building secondary indices (e.g., initial-letter abbreviation tables).
    ///
    /// For hot per-keystroke paths where you only keep a small top-K subset,
    /// prefer [`prefix_for_each`](Self::prefix_for_each) — this method
    /// allocates a `Vec<(String, String, u64)>` plus 2 `String`s per entry,
    /// which is ~5MB / ~50ms on short prefixes like `"z"`.
    pub fn prefix_with_freq(&self, prefix: &str) -> Vec<(String, String, u64)> {
        let lower = prefix.to_ascii_lowercase();
        let lo = lower.into_bytes();
        let hi = bump_last(&lo);

        let mut stream = self
            .map
            .range()
            .ge(lo.as_slice())
            .lt(hi.as_slice())
            .into_stream();

        let mut results: Vec<(String, String, u64)> = Vec::new();
        while let Some((key, value)) = stream.next() {
            let Some(sep) = key.iter().position(|b| *b == 0u8) else {
                continue;
            };
            let (pinyin_bytes, rest) = key.split_at(sep);
            let word_bytes = &rest[1..];
            if let (Ok(pinyin), Ok(word)) = (
                core::str::from_utf8(pinyin_bytes),
                core::str::from_utf8(word_bytes),
            ) {
                results.push((pinyin.to_string(), word.to_string(), value));
            }
        }
        results
    }

    // -------------------------------------------------------------------
    // L0 mutation
    // -------------------------------------------------------------------

    /// Record that the user picked `word` for `pinyin`. If this is the
    /// `PROMOTE_THRESHOLD`-th consecutive pick, the word is auto-pinned
    /// and all counters for `pinyin` are cleared. Returns `true` iff this
    /// call caused a promotion.
    ///
    /// Silently no-ops if `(pinyin, word)` isn't in L1 (defends against
    /// the host accidentally feeding us things the user couldn't actually
    /// have selected).
    pub fn record_pick(&self, pinyin: &str, word: &str) -> bool {
        if !self.exists_in_l1(pinyin, word) {
            return false;
        }
        let lower = lower_str(pinyin);
        let Ok(mut l0) = self.l0.write() else {
            return false;
        };
        let key = (lower.clone(), word.to_string());
        let count = l0.pick_counts.entry(key).or_insert(0);
        *count += 1;
        if *count >= PROMOTE_THRESHOLD {
            l0.pins.insert(lower.clone(), word.to_string());
            l0.pick_counts.retain(|(p, _), _| p != &lower);
            return true;
        }
        false
    }

    /// Force-pin a word without going through the pick counter. Validates
    /// against L1; returns whether the pin was applied.
    pub fn pin(&self, pinyin: &str, word: &str) -> bool {
        if !self.exists_in_l1(pinyin, word) {
            return false;
        }
        let lower = lower_str(pinyin);
        let Ok(mut l0) = self.l0.write() else {
            return false;
        };
        l0.pins.insert(lower.clone(), word.to_string());
        l0.pick_counts.retain(|(p, _), _| p != &lower);
        true
    }

    /// Drop the pin for `pinyin` (if any) AND any pick counters for it.
    /// Returns whether any state was removed.
    pub fn forget(&self, pinyin: &str) -> bool {
        let lower = lower_str(pinyin);
        let Ok(mut l0) = self.l0.write() else {
            return false;
        };
        let had_pin = l0.pins.remove(&lower).is_some();
        let len_before = l0.pick_counts.len();
        l0.pick_counts.retain(|(p, _), _| p != &lower);
        had_pin || l0.pick_counts.len() != len_before
    }

    /// Snapshot the entire L0 layer (pins + pick counts) for host-side
    /// persistence. Pair with [`Self::import_l0`] on app startup.
    pub fn export_l0(&self) -> L0Snapshot {
        let Ok(l0) = self.l0.read() else {
            return L0Snapshot::default();
        };
        L0Snapshot {
            pins: l0
                .pins
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            pick_counts: l0
                .pick_counts
                .iter()
                .map(|((p, w), n)| (p.clone(), w.clone(), *n))
                .collect(),
        }
    }

    /// Replace the entire L0 layer with `snap`. Pins / pick_counts whose
    /// `(pinyin, word)` isn't in L1 are silently dropped (lexicon may have
    /// evolved between versions). Returns the count of *accepted* pins.
    pub fn import_l0(&self, snap: L0Snapshot) -> usize {
        let valid_pins: Vec<(String, String)> = snap
            .pins
            .into_iter()
            .filter(|(p, w)| self.exists_in_l1(p, w))
            .collect();
        let valid_counts: Vec<((String, String), u32)> = snap
            .pick_counts
            .into_iter()
            .filter_map(|(p, w, n)| {
                if self.exists_in_l1(&p, &w) {
                    Some(((p, w), n))
                } else {
                    None
                }
            })
            .collect();
        let accepted = valid_pins.len();
        let Ok(mut l0) = self.l0.write() else {
            return 0;
        };
        l0.pins = valid_pins.into_iter().collect();
        l0.pick_counts = valid_counts.into_iter().collect();
        accepted
    }

    fn exists_in_l1(&self, pinyin: &str, word: &str) -> bool {
        self.lookup(pinyin).iter().any(|w| w == word)
    }
}

fn lower_str(s: &str) -> String {
    s.to_ascii_lowercase()
}

fn bump_last(bytes: &[u8]) -> Vec<u8> {
    let mut v = bytes.to_vec();
    if let Some(last) = v.last_mut() {
        if *last < 0xFF {
            *last += 1;
            return v;
        }
    }
    v.push(0xFF);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_loads() {
        let d = PinyinDict::embedded();
        assert!(d.len() >= 50, "bootstrap should have at least 50 entries");
    }

    #[test]
    fn lookup_zhongguo_returns_zhongguo() {
        let d = PinyinDict::embedded();
        let words = d.lookup("zhongguo");
        assert_eq!(words.first().map(String::as_str), Some("中国"));
    }

    #[test]
    fn lookup_wo_returns_wo_first() {
        let d = PinyinDict::embedded();
        let words = d.lookup("wo");
        assert_eq!(words.first().map(String::as_str), Some("我"));
    }

    #[test]
    fn lookup_shi_returns_multiple() {
        let d = PinyinDict::embedded();
        let words = d.lookup("shi");
        assert!(
            words.len() >= 3,
            "expected ≥3 candidates for shi, got {words:?}"
        );
        assert!(words.contains(&"是".to_string()));
    }

    #[test]
    fn lookup_unknown_returns_empty() {
        let d = PinyinDict::embedded();
        assert!(d.lookup("qzqzqz").is_empty());
    }

    #[test]
    fn case_insensitive() {
        let d = PinyinDict::embedded();
        assert_eq!(d.lookup("WO"), d.lookup("wo"));
        assert_eq!(d.lookup("ZhongGuo"), d.lookup("zhongguo"));
    }

    #[test]
    fn lookup_into_reuses_buffer() {
        let d = PinyinDict::embedded();
        let mut buf = Vec::with_capacity(16);
        d.lookup_into("ni", &mut buf);
        let cap_after_first = buf.capacity();
        d.lookup_into("ta", &mut buf);
        // Buffer should reuse the same allocation (or larger).
        assert!(buf.capacity() >= cap_after_first);
        assert_eq!(buf.first().map(String::as_str), Some("他"));
    }

    #[test]
    fn prefix_returns_sorted_pairs() {
        let d = PinyinDict::embedded();
        let pairs = d.prefix("zhong");
        // Should include both "zhong" entries (单字) and "zhongguo".
        assert!(pairs.iter().any(|(p, _)| p == "zhong"));
        assert!(pairs.iter().any(|(p, w)| p == "zhongguo" && w == "中国"));
    }

    // -------------------------------------------------------------------
    // L0 ranking (item 27) — these tests rely on real data having multiple
    // candidates per pinyin. Bootstrap dict is too thin (single-candidate
    // entries dominate) so they're gated to default features.
    // -------------------------------------------------------------------

    #[cfg(not(feature = "bootstrap_only"))]
    #[test]
    fn l0_starts_empty() {
        let d = PinyinDict::embedded();
        assert_eq!(d.l0_pin_count(), 0);
        assert_eq!(d.l0_pending_count(), 0);
    }

    #[cfg(not(feature = "bootstrap_only"))]
    #[test]
    fn record_pick_promotes_after_threshold() {
        let d = PinyinDict::embedded();
        // shi has many candidates; pick a non-default one and pin it via
        // 3 picks. 时 is a real shi-reading entry.
        let target = "时";
        for _ in 0..(PROMOTE_THRESHOLD - 1) {
            assert!(!d.record_pick("shi", target));
        }
        assert!(d.record_pick("shi", target), "should promote on Nth pick");
        assert_eq!(d.lookup("shi").first().map(String::as_str), Some(target));
        assert_eq!(d.l0_pin_count(), 1);
        // Counters reset on promotion.
        assert_eq!(d.l0_pending_count(), 0);
    }

    #[cfg(not(feature = "bootstrap_only"))]
    #[test]
    fn record_pick_resets_on_promotion_so_others_must_earn_3_again() {
        let d = PinyinDict::embedded();
        for _ in 0..PROMOTE_THRESHOLD {
            d.record_pick("shi", "时");
        }
        // Now picking 事 once shouldn't auto-flip.
        assert!(!d.record_pick("shi", "事"));
        assert_eq!(d.lookup("shi").first().map(String::as_str), Some("时"));
        // But three picks of 事 will dethrone 时.
        for _ in 0..(PROMOTE_THRESHOLD - 1) {
            d.record_pick("shi", "事");
        }
        assert_eq!(d.lookup("shi").first().map(String::as_str), Some("事"));
    }

    #[cfg(not(feature = "bootstrap_only"))]
    #[test]
    fn record_pick_rejects_unknown_word() {
        let d = PinyinDict::embedded();
        for _ in 0..PROMOTE_THRESHOLD {
            assert!(!d.record_pick("shi", "this_is_not_a_real_word"));
        }
        assert_eq!(d.l0_pin_count(), 0);
        assert_eq!(d.l0_pending_count(), 0);
    }

    #[cfg(not(feature = "bootstrap_only"))]
    #[test]
    fn pin_force_pins_without_counters() {
        let d = PinyinDict::embedded();
        assert!(d.pin("shi", "时"));
        assert_eq!(d.lookup("shi").first().map(String::as_str), Some("时"));
        // Pin counter wasn't incremented.
        assert_eq!(d.l0_pending_count(), 0);
    }

    #[cfg(not(feature = "bootstrap_only"))]
    #[test]
    fn forget_clears_pin_and_counters() {
        let d = PinyinDict::embedded();
        d.pin("shi", "时");
        d.record_pick("shi", "事");
        assert!(d.forget("shi"));
        assert_eq!(d.l0_pin_count(), 0);
        assert_eq!(d.l0_pending_count(), 0);
    }

    #[cfg(not(feature = "bootstrap_only"))]
    #[test]
    fn export_import_roundtrip() {
        let d = PinyinDict::embedded();
        d.pin("shi", "时");
        d.record_pick("zhongguo", "中国"); // also valid; counter gets one tick
        let snap = d.export_l0();
        assert_eq!(snap.pins.len(), 1);
        assert_eq!(snap.pick_counts.len(), 1);

        d.forget("shi");
        d.forget("zhongguo");
        assert_eq!(d.l0_pin_count(), 0);

        let accepted = d.import_l0(snap);
        assert_eq!(accepted, 1);
        assert_eq!(d.lookup("shi").first().map(String::as_str), Some("时"));
    }

    #[cfg(not(feature = "bootstrap_only"))]
    #[test]
    fn import_drops_invalid_entries() {
        let d = PinyinDict::embedded();
        let snap = L0Snapshot {
            pins: vec![
                ("shi".into(), "时".into()),
                ("shi".into(), "bogus_word".into()),
            ],
            pick_counts: vec![("shi".into(), "ghost_word".into(), 2)],
        };
        let accepted = d.import_l0(snap);
        assert_eq!(accepted, 1);
        assert_eq!(d.l0_pending_count(), 0);
    }

    // L0 surface compiles + works on bootstrap too — minimum smoke without
    // requiring multi-candidate predicates.
    #[test]
    fn l0_pin_pin_lookup_compiles() {
        let d = PinyinDict::embedded();
        // 中国 exists in both bootstrap + full datasets.
        assert!(d.pin("zhongguo", "中国"));
        assert_eq!(d.l0_pin_count(), 1);
        assert!(d.forget("zhongguo"));
        assert_eq!(d.l0_pin_count(), 0);
    }
}
