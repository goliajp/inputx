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
//!     the counter. Counters are usage statistics ONLY: they never change
//!     ranking. Auto-pin was removed 2026-07-20 (user: "整个自动置顶都关了
//!     吧，没必要这个功能") — repeatedly picking a non-top candidate used to
//!     silently pin it at position 0, which made candidate order drift under
//!     the user instead of staying at the dictionary's ruling.
//!   - **Layer preferences** — `Layer → f64` multiplier (default 1.0, with
//!     `Auto = 0.7` so extension characters don't dominate). Applied to the
//!     L1 nominal weight at sort time. Settable via API; **not** auto-tuned.
//!
//! Layer prefs reorder *within* L1; pins override the resulting ordering at
//! position 0. So in steady state most codes have empty L0 and the layer
//! base ordering wins (hence "L0 default ≈ L1 default").

use std::collections::HashMap;
use std::sync::RwLock;

use inputx_fsa::Dict;

use crate::layer::{DEFAULT_LAYER_PREFS, LAYER_COUNT, Layer, unpack};

const DICT_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/wubi86.dict"));

/// Persistent state of the L0 layer. Caller serializes / deserializes this
/// however it likes (TOML, MessagePack, sqlite, …) — the crate intentionally
/// has no `serde` dependency.
#[derive(Debug, Clone)]
pub struct L0Snapshot {
    /// `(code, word)` pairs the user has pinned. Only `pin` creates these —
    /// picking a candidate never does (auto-pin removed 2026-07-20).
    pub pins: Vec<(String, String)>,
    /// `(code, word, count)` — how often the user picked each candidate.
    /// Usage statistics only; does not affect ranking.
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
    map: Dict<&'static [u8]>,
    l0: RwLock<L0Inner>,
}

impl WubiDict {
    /// Construct the dictionary from the embedded FST. Cheap (validates the
    /// FST header and initializes an empty L0); callers should still cache
    /// the instance and reuse it for the program lifetime.
    pub fn embedded() -> Self {
        Self {
            map: Dict::new(DICT_BYTES).expect("invalid embedded wubi dict"),
            l0: RwLock::new(L0Inner::new()),
        }
    }

    /// Number of distinct codes in the dictionary. (The two-level `Dict`
    /// counts codes, not total (code, word) pairs.)
    pub fn len(&self) -> usize {
        self.map.len() as usize
    }

    /// `true` iff the dictionary has zero codes.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
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

    /// Scored lookup: same ordering as `lookup_into` (layer / freq / promote
    /// rules + L0 pin), but emits `(word, score)` tuples so the cross-engine
    /// merge layer can do a single unified sort instead of hard-coding which
    /// engine wins. Score reflects:
    ///   * layer.base() × layer_prefs   (jianma1 = 1e6, …)
    ///   * + freq                       (corpus weight)
    ///   * × 100.0                      if full-code single-char promotion fires
    ///     (see lookup_into doc for the rule)
    ///   * × 1000.0                     if the candidate is L0-pinned
    ///     (must dominate any natural score)
    ///
    /// The post-multipliers keep wubi simcodes and L0 pins on top across
    /// the cross-engine merge.
    pub fn lookup_with_scores_into(&self, code: &str, out: &mut Vec<(String, f64)>) {
        let mut layered = Vec::with_capacity(out.capacity());
        self.lookup_with_layer_into(code, &mut layered);
        out.clear();
        out.reserve(layered.len());
        for (w, score, _layer) in layered.drain(..) {
            out.push((w, score));
        }
    }

    /// Layer-aware scored lookup: identical to `lookup_with_scores_into`
    /// but each candidate also carries its origin `Layer`. The composite
    /// engine uses the layer tag to make context-aware ranking decisions
    /// — e.g. demoting low-confidence Auto / Phrase entries at short
    /// pinyin-shaped input while keeping high-confidence Jianma1/2/3 +
    /// Zigen simcodes at full strength (the 伙 vs 嶙 distinction —
    /// 伙 is Jianma2 wubi-simcode and must lead at #0 for its code,
    /// 嶙 is typically Auto-layer and should not displace pinyin top).
    pub fn lookup_with_layer_into(&self, code: &str, out: &mut Vec<(String, f64, Layer)>) {
        out.clear();
        let lower = code.to_ascii_lowercase();

        let prefs = self
            .l0
            .read()
            .map(|g| g.layer_prefs)
            .unwrap_or(DEFAULT_LAYER_PREFS);

        let full_code = lower.len() == 4;
        // Tuple: (word, score, is_single, freq, layer).
        let mut scratch: Vec<(String, f64, bool, u64, Layer)> = Vec::with_capacity(8);
        let mut max_phrase_freq: u64 = 0;
        self.map.get_for_each(lower.as_bytes(), |word, value| {
            if let Ok(s) = core::str::from_utf8(word) {
                let (layer, freq) = unpack(value);
                let base = layer.base() as f64;
                let pref = prefs[layer.as_index()];
                let is_single = s.chars().count() == 1;
                if !is_single && freq > max_phrase_freq {
                    max_phrase_freq = freq;
                }
                scratch.push((
                    s.to_string(),
                    base * pref + freq as f64,
                    is_single,
                    freq,
                    layer,
                ));
            }
        });

        // Apply full-code single-char promote (lifts qualifying single
        // chars above the same-code phrases) and L0 pin (lifts the pinned
        // word above natural sort).
        let pinned: Option<String> = self
            .l0
            .read()
            .ok()
            .and_then(|g| g.pins.get(&lower).cloned());
        for e in scratch.iter_mut() {
            let promote = full_code && e.2 && e.3 > max_phrase_freq;
            if promote {
                e.1 *= 100.0;
            }
            if let Some(p) = &pinned
                && &e.0 == p
            {
                e.1 *= 1000.0;
            }
        }
        scratch.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        out.reserve(scratch.len());
        for (w, score, _, _, layer) in scratch.drain(..) {
            out.push((w, score, layer));
        }
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

        let lower = code.to_ascii_lowercase();
        let prefix_len = lower.len();

        let prefs = self
            .l0
            .read()
            .map(|g| g.layer_prefs)
            .unwrap_or(DEFAULT_LAYER_PREFS);

        // Score during the FST scan; reuse a small scratch Vec.
        // Tuple: (word, score, is_single_char, freq, is_phrase).
        //
        // The wubi-86 "full-code single-char wins" rule applied here:
        // at a fully-typed 4-letter code, a single-char entry whose
        // corpus frequency *exceeds the highest phrase frequency at
        // the same code* gets promoted above all phrases. Otherwise
        // the standard score ordering (layer_base × pref + freq) wins.
        //
        // Why this shape (relative freq comparison, not absolute):
        //
        //   - gmww 两 (Auto, freq 37372) vs 两败俱伤 (Phrase, freq 15272):
        //     37372 > 15272 → 两 promoted. ✓
        //
        //   - wcng 鹟 (Auto, freq 5961) vs 公司 (Phrase, freq 42817):
        //     5961 < 42817 → 鹟 stays at its natural Auto score (low),
        //     公司 wins on layer_base alone. ✓
        //
        //   - khlg 䟧 (Auto, freq 0) vs 中国 (Phrase, freq 44985):
        //     0 < 44985 → 䟧 stays low, 中国 wins. ✓
        //
        // The earlier absolute-`freq > 0` gate worked for gmww but
        // wrongly promoted any uncommon-but-corpus-present single char
        // over a popular phrase (the wcng case the user just flagged).
        let full_code = prefix_len == 4;
        let mut scratch: Vec<(String, f64, bool, u64)> = Vec::with_capacity(8);
        // Track the highest phrase frequency at this code so the
        // promote decision can be made after the scan.
        let mut max_phrase_freq: u64 = 0;
        self.map.get_for_each(lower.as_bytes(), |word, value| {
            if let Ok(s) = core::str::from_utf8(word) {
                let (layer, freq) = unpack(value);
                let base = layer.base() as f64;
                let pref = prefs[layer.as_index()];
                let is_single = s.chars().count() == 1;
                if !is_single && freq > max_phrase_freq {
                    max_phrase_freq = freq;
                }
                scratch.push((s.to_string(), base * pref + freq as f64, is_single, freq));
            }
        });
        scratch.sort_by(|a, b| {
            let a_promote = full_code && a.2 && a.3 > max_phrase_freq;
            let b_promote = full_code && b.2 && b.3 > max_phrase_freq;
            if a_promote != b_promote {
                return if a_promote {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                };
            }
            b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
        });

        out.reserve(scratch.len());
        for (w, _, _, _) in scratch.drain(..) {
            out.push(w);
        }

        // L0 pin: pull to position 0.
        if let Ok(l0) = self.l0.read()
            && let Some(pref) = l0.pins.get(code)
            && let Some(idx) = out.iter().position(|w| w == pref)
            && idx > 0
        {
            let p = out.remove(idx);
            out.insert(0, p);
        }
    }

    /// Same as [`Self::lookup`] but exposes the `(word, layer, freq_score)` triples
    /// in the FST's natural byte order. Callers that want to apply their
    /// own ranking can start from this.
    pub fn lookup_with_meta(&self, code: &str) -> Vec<(String, Layer, u64)> {
        let lower = code.to_ascii_lowercase();
        let mut results = Vec::new();
        self.map.get_for_each(lower.as_bytes(), |word, value| {
            if let Ok(s) = core::str::from_utf8(word) {
                let (layer, freq) = unpack(value);
                results.push((s.to_string(), layer, freq));
            }
        });
        results
    }

    /// Prefix-prediction lookups: all `(word, freq, code_len)` triples where
    /// `code` strictly extends `prefix` (i.e., `code_len > prefix.len()`).
    /// Exact-code matches are excluded — those are not predictions.
    ///
    /// Returned tuples are ordered by `freq` descending, then `word` ascending
    /// (FST byte order tiebreaker). Pins are NOT applied (per-code; prefix
    /// scan can't generalize). Used by the composite dispatch to attach Wubi
    /// prediction candidates in Mixed mode (e.g., `jj` → 日, 时, 旧 as
    /// predictions in addition to exact 是/我).
    ///
    /// Raw frequency is returned (not score) so the caller can compose the
    /// final score via `scoring::predict_score(base, freq, freq_mult,
    /// proximity)` where `proximity = typed_len / code_len`.
    pub fn prefix_predictions(&self, prefix: &str) -> Vec<(String, u64, usize)> {
        let lower = prefix.to_ascii_lowercase();
        let prefix_len = lower.len();
        let mut results: Vec<(String, u64, usize)> = Vec::new();
        self.map
            .prefix_for_each(lower.as_bytes(), |code_bytes, word_bytes, value| {
                if code_bytes.len() <= prefix_len {
                    return;
                }
                if let (Ok(_code), Ok(word)) = (
                    core::str::from_utf8(code_bytes),
                    core::str::from_utf8(word_bytes),
                ) {
                    let (_layer, freq) = unpack(value);
                    results.push((word.to_string(), freq, code_bytes.len()));
                }
            });
        results.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        results
    }

    /// Per-code lookup exposing raw `freq` alongside [`Layer`] — used by
    /// the v1.4.7 composite hot path for the orthodox score
    /// decomposition (split log_prior_q4 = Q4·ln(1+freq) from
    /// log_likelihood_q4 = Q4·ln(layer.base()·pref·demotes)). The
    /// existing [`Self::lookup_with_layer_into`] returns the combined
    /// `layer.base()·pref + freq` score; for Q4 log-space additive
    /// sort key (PLAN.md L1 probability-native ranking) we need the
    /// two terms unfused.
    ///
    /// Rare-CJK filter NOT applied here (caller decides; consistent
    /// with `lookup_with_layer_into`).
    pub fn lookup_with_freq_layer_into(&self, code: &str, out: &mut Vec<(String, Layer, u64)>) {
        out.clear();
        let lower = code.to_ascii_lowercase();
        self.map.get_for_each(lower.as_bytes(), |word, value| {
            if let Ok(s) = core::str::from_utf8(word) {
                let (layer, freq) = unpack(value);
                out.push((s.to_string(), layer, freq));
            }
        });
    }

    /// Iterate every entry in the embedded dict, in FST traversal order
    /// (canonical lexicographic by `code` bytes; for a given code the
    /// internal layout sees `(code, word, packed_value)` triples). Used
    /// by tools / snapshot binaries (e.g. v1.4.3 `idf-from-wubi-tables`)
    /// that need to re-emit the full dict in another format. NOT a
    /// runtime hot-path API — allocates one `(String, String)` pair per
    /// entry (~135k for the embedded dict, ~5 MB allocation total).
    ///
    /// `layer` is the layered confidence band ([`Layer`]), `freq` is the
    /// per-entry frequency score (post-`pack` / pre-`unpack`).
    pub fn all_entries(&self) -> Vec<(String, String, Layer, u64)> {
        let mut results: Vec<(String, String, Layer, u64)> = Vec::new();
        self.map
            .prefix_for_each(b"", |code_bytes, word_bytes, value| {
                if let (Ok(code), Ok(word)) = (
                    core::str::from_utf8(code_bytes),
                    core::str::from_utf8(word_bytes),
                ) {
                    let (layer, freq) = unpack(value);
                    results.push((code.to_string(), word.to_string(), layer, freq));
                }
            });
        results
    }

    /// All `(code, word)` pairs with code starting with `prefix`, ordered by
    /// (effective L1 weight desc, code, word). Pins are NOT applied here —
    /// they're per-code and don't generalize across a prefix scan.
    pub fn prefix(&self, prefix: &str) -> Vec<(String, String)> {
        let lower = prefix.to_ascii_lowercase();

        let prefs = self
            .l0
            .read()
            .map(|g| g.layer_prefs)
            .unwrap_or(DEFAULT_LAYER_PREFS);

        let mut results: Vec<(String, String, f64)> = Vec::new();
        self.map
            .prefix_for_each(lower.as_bytes(), |code_bytes, word_bytes, value| {
                if let (Ok(code), Ok(word)) = (
                    core::str::from_utf8(code_bytes),
                    core::str::from_utf8(word_bytes),
                ) {
                    let (layer, freq) = unpack(value);
                    let score = layer.base() as f64 * prefs[layer.as_index()] + freq as f64;
                    results.push((code.to_string(), word.to_string(), score));
                }
            });
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

    /// Record that the user picked `word` for `code`, incrementing that
    /// pair's usage counter. Ranking is NOT affected — counters are
    /// statistics. Only [`Self::pin`] changes candidate order.
    ///
    /// Silently no-ops if `(code, word)` isn't in L1 (defends against the
    /// host accidentally feeding us things the user couldn't actually have
    /// selected from candidates).
    pub fn record_pick(&self, code: &str, word: &str) {
        if !self.exists_in_l1(code, word) {
            return;
        }
        let Ok(mut l0) = self.l0.write() else {
            return;
        };
        *l0.pick_counts
            .entry((code.to_string(), word.to_string()))
            .or_insert(0) += 1;
    }

    /// User-pinned word for `code`, if any. Used by the composite
    /// dispatch layer to re-apply the L0 pin promotion when wubi
    /// candidates are sourced from the `lookup_with_freq_layer` path
    /// (raw per-entry data, no pin baked in).
    pub fn pinned_word(&self, code: &str) -> Option<String> {
        let lower = code.to_ascii_lowercase();
        self.l0
            .read()
            .ok()
            .and_then(|g| g.pins.get(&lower).cloned())
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

    /// Auto-pin removal (user 2026-07-20 "整个自动置顶都关了吧"): picking
    /// the same candidate any number of times must NEVER reorder
    /// candidates. Previously the 3rd pick auto-pinned it, which silently
    /// rewrote the user's candidate order (fcu: 3 picks of 云 moved it
    /// above 去 and it stayed there).
    #[test]
    fn record_pick_never_pins_however_many_times() {
        let d = WubiDict::embedded();
        let before = d.lookup("khlg").first().cloned();
        for _ in 0..10 {
            d.record_pick("khlg", "跑车");
        }
        assert_eq!(
            d.lookup("khlg").first().cloned(),
            before,
            "record_pick must not reorder candidates"
        );
        assert_eq!(d.l0_pin_count(), 0, "record_pick must not create pins");
        // The counter itself still accrues — it is usage statistics.
        assert_eq!(d.l0_pending_count(), 1);
    }

    #[test]
    fn record_pick_rejects_unknown_word() {
        let d = WubiDict::embedded();
        for _ in 0..5 {
            d.record_pick("khlg", "this_is_not_a_real_word");
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
}
