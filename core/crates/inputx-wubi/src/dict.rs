//! FST-backed Wubi dictionary with a two-tier ranking model.
//!
//! # L0 / L1+
//!
//! - **L1+** is the immutable lexicon: the embedded FST built at compile
//!   time, plus a per-entry [`Layer`] tag and a per-entry frequency score.
//!   Every entry's nominal weight is `LAYER_BASE[layer] + freq_score`. Future
//!   immutable layers (e.g., a per-app dictionary shipped by the host) can
//!   stack on top with the same shape.
//! - **L0** is a thin, per-user override layer:
//!   - **Pinned candidates** — `code → preferred_word`. A pin moves that word
//!     to position 0 in `lookup`'s output, regardless of L1+ weight.
//!   - **Pick counters** — `(code, word) → u32`. [`WubiDict::record_pick`] increments
//!     the counter; once it hits [`PROMOTE_THRESHOLD`], the word is auto-
//!     pinned and all counters for that code are reset (so a later, different
//!     pick has to earn its 3 votes from scratch — prevents thrashing).
//!   - **Layer preferences** — `Layer → f64` multiplier (default 1.0, with
//!     `Auto = 0.7` so extension characters don't dominate). Applied to the
//!     L1 nominal weight at sort time. Settable via API; **not** auto-tuned.
//!
//! Layer prefs reorder *within* L1; pins override the resulting ordering at
//! position 0. So in steady state most codes have empty L0 and the layer
//! base ordering wins (hence "L0 default ≈ L1 default").

use std::collections::HashMap;
use std::sync::RwLock;

use fst::{IntoStreamer, Map, Streamer};

use crate::layer::{DEFAULT_LAYER_PREFS, LAYER_COUNT, Layer, unpack};

const DICT_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/wubi86.fst"));

/// Number of consecutive picks of the same `(code, word)` required before
/// L0 auto-pins it. Defaults to 3; can be overridden at build time via the
/// `WUBI_PROMOTE_THRESHOLD` env var (developer escape hatch — not exposed
/// to end users).
pub const PROMOTE_THRESHOLD: u32 = parse_threshold_const();

const fn parse_threshold_const() -> u32 {
    match option_env!("WUBI_PROMOTE_THRESHOLD") {
        Some(s) => parse_u32_const(s),
        None => 3,
    }
}

const fn parse_u32_const(s: &str) -> u32 {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        panic!("WUBI_PROMOTE_THRESHOLD must not be empty");
    }
    let mut i = 0;
    let mut n: u32 = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b < b'0' || b > b'9' {
            panic!("WUBI_PROMOTE_THRESHOLD must be ASCII digits");
        }
        n = n * 10 + (b - b'0') as u32;
        i += 1;
    }
    if n == 0 {
        panic!("WUBI_PROMOTE_THRESHOLD must be >= 1");
    }
    n
}

/// Persistent state of the L0 layer. Caller serializes / deserializes this
/// however it likes (TOML, MessagePack, sqlite, …) — the crate intentionally
/// has no `serde` dependency.
#[derive(Debug, Clone)]
pub struct L0Snapshot {
    /// `(code, word)` pairs the user has pinned (manually or via `record_pick`
    /// reaching threshold).
    pub pins: Vec<(String, String)>,
    /// `(code, word, count)` — pending pick counts that haven't yet reached
    /// `PROMOTE_THRESHOLD`. Snapshot semantics are best-effort; a count of
    /// `threshold - 1` restored after restart needs only one more pick to
    /// promote.
    pub pick_counts: Vec<(String, String, u32)>,
    /// Layer multipliers, indexed by `Layer as usize`.
    pub layer_prefs: [f64; LAYER_COUNT],
}

#[derive(Default)]
struct L0Inner {
    pins: HashMap<String, String>,
    pick_counts: HashMap<(String, String), u32>,
    layer_prefs: [f64; LAYER_COUNT],
}

impl L0Inner {
    fn new() -> Self {
        Self {
            pins: HashMap::new(),
            pick_counts: HashMap::new(),
            layer_prefs: DEFAULT_LAYER_PREFS,
        }
    }
}

/// The Wubi 86 dictionary: an embedded FST plus a mutable L0 layer for
/// per-user preference learning.
///
/// All read methods take `&self`. L0 mutations (`record_pick`, `pin`,
/// `forget`, `set_layer_pref`, `import_l0`) also take `&self` — interior
/// mutability via `RwLock` lets a single shared instance feed every
/// concurrent IME / WASM session without exposing the lock to the caller.
pub struct WubiDict {
    map: Map<&'static [u8]>,
    l0: RwLock<L0Inner>,
}

impl WubiDict {
    /// Construct the dictionary from the embedded FST. Cheap (validates the
    /// FST header and initializes an empty L0); callers should still cache
    /// the instance and reuse it for the program lifetime.
    pub fn embedded() -> Self {
        Self {
            map: Map::new(DICT_BYTES).expect("invalid embedded FST"),
            l0: RwLock::new(L0Inner::new()),
        }
    }

    /// Total number of FST entries (one per `(code, word)` pair). Not the
    /// number of unique characters or codes.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// `true` iff the FST has zero entries. Always `false` for the embedded
    /// build; useful as a future-proofing check if the dict source becomes
    /// configurable.
    pub fn is_empty(&self) -> bool {
        self.map.len() == 0
    }

    /// Number of L0 pinned codes.
    pub fn l0_pin_count(&self) -> usize {
        self.l0.read().map(|g| g.pins.len()).unwrap_or(0)
    }

    /// Number of distinct `(code, word)` pairs with pending pick counters.
    pub fn l0_pending_count(&self) -> usize {
        self.l0.read().map(|g| g.pick_counts.len()).unwrap_or(0)
    }

    // -------------------------------------------------------------------
    // Lookups
    // -------------------------------------------------------------------

    /// Words for the exact code, ordered by:
    ///   1. L0 pin (if any) at index 0,
    ///   2. then `LAYER_BASE[layer] * layer_prefs[layer] + freq_score` desc,
    ///   3. then FST byte order (stable tiebreaker).
    ///
    /// Allocates a fresh `Vec`. Hot-loop callers (the IME's per-keystroke
    /// candidate refresh) should use [`Self::lookup_into`] to reuse a
    /// caller-owned buffer.
    pub fn lookup(&self, code: &str) -> Vec<String> {
        let mut out = Vec::new();
        self.lookup_into(code, &mut out);
        out
    }

    /// Same as [`Self::lookup`] but writes into a caller-owned buffer.
    /// `out` is cleared (capacity preserved) on entry.
    ///
    /// Reuses the result buffer's allocation across calls — a measurable win
    /// for the IME's per-keystroke candidate refresh, where the candidate
    /// list is rebuilt thousands of times per typing session. Per-candidate
    /// `String`s are still freshly allocated (the FST stream yields owned
    /// bytes; the crate doesn't expose `&'static str` because the stream's
    /// borrow doesn't outlive the call).
    pub fn lookup_into(&self, code: &str, out: &mut Vec<String>) {
        out.clear();

        // Inline lookup_with_meta to avoid the intermediate Vec allocation.
        let lower = code.to_ascii_lowercase();
        let mut prefix = lower.into_bytes();
        let prefix_len = prefix.len();
        prefix.push(0u8);

        let mut upper = prefix.clone();
        let last = upper.len() - 1;
        upper[last] = 0x01;

        let prefs = self
            .l0
            .read()
            .map(|g| g.layer_prefs)
            .unwrap_or(DEFAULT_LAYER_PREFS);

        // Score during the FST scan; reuse a small scratch Vec.
        // Tuple: (word, score, promote_to_top).
        //
        // `promote_to_top` encodes the wubi-86 "full-code single-char wins"
        // rule: at a fully-typed 4-letter code, a single-char entry with
        // non-zero corpus frequency should rank above any phrase sharing
        // the same code. Without it, Auto-layer single chars (e.g. gmww →
        // 两, layer Auto base ~100k, freq 37372) silently lose to Phrase-
        // layer 4-char idioms (e.g. 两败俱伤, layer Phrase base ~400k)
        // because the layer base swamps per-entry freq.
        //
        // The `freq > 0` gate stops the rule from promoting CJK-extension
        // single chars that have no corpus presence (e.g. khlg → 䟧 freq 0)
        // above the canonical phrase (khlg → 中国 freq 44985) — those
        // rare-char entries are correctly classified as "exists but isn't
        // what anyone typing this code meant".
        let full_code = prefix_len == 4;
        let mut scratch: Vec<(String, f64, bool)> = Vec::with_capacity(8);
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
                let (layer, freq) = unpack(value);
                let base = layer.base() as f64;
                let pref = prefs[layer.as_index()];
                let is_single = s.chars().count() == 1;
                let promote = full_code && is_single && freq > 0;
                scratch.push((s.to_string(), base * pref + freq as f64, promote));
            }
        }
        scratch.sort_by(|a, b| {
            if a.2 != b.2 {
                return if a.2 {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                };
            }
            b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
        });

        out.reserve(scratch.len());
        for (w, _, _) in scratch.drain(..) {
            out.push(w);
        }

        // L0 pin: pull to position 0.
        if let Ok(l0) = self.l0.read() {
            if let Some(pref) = l0.pins.get(code) {
                if let Some(idx) = out.iter().position(|w| w == pref) {
                    if idx > 0 {
                        let p = out.remove(idx);
                        out.insert(0, p);
                    }
                }
            }
        }
    }

    /// Same as [`Self::lookup`] but exposes the `(word, layer, freq_score)` triples
    /// in the FST's natural byte order. Callers that want to apply their
    /// own ranking can start from this.
    pub fn lookup_with_meta(&self, code: &str) -> Vec<(String, Layer, u64)> {
        let lower = code.to_ascii_lowercase();
        let mut prefix = lower.into_bytes();
        let prefix_len = prefix.len();
        prefix.push(0u8);

        let mut upper = prefix.clone();
        let last = upper.len() - 1;
        upper[last] = 0x01;

        let mut stream = self
            .map
            .range()
            .ge(prefix.as_slice())
            .lt(upper.as_slice())
            .into_stream();

        let mut results = Vec::new();
        while let Some((key, value)) = stream.next() {
            if key.len() <= prefix_len + 1 {
                continue;
            }
            let word_bytes = &key[prefix_len + 1..];
            if let Ok(s) = core::str::from_utf8(word_bytes) {
                let (layer, freq) = unpack(value);
                results.push((s.to_string(), layer, freq));
            }
        }
        results
    }

    /// All `(code, word)` pairs with code starting with `prefix`, ordered by
    /// (effective L1 weight desc, code, word). Pins are NOT applied here —
    /// they're per-code and don't generalize across a prefix scan.
    pub fn prefix(&self, prefix: &str) -> Vec<(String, String)> {
        let lower = prefix.to_ascii_lowercase();
        let lo = lower.as_bytes().to_vec();
        let hi = bump_last(&lo);

        let prefs = self
            .l0
            .read()
            .map(|g| g.layer_prefs)
            .unwrap_or(DEFAULT_LAYER_PREFS);

        let mut stream = self
            .map
            .range()
            .ge(lo.as_slice())
            .lt(hi.as_slice())
            .into_stream();

        let mut results: Vec<(String, String, f64)> = Vec::new();
        while let Some((key, value)) = stream.next() {
            let Some(sep) = key.iter().position(|b| *b == 0u8) else {
                continue;
            };
            let (code_bytes, rest) = key.split_at(sep);
            let word_bytes = &rest[1..];
            if let (Ok(code), Ok(word)) = (
                core::str::from_utf8(code_bytes),
                core::str::from_utf8(word_bytes),
            ) {
                let (layer, freq) = unpack(value);
                let score = layer.base() as f64 * prefs[layer.as_index()] + freq as f64;
                results.push((code.to_string(), word.to_string(), score));
            }
        }
        results.sort_by(|a, b| {
            b.2.partial_cmp(&a.2)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
                .then(a.1.cmp(&b.1))
        });
        results.into_iter().map(|(c, w, _)| (c, w)).collect()
    }

    // -------------------------------------------------------------------
    // L0 mutation
    // -------------------------------------------------------------------

    /// Record that the user picked `word` for `code`. If this is the
    /// `PROMOTE_THRESHOLD`-th consecutive pick, the word is auto-pinned and
    /// all counters for `code` are cleared. Returns `true` iff this call
    /// caused a promotion.
    ///
    /// Silently no-ops if `(code, word)` isn't in L1 (defends against the
    /// host accidentally feeding us things the user couldn't actually have
    /// selected from candidates).
    pub fn record_pick(&self, code: &str, word: &str) -> bool {
        if !self.exists_in_l1(code, word) {
            return false;
        }
        let Ok(mut l0) = self.l0.write() else {
            return false;
        };
        let key = (code.to_string(), word.to_string());
        let count = l0.pick_counts.entry(key).or_insert(0);
        *count += 1;
        if *count >= PROMOTE_THRESHOLD {
            l0.pins.insert(code.to_string(), word.to_string());
            l0.pick_counts.retain(|(c, _), _| c != code);
            return true;
        }
        false
    }

    /// Force-pin a word without going through the pick counter. Validates
    /// against L1; returns whether the pin was applied.
    pub fn pin(&self, code: &str, word: &str) -> bool {
        if !self.exists_in_l1(code, word) {
            return false;
        }
        let Ok(mut l0) = self.l0.write() else {
            return false;
        };
        l0.pins.insert(code.to_string(), word.to_string());
        l0.pick_counts.retain(|(c, _), _| c != code);
        true
    }

    /// Drop the pin for `code` (if any) AND any pick counters for it. Returns
    /// whether any state was removed.
    pub fn forget(&self, code: &str) -> bool {
        let Ok(mut l0) = self.l0.write() else {
            return false;
        };
        let had_pin = l0.pins.remove(code).is_some();
        let len_before = l0.pick_counts.len();
        l0.pick_counts.retain(|(c, _), _| c != code);
        had_pin || l0.pick_counts.len() != len_before
    }

    /// Set the multiplier for `layer`. Negative or non-finite values are
    /// clamped to 0.0 (silently — they're nonsensical for ranking).
    pub fn set_layer_pref(&self, layer: Layer, multiplier: f64) {
        let m = if multiplier.is_finite() && multiplier >= 0.0 {
            multiplier
        } else {
            0.0
        };
        if let Ok(mut l0) = self.l0.write() {
            l0.layer_prefs[layer.as_index()] = m;
        }
    }

    /// Current multiplier for `layer`. Returns the default value if the
    /// internal lock is poisoned (treat as best-effort).
    pub fn layer_pref(&self, layer: Layer) -> f64 {
        self.l0
            .read()
            .map(|g| g.layer_prefs[layer.as_index()])
            .unwrap_or(DEFAULT_LAYER_PREFS[layer.as_index()])
    }

    /// Snapshot the entire L0 layer (pins + pick counts + layer prefs) for
    /// host-side persistence. Pair with [`WubiDict::import_l0`] on app
    /// startup.
    pub fn export_l0(&self) -> L0Snapshot {
        let Ok(l0) = self.l0.read() else {
            return L0Snapshot {
                pins: Vec::new(),
                pick_counts: Vec::new(),
                layer_prefs: DEFAULT_LAYER_PREFS,
            };
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
                .map(|((c, w), n)| (c.clone(), w.clone(), *n))
                .collect(),
            layer_prefs: l0.layer_prefs,
        }
    }

    /// Replace the entire L0 layer with `snap`. Pins / pick_counts whose
    /// `(code, word)` isn't in L1 are silently dropped (lexicon may have
    /// evolved between versions). Returns the count of *accepted* pins.
    pub fn import_l0(&self, snap: L0Snapshot) -> usize {
        // Validate everything against L1 before touching state, then commit.
        let valid_pins: Vec<(String, String)> = snap
            .pins
            .into_iter()
            .filter(|(c, w)| self.exists_in_l1(c, w))
            .collect();
        let valid_counts: Vec<((String, String), u32)> = snap
            .pick_counts
            .into_iter()
            .filter_map(|(c, w, n)| {
                if self.exists_in_l1(&c, &w) {
                    Some(((c, w), n))
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
        l0.layer_prefs = snap.layer_prefs;
        accepted
    }

    fn exists_in_l1(&self, code: &str, word: &str) -> bool {
        self.lookup_with_meta(code)
            .iter()
            .any(|(w, _, _)| w == word)
    }
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
        let d = WubiDict::embedded();
        assert!(d.len() >= 50);
    }

    #[test]
    fn jianma1_g_returns_yi_first() {
        let d = WubiDict::embedded();
        let words = d.lookup("g");
        assert_eq!(words.first().map(String::as_str), Some("一"));
    }

    #[test]
    fn khlg_phrase_outranks_extension_char() {
        let d = WubiDict::embedded();
        let words = d.lookup("khlg");
        let zg = words.iter().position(|w| w == "中国");
        let ext = words.iter().position(|w| w == "䟧");
        if let (Some(zg), Some(ext)) = (zg, ext) {
            assert!(zg < ext, "中国 should rank above 䟧, got {words:?}");
        }
    }

    #[test]
    fn rrrr_keyname_outranks_phrase() {
        let d = WubiDict::embedded();
        let words = d.lookup("rrrr");
        let bai = words.iter().position(|w| w == "白");
        let zhua = words.iter().position(|w| w == "抓拍");
        if let (Some(bai), Some(zhua)) = (bai, zhua) {
            assert!(bai < zhua, "白 should rank above 抓拍, got {words:?}");
        }
    }

    #[test]
    fn record_pick_promotes_after_threshold() {
        let d = WubiDict::embedded();
        // Three picks → promoted.
        assert!(!d.record_pick("khlg", "跑车"));
        assert!(!d.record_pick("khlg", "跑车"));
        assert!(d.record_pick("khlg", "跑车"));
        assert_eq!(d.lookup("khlg").first().map(String::as_str), Some("跑车"));
        assert_eq!(d.l0_pin_count(), 1);
        // Counters reset on promotion.
        assert_eq!(d.l0_pending_count(), 0);
    }

    #[test]
    fn record_pick_resets_on_promotion_so_others_must_earn_3_again() {
        let d = WubiDict::embedded();
        // Promote 跑车 first.
        for _ in 0..3 {
            d.record_pick("khlg", "跑车");
        }
        // Now picking 中国 once shouldn't auto-flip.
        assert!(!d.record_pick("khlg", "中国"));
        assert_eq!(d.lookup("khlg").first().map(String::as_str), Some("跑车"));
        // But three picks of 中国 will dethrone 跑车.
        assert!(!d.record_pick("khlg", "中国"));
        assert!(d.record_pick("khlg", "中国"));
        assert_eq!(d.lookup("khlg").first().map(String::as_str), Some("中国"));
    }

    #[test]
    fn record_pick_rejects_unknown_word() {
        let d = WubiDict::embedded();
        for _ in 0..PROMOTE_THRESHOLD {
            assert!(!d.record_pick("khlg", "this_is_not_a_real_word"));
        }
        assert_eq!(d.l0_pin_count(), 0);
        assert_eq!(d.l0_pending_count(), 0);
    }

    #[test]
    fn pin_force_pins_without_counters() {
        let d = WubiDict::embedded();
        assert!(d.pin("khlg", "跑车"));
        assert_eq!(d.lookup("khlg").first().map(String::as_str), Some("跑车"));
    }

    #[test]
    fn forget_clears_pin_and_counters() {
        let d = WubiDict::embedded();
        d.pin("khlg", "跑车");
        d.record_pick("khlg", "中国");
        assert!(d.forget("khlg"));
        assert_eq!(d.lookup("khlg").first().map(String::as_str), Some("中国"));
        assert_eq!(d.l0_pin_count(), 0);
        assert_eq!(d.l0_pending_count(), 0);
    }

    #[test]
    fn layer_pref_can_demote_a_layer() {
        let d = WubiDict::embedded();
        // Phrase normally beats Auto. Demote Phrase to 0 → Auto wins (if any).
        // Use a code with both phrase and auto candidates: khlg has 中国 (Phrase)
        // and 䟧 (Auto). Default Auto pref is 0.7 so Phrase still wins; bump
        // Auto to 5.0 to flip.
        d.set_layer_pref(Layer::Phrase, 0.0);
        d.set_layer_pref(Layer::Auto, 5.0);
        let words = d.lookup("khlg");
        let ext = words.iter().position(|w| w == "䟧");
        let zg = words.iter().position(|w| w == "中国");
        if let (Some(ext), Some(zg)) = (ext, zg) {
            assert!(
                ext < zg,
                "with Phrase=0 and Auto=5, 䟧 should outrank 中国, got {words:?}"
            );
        }
    }

    #[test]
    fn export_import_roundtrip() {
        let d = WubiDict::embedded();
        d.pin("khlg", "跑车");
        d.record_pick("wqvb", "您好");
        d.set_layer_pref(Layer::Phrase, 1.5);
        let snap = d.export_l0();
        assert_eq!(snap.pins.len(), 1);
        assert_eq!(snap.pick_counts.len(), 1);
        assert!((snap.layer_prefs[Layer::Phrase.as_index()] - 1.5).abs() < f64::EPSILON);

        d.forget("khlg");
        d.forget("wqvb");
        d.set_layer_pref(Layer::Phrase, 1.0);
        assert_eq!(d.l0_pin_count(), 0);

        let accepted = d.import_l0(snap);
        assert_eq!(accepted, 1);
        assert_eq!(d.lookup("khlg").first().map(String::as_str), Some("跑车"));
        assert!((d.layer_pref(Layer::Phrase) - 1.5).abs() < f64::EPSILON);
    }

    #[test]
    fn import_drops_invalid_entries() {
        let d = WubiDict::embedded();
        let snap = L0Snapshot {
            pins: vec![
                ("khlg".into(), "中国".into()),
                ("khlg".into(), "bogus".into()),
            ],
            pick_counts: vec![("khlg".into(), "ghost".into(), 2)],
            layer_prefs: DEFAULT_LAYER_PREFS,
        };
        let accepted = d.import_l0(snap);
        assert_eq!(accepted, 1);
        assert_eq!(d.l0_pending_count(), 0);
    }

    #[test]
    fn set_layer_pref_clamps_negatives_and_nan() {
        let d = WubiDict::embedded();
        d.set_layer_pref(Layer::Phrase, -3.0);
        assert_eq!(d.layer_pref(Layer::Phrase), 0.0);
        d.set_layer_pref(Layer::Phrase, f64::NAN);
        assert_eq!(d.layer_pref(Layer::Phrase), 0.0);
    }

    // Compile-time check — `PROMOTE_THRESHOLD` is a `const`, so a runtime
    // assertion would be trivially true (and clippy flags it). A `const _`
    // assertion fails at compile time if anyone ever sets it to 0.
    const _: () = assert!(PROMOTE_THRESHOLD >= 1);
}
