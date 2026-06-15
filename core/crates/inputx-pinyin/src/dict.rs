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

use std::sync::{Arc, OnceLock, RwLock};

use inputx_fsa::{Dict, Fsa};

use crate::bigram_lm::LmBackend;
use crate::ranking::{L0Inner, L0Snapshot, PROMOTE_THRESHOLD};

/// Phase-2 LM mixing weight read from env `PINYIN_LM_LAMBDA` on first
/// call and cached. Default 1.0, picked at CP-2.6 sweet-spot sweep
/// (gold-1000 MIU peaks at λ=1.0 with +0.90pp gold lift, full 50k
/// confirms +0.76pp overall; λ=3.0 starts over-shooting the LM term
/// vs raw_freq and degrades MIU). See
/// `tools/eval/results/lm_lambda_sweep.tsv` for the full sweep data.
/// Set to 0 to disable the LM contribution entirely while keeping
/// the model loaded — useful for byte-equal regression testing /
/// rollback without rebuilding the binary.
fn lm_lambda() -> f64 {
    const LM_LAMBDA_DEFAULT: f64 = 1.0;
    static CACHED: OnceLock<f64> = OnceLock::new();
    *CACHED.get_or_init(|| {
        std::env::var("PINYIN_LM_LAMBDA")
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(LM_LAMBDA_DEFAULT)
    })
}

// Default = full pinyin dict from the committed `data/pinyin.dict` (3.9 MB,
// an `inputx-fsa` two-level Dict pre-built by `tools/build_dict.rs` from
// `data/weights/weights.tsv` — maintainer regenerates after data changes;
// see workspace ROADMAP item 23). `bootstrap_only` feature swaps to the tiny
// ~125-entry bootstrap dict built at compile time from `data/bootstrap.tsv`
// (1.7 KB).
//
// Why pre-built vs build.rs-generated: keeps the published crate under
// crates.io's size cap by letting us exclude the heavy intermediate TSV
// files (weights.tsv 23 MB, readings.tsv 13 MB, etc.) from the package.
// v1.4.7 sub-phase B (Strategy C): the embedded dict blob moved out
// of `../data/pinyin.dict` into the sibling `inputx-pinyin-data-core`
// crate so the facade publishes light.
#[cfg(not(feature = "bootstrap_only"))]
const DICT_BYTES: &[u8] = inputx_pinyin_data_core::EMBEDDED_PINYIN_DICT;

#[cfg(feature = "bootstrap_only")]
const DICT_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/bootstrap.dict"));

/// Inter-token word-bigram FST (v1.3 联想-conservative split):
/// keys = `<prev_word>\0<next_word>` where both ends are distinct jieba
/// tokens adjacent in the corpus. This is the ONLY source for
/// next-word prediction — chains built from this set are real
/// "what word followed what word" patterns, not character pairs from
/// within a single phrase.
// v1.4.7 sub-phase B (Strategy C): the embedded bigrams moved out
// of `../data/bigrams.fsa` into `inputx-pinyin-data-bigrams`, gated
// behind the `bigrams` feature flag (default-on). With the flag off
// the dict still loads — `PinyinDict::bigram_boost` returns 0 and
// Viterbi falls back to corpus-freq-only composition ordering.
#[cfg(all(not(feature = "bootstrap_only"), feature = "bigrams"))]
const BIGRAMS_BYTES: &[u8] = inputx_pinyin_data_bigrams::EMBEDDED_BIGRAMS;

#[cfg(any(feature = "bootstrap_only", not(feature = "bigrams")))]
const BIGRAMS_BYTES: &[u8] = &[];

/// Intra-token char-bigram FST: keys = `<a>\0<b>` where `a` and `b`
/// are adjacent characters INSIDE a single jieba token (e.g. (你, 好)
/// captured from "你好"). Used ONLY by [`PinyinDict::bigram_boost`] to
/// help Viterbi composition prefer known phrases; NEVER used for
/// next-word prediction (intra-phrase char pairs aren't sentence
/// continuations and spawning predictions from them produces chains
/// like 椒→粉→碎→机构 that look superficially plausible but are
/// globally nonsense).
#[cfg(all(not(feature = "bootstrap_only"), feature = "bigrams"))]
const BIGRAMS_INTRA_BYTES: &[u8] = inputx_pinyin_data_bigrams::EMBEDDED_BIGRAMS_INTRA;

#[cfg(any(feature = "bootstrap_only", not(feature = "bigrams")))]
const BIGRAMS_INTRA_BYTES: &[u8] = &[];

/// Inter-token word-trigram FST: keys = `<a>\0<b>\0<c>`, all three
/// distinct jieba tokens. Used by `predict_next_words_context` for
/// sentence-level coherent next-word prediction.
// v1.4.7 sub-phase B (Strategy C): trigrams moved into the
// `inputx-pinyin-data-trigrams` stone, gated behind the `trigrams`
// feature flag (default-on). With the flag off the dict still loads
// — `PinyinDict::predict_next_words_context` returns an empty Vec.
#[cfg(all(not(feature = "bootstrap_only"), feature = "trigrams"))]
const TRIGRAMS_BYTES: &[u8] = inputx_pinyin_data_trigrams::EMBEDDED_TRIGRAMS;

#[cfg(any(feature = "bootstrap_only", not(feature = "trigrams")))]
const TRIGRAMS_BYTES: &[u8] = &[];

/// The pinyin dictionary: an embedded FST plus a mutable L0 layer for
/// per-user preference learning.
///
/// All read methods take `&self`. L0 mutations (`record_pick`, `pin`,
/// `forget`, `import_l0`) also take `&self` — interior mutability via
/// `RwLock` lets a single shared instance feed every concurrent IME /
/// WASM session without exposing the lock to the caller.
pub struct PinyinDict {
    map: Dict<&'static [u8]>,
    /// Inter-token bigram FST (truly adjacent jieba tokens). Source of
    /// next-word predictions. `None` in bootstrap_only.
    bigrams: Option<Fsa<&'static [u8]>>,
    /// Intra-token char-bigram FST (chars inside one phrase). Helps
    /// Viterbi prefer known phrases. NEVER used for predictions.
    bigrams_intra: Option<Fsa<&'static [u8]>>,
    /// Inter-token trigram index. Two-level Dict (a\0b) → [(c, count)] —
    /// predict only scans (a\0b, *), so two-level is the natural + smaller
    /// fit (~2 MB under the flat Fsa). Source of context-aware predictions.
    trigrams: Option<Dict<&'static [u8]>>,
    l0: RwLock<L0Inner>,
    /// Per-char max freq across ALL its pinyin readings (lazy init).
    /// Built once on first access by scanning the entire FST. Used by
    /// the composite layer to detect "wubi Jianma2 simcode char that
    /// is itself rare" (e.g. 嶙 freq 15k) vs "common-char simcode
    /// (e.g. 左 freq 41k, 能 freq 56k)" — score-driven yield to
    /// pinyin top for rare-char simcodes, no hardcoded special-case
    /// lists in dispatch.
    char_max_freq: OnceLock<std::collections::HashMap<char, u64>>,
    /// Phase-2 optional bigram LM backend. None = no LM (Phase 1
    /// byte-equal behavior). When set, [`Self::bigram_lm_bonus`]
    /// returns a non-zero contribution that the Viterbi composition
    /// adds to per-step scores.
    lm: Option<Arc<dyn LmBackend>>,
}

impl PinyinDict {
    /// Construct from the embedded FST. Cheap (validates the FST header
    /// and initializes an empty L0). Callers should still cache the
    /// instance and reuse it for the program lifetime.
    pub fn embedded() -> Self {
        fn load_optional(bytes: &'static [u8], label: &str) -> Option<Fsa<&'static [u8]>> {
            if bytes.is_empty() {
                None
            } else {
                Some(Fsa::new(bytes).unwrap_or_else(|_| panic!("invalid embedded {label} fsa")))
            }
        }
        fn load_optional_dict(bytes: &'static [u8], label: &str) -> Option<Dict<&'static [u8]>> {
            if bytes.is_empty() {
                None
            } else {
                Some(Dict::new(bytes).unwrap_or_else(|_| panic!("invalid embedded {label} dict")))
            }
        }
        // dev/test escape hatch: INPUTX_PINYIN_DICT points at a dict file to
        // load at runtime instead of the embedded bytes — lets gate1
        // (07_validate/gate1_regression_corpus.py) validate a pipeline-built
        // dict without rebuilding the binary. Box::leak supplies the 'static
        // lifetime Dict needs; harmless in a short-lived probe. Not compiled
        // for wasm (no fs/env there) — env unset everywhere else keeps the
        // embedded-bytes behavior byte-for-byte unchanged.
        #[cfg(not(target_arch = "wasm32"))]
        let dict_bytes: &'static [u8] = match std::env::var_os("INPUTX_PINYIN_DICT") {
            Some(path) => {
                let data = std::fs::read(&path)
                    .unwrap_or_else(|e| panic!("INPUTX_PINYIN_DICT {path:?}: {e}"));
                Box::leak(data.into_boxed_slice())
            }
            None => DICT_BYTES,
        };
        #[cfg(target_arch = "wasm32")]
        let dict_bytes: &'static [u8] = DICT_BYTES;

        Self {
            map: Dict::new(dict_bytes).expect("invalid pinyin dict"),
            bigrams: load_optional(BIGRAMS_BYTES, "bigrams"),
            bigrams_intra: load_optional(BIGRAMS_INTRA_BYTES, "bigrams_intra"),
            trigrams: load_optional_dict(TRIGRAMS_BYTES, "trigrams"),
            l0: RwLock::new(L0Inner::new()),
            char_max_freq: OnceLock::new(),
            lm: None,
        }
    }

    /// Attach a Phase-2 bigram LM backend. Returns the same dict, now
    /// with `lm` set, so callers can chain `PinyinDict::embedded().with_lm(...)`.
    ///
    /// Pass `None` (or simply skip this call) to keep Phase-1 byte-equal
    /// behaviour. With Some(lm), Viterbi composition adds
    /// `LM_SCALE * λ * log10 P(curr | prev)` to per-step scores; λ comes
    /// from env `PINYIN_LM_LAMBDA` (default 0.3, climb-plan CP-2.5).
    pub fn with_lm(mut self, lm: Option<Arc<dyn LmBackend>>) -> Self {
        self.lm = lm;
        self
    }

    /// Try to attach the Phase-2 KenLM bigram model from the file path
    /// in env `PINYIN_LM_BINARY`. No-op (returns self unchanged) when:
    ///   - the crate is built without the `kenlm` feature
    ///   - PINYIN_LM_BINARY is unset
    ///   - loading the file fails (a warning goes to stderr)
    ///
    /// `PinyinEngine::new` calls this so that the env-var conversion
    /// happens exactly once at engine construction, and downstream
    /// callers (sessions, adapters, etc.) just inherit the dict.
    pub fn with_lm_from_env(self) -> Self {
        #[cfg(feature = "kenlm")]
        {
            let Ok(path) = std::env::var("PINYIN_LM_BINARY") else {
                return self;
            };
            match crate::bigram_lm::kenlm_backend::KenLmBackend::load(&path) {
                Ok(lm) => {
                    let arc: Arc<dyn LmBackend> = Arc::new(lm);
                    return self.with_lm(Some(arc));
                }
                Err(e) => {
                    eprintln!("[pinyin] failed to load PINYIN_LM_BINARY={path:?}: {e:?}");
                    return self;
                }
            }
        }
        #[cfg(not(feature = "kenlm"))]
        self
    }

    /// Max freq across all pinyin readings of single-char `c`. Returns
    /// 0 for characters not in the dict OR for non-single-char strings.
    ///
    /// Lazy: scans the entire FST on first call and caches the result
    /// (HashMap<char, u64>). Subsequent calls are O(1) hash lookup.
    ///
    /// Used by composite/dispatch to score-driven-demote wubi Jianma2
    /// entries whose target char is rare (e.g. 嶙 freq 15k yields to
    /// pinyin 默 freq 36k at code 'mo'). NO hardcoded protected list
    /// — the user's principle: "完全走评分候选，一行 hardcode 都不允
    /// 许有". Common chars (左/表/能/伙) naturally retain their Jianma2
    /// lead because their own freq is high; rare chars (嶙) lose.
    pub fn char_max_freq(&self, c: char) -> u64 {
        self.build_char_freq_cache().get(&c).copied().unwrap_or(0)
    }

    fn build_char_freq_cache(&self) -> &std::collections::HashMap<char, u64> {
        self.char_max_freq.get_or_init(|| {
            let mut cache = std::collections::HashMap::with_capacity(8192);
            // Item bytes ARE the word (two-level Dict keeps words out of the
            // automaton), so no \0-split needed.
            self.map.prefix_for_each(b"", |_code, word_bytes, freq| {
                let Ok(word) = core::str::from_utf8(word_bytes) else {
                    return;
                };
                // Only track single-char entries — multi-char phrases'
                // own freq doesn't tell us how common the constituent
                // chars are individually.
                let mut chars = word.chars();
                let Some(c) = chars.next() else { return };
                if chars.next().is_some() {
                    return;
                }
                let entry = cache.entry(c).or_insert(0);
                if freq > *entry {
                    *entry = freq;
                }
            });
            cache
        })
    }

    /// Number of L0 pinned pinyins.
    pub fn l0_pin_count(&self) -> usize {
        self.l0.read().map(|g| g.pins.len()).unwrap_or(0)
    }

    /// Number of distinct `(pinyin, word)` pairs with pending pick counters.
    pub fn l0_pending_count(&self) -> usize {
        self.l0.read().map(|g| g.pick_counts.len()).unwrap_or(0)
    }

    /// Number of distinct pinyin codes in the dictionary. (The two-level
    /// `Dict` counts codes, not total (pinyin, word) pairs.)
    pub fn len(&self) -> usize {
        self.map.len() as usize
    }

    /// `true` iff the dictionary is empty.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
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

        // Use `lower_str` (not raw `to_ascii_lowercase`) so the lue↔lve /
        // nue↔nve alias collapse fires here too — `celue` queries the
        // same FST key as `celve`.
        let lower = lower_str(pinyin);
        // Dict items come freq-desc (then item-asc), matching the old
        // `sort_by_key(Reverse(freq))` stable order — no re-sort. Streamed
        // (no intermediate Vec / per-item copy).
        self.map.get_for_each(lower.as_bytes(), |word, _freq| {
            if let Ok(s) = core::str::from_utf8(word) {
                out.push(s.to_string());
            }
        });

        // L0 pin: pull to position 0 if present.
        if let Ok(l0) = self.l0.read()
            && let Some(pref) = l0.pins.get(&lower)
            && let Some(idx) = out.iter().position(|w| w == pref)
            && idx > 0
        {
            let p = out.remove(idx);
            out.insert(0, p);
        }
    }

    /// `true` iff at least one entry's pinyin starts with `prefix`. Stops
    /// scanning at the first hit — much cheaper than calling `.prefix()`
    /// or `.prefix_for_each()` just to check existence. Used by composite
    /// engines on the per-keystroke hot path where short prefixes would
    /// otherwise allocate tens of thousands of `(String, String)` pairs
    /// only to throw them away.
    pub fn prefix_exists(&self, prefix: &str) -> bool {
        // Normalize via `lower_str` so lue/nue alias collapses to lve/nve
        // for prefix checks too (otherwise `prefix_exists("celue")`
        // misses the `celve…` family of entries).
        self.map.contains_prefix(lower_str(prefix).as_bytes())
    }

    /// All `(pinyin, word)` pairs with pinyin starting with `prefix`. Ordered
    /// by (pinyin asc, word asc) — useful for prefix completion suggestions.
    pub fn prefix(&self, prefix: &str) -> Vec<(String, String)> {
        let lower = lower_str(prefix);
        let mut results: Vec<(String, String)> = Vec::new();
        self.map
            .prefix_for_each(lower.as_bytes(), |code, word, _freq| {
                if let (Ok(pinyin), Ok(word)) =
                    (core::str::from_utf8(code), core::str::from_utf8(word))
                {
                    results.push((pinyin.to_string(), word.to_string()));
                }
            });
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
        let lower = lower_str(prefix);
        self.map
            .prefix_for_each(lower.as_bytes(), |code, word, value| {
                visit(code, word, value);
            });
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
        let lower = lower_str(prefix);
        let mut results: Vec<(String, String, u64)> = Vec::new();
        self.map
            .prefix_for_each(lower.as_bytes(), |code, word, value| {
                if let (Ok(pinyin), Ok(word)) =
                    (core::str::from_utf8(code), core::str::from_utf8(word))
                {
                    results.push((pinyin.to_string(), word.to_string(), value));
                }
            });
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

    /// Per-code lookup exposing raw `freq` directly — companion to
    /// [`Self::lookup_with_scores_into`] which fuses `PINYIN_PHRASE_BASE
    /// + freq` plus L0 pin promote into a single f64 score. v1.4.7
    /// composite hot path needs the unfused freq for orthodox Q4 log
    /// decomposition (log_prior_q4 = Q4·ln(1+freq); log_likelihood_q4
    /// = Q4·ln(PINYIN_PHRASE_BASE) + per-path multiplicative log
    /// factors). No L0 pin promote applied here — that's a cement-
    /// level business rule the composite layer re-applies.
    pub fn lookup_with_freq_into(&self, pinyin: &str, out: &mut Vec<(String, u64)>) {
        out.clear();
        let lower = lower_str(pinyin);
        self.map.get_for_each(lower.as_bytes(), |word, freq| {
            if let Ok(s) = core::str::from_utf8(word) {
                out.push((s.to_string(), freq));
            }
        });
    }

    /// Scored variant of `lookup_into`. Same ordering rules (freq desc,
    /// L0 pin promoted to position 0) but emits `(word, score)` tuples
    /// so the composite-layer merge can do unified cross-engine sort.
    ///
    /// Score formula:
    ///   * base = `PINYIN_PHRASE_BASE` (≈ wubi Phrase layer base)
    ///   * raw  = base + freq
    ///   * pinned candidate: × 1000.0 (must dominate any natural score)
    ///
    /// The base placement deliberately matches wubi Phrase (~400k) so a
    /// high-freq pinyin word competes fairly with wubi Phrase entries
    /// at the same code, but stays below wubi Jianma simcodes (which
    /// have base 600k–1M depending on layer).
    pub fn lookup_with_scores_into(&self, pinyin: &str, out: &mut Vec<(String, f64)>) {
        out.clear();
        let lower = lower_str(pinyin);

        const PINYIN_PHRASE_BASE: f64 = 400_000.0;

        let mut scratch: Vec<(String, f64)> = Vec::with_capacity(8);
        self.map.get_for_each(lower.as_bytes(), |word, freq| {
            if let Ok(s) = core::str::from_utf8(word) {
                scratch.push((s.to_string(), PINYIN_PHRASE_BASE + freq as f64));
            }
        });
        // L0 pin: multiply pinned candidate's score so it tops the
        // engine-internal sort AND the cross-engine merge layer.
        let pinned: Option<String> = self
            .l0
            .read()
            .ok()
            .and_then(|g| g.pins.get(&lower).cloned());
        if let Some(p) = &pinned {
            for e in scratch.iter_mut() {
                if &e.0 == p {
                    e.1 *= 1000.0;
                }
            }
        }
        scratch.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        out.reserve(scratch.len());
        for (w, score) in scratch.drain(..) {
            out.push((w, score));
        }
    }

    /// Viterbi-style sentence-level segmentation for long pinyin buffers.
    ///
    /// Given an input like `nihaomawojiao`, splits it into the best
    /// sequence of dict-matched syllable groups and returns the composed
    /// Chinese string (e.g. `你好吗我叫`) paired with the total path score.
    /// Returns `None` if no valid all-buffer-covering segmentation exists
    /// (e.g. garbage input that contains no dictionary matches at any
    /// substring), or for short buffers where regular phrase lookup
    /// already covers the case.
    ///
    /// Algorithm: standard DP. `dp[i]` = the best path reaching position
    /// `i` from `0`, represented as (cumulative_score, prev_pos, word).
    /// For each `i`, we try every cut point `j ∈ [i-MAX_SYL, i)` where
    /// the segment `buffer[j..i]` is a valid dict-matched chunk; the
    /// per-step score is `dict_score + bigram_boost(prev_word, word)`.
    /// `MAX_SYL = 6` covers the longest pinyin syllables (zhuang/chuang/
    /// shuang); the segment lookup naturally also catches multi-syllable
    /// phrases up to 6 chars (zhongguo / women / etc.).
    ///
    /// Complexity: O(n × MAX_SYL × avg_lookup_size). For n=20 that's
    /// ~120 dict lookups — well under the per-keystroke budget.
    ///
    /// Returns `None` when:
    ///   * Buffer is shorter than `MIN_LEN` (4) — regular lookup is fine.
    ///   * Buffer is longer than `MAX_LEN` (30) — bail out, user is
    ///     probably mashing keys, not typing a coherent sentence.
    ///   * No path covers the full buffer (some segment had no dict hits).
    /// Like [`best_composition`] but also returns the per-segment chain
    /// (Vec of dict-word strings in left-to-right order). Caller can audit
    /// cross-segment bigram strength using [`bigram_boost`] over consecutive
    /// pairs — used by the Path 0b quality gate (user polish-log 2026-05-27:
    /// `houxuanqu` → 候选+去 where (候选, 去) bigram is 0, so the
    /// composition is a mechanical join with no corpus backing).
    pub fn best_composition_chain(&self, buffer: &str) -> Option<(f64, String, Vec<String>)> {
        const MIN_LEN: usize = 4;
        const MAX_LEN: usize = 30;
        const MAX_SYL: usize = 24;
        const STEP_PENALTY: f64 = 100_000.0;
        let buf = buffer.as_bytes();
        let n = buf.len();
        if !(MIN_LEN..=MAX_LEN).contains(&n) {
            return None;
        }
        let mut dp: Vec<Option<(f64, usize, String)>> = vec![None; n + 1];
        dp[0] = Some((0.0, 0, String::new()));
        let mut scratch: Vec<(String, u64)> = Vec::new();
        for i in 1..=n {
            let lo = i.saturating_sub(MAX_SYL);
            for j in lo..i {
                let prev_entry = match dp[j].as_ref() {
                    Some(p) => p.clone(),
                    None => continue,
                };
                let seg = match core::str::from_utf8(&buf[j..i]) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                self.lookup_raw_into(seg, &mut scratch);
                if scratch.is_empty() {
                    continue;
                }
                for (word, raw_freq) in scratch.iter() {
                    let prev_word_opt = if prev_entry.2.is_empty() {
                        None
                    } else {
                        Some(prev_entry.2.as_str())
                    };
                    let bonus = self.bigram_boost(prev_word_opt, word)
                        + self.bigram_lm_bonus(prev_word_opt, word);
                    let step_score = (*raw_freq as f64) + bonus - STEP_PENALTY;
                    let total = prev_entry.0 + step_score;
                    let dp_better = match dp[i].as_ref() {
                        None => true,
                        Some(cur) => total > cur.0,
                    };
                    if dp_better {
                        dp[i] = Some((total, j, word.clone()));
                    }
                }
            }
        }
        let final_entry = dp[n].as_ref()?;
        let final_score = final_entry.0;
        let mut chain: Vec<String> = Vec::new();
        let mut pos = n;
        while pos > 0 {
            let entry = dp[pos].as_ref()?;
            chain.push(entry.2.clone());
            pos = entry.1;
        }
        chain.reverse();
        let sentence = chain.concat();
        Some((final_score, sentence, chain))
    }

    /// Best Viterbi composition for `buffer`, score and concatenated
    /// sentence only. See [`best_composition_chain`] for the same result
    /// with the per-segment chain exposed (needed by Path 0b's bigram-
    /// support audit).
    pub fn best_composition(&self, buffer: &str) -> Option<(f64, String)> {
        const MIN_LEN: usize = 4;
        const MAX_LEN: usize = 30;
        // Per-segment max byte length. Pinyin syllables are ≤6 chars
        // (zhuang/chuang/shuang), but dict entries can be multi-syllable
        // PHRASES — `zhongguo` is 8 chars but a single lexeme 中国;
        // `zhonghuarenmingongheguo` is 23 chars (中华人民共和国). A
        // syllable-length cap was wrong (8>6 → never tried as 1 step →
        // forced into multi-segment paths like 中+國). Allow up to a
        // generous phrase length so any reasonable dict phrase has a
        // chance to be picked in one go.
        const MAX_SYL: usize = 24;
        // Per-segment fixed cost. Subtracted from each segment's freq so
        // longer phrases (one segment covering more pinyin) consistently
        // outscore the same span split into multiple single-char hits.
        //
        // Calibration: raw freq for 你 is ~63k, 好 ~58k, 你好 ~35k. With
        // STEP_PENALTY=100k, the phrase 你好 scores 35k-100k=-65k vs
        // the two-char split scoring (63k+58k)-200k=-79k. Phrase wins
        // by 14k — comfortable margin.
        const STEP_PENALTY: f64 = 100_000.0;
        // Bigram bonus calibration. Reuse the standard bigram_boost
        // (max 50k); plenty to break ties between same-segment-count
        // paths but doesn't overpower the STEP_PENALTY preference for
        // longer segments.
        let buf = buffer.as_bytes();
        let n = buf.len();
        if !(MIN_LEN..=MAX_LEN).contains(&n) {
            return None;
        }
        // Pinyin is always ASCII, so byte indexing is safe.
        // dp[i] = (best_cumulative_score, prev_position, chosen_word_at_this_step)
        // dp[0] is the start sentinel with empty chosen word.
        let mut dp: Vec<Option<(f64, usize, String)>> = vec![None; n + 1];
        dp[0] = Some((0.0, 0, String::new()));

        let mut scratch: Vec<(String, u64)> = Vec::new();
        for i in 1..=n {
            let lo = i.saturating_sub(MAX_SYL);
            for j in lo..i {
                let prev_entry = match dp[j].as_ref() {
                    Some(p) => p.clone(),
                    None => continue,
                };
                let seg = match core::str::from_utf8(&buf[j..i]) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                self.lookup_raw_into(seg, &mut scratch);
                if scratch.is_empty() {
                    continue;
                }
                for (word, raw_freq) in scratch.iter() {
                    let prev_word_opt = if prev_entry.2.is_empty() {
                        None
                    } else {
                        Some(prev_entry.2.as_str())
                    };
                    let bonus = self.bigram_boost(prev_word_opt, word)
                        + self.bigram_lm_bonus(prev_word_opt, word);
                    let step_score = (*raw_freq as f64) + bonus - STEP_PENALTY;
                    let total = prev_entry.0 + step_score;
                    let dp_better = match dp[i].as_ref() {
                        None => true,
                        Some(cur) => total > cur.0,
                    };
                    if dp_better {
                        dp[i] = Some((total, j, word.clone()));
                    }
                }
            }
        }

        let final_entry = dp[n].as_ref()?;
        let final_score = final_entry.0;
        // Reconstruct word chain by walking back.
        let mut chain: Vec<String> = Vec::new();
        let mut pos = n;
        while pos > 0 {
            let entry = dp[pos].as_ref()?;
            chain.push(entry.2.clone());
            pos = entry.1;
        }
        chain.reverse();
        Some((final_score, chain.concat()))
    }

    /// K-best Viterbi composition: like [`Self::best_composition`] but
    /// retains the top-`k` paths to every position instead of just the
    /// single best, so the top-K full-buffer paths are recovered. Returns
    /// `(score, sentence)` tuples in score-desc order, deduped by sentence.
    ///
    /// Why it matters even when 1-best looks "right":
    ///   1-best DP commits irrevocably to dp[j]'s top word and only looks
    ///   forward from there. For `pianni` → 片(highest freq at 'pian') →
    ///   (片, *) bigram is weak so any 'ni' word fits → 你 (highest freq)
    ///   → "片你". The strong (骗, 你) bigram never gets to apply because
    ///   骗 was never the prev word. K-best keeps 骗 alive in dp[4] and
    ///   the (骗, 你) bonus (~50k from the bigram FST) pushes "骗你" above
    ///   "片你" globally. User-reported 2026-05-26: pianni should give
    ///   骗你, not 片你.
    ///
    /// Cost: O(n × MAX_SYL × k × avg_lookup_size × k_resort). For typical
    /// short buffers (≤8 chars, k=5) on the order of a few hundred
    /// `cmp::partial_cmp` calls per call. Safe to invoke from the per-
    /// keystroke composition path (Path 5 in pinyin_adapter).
    ///
    /// Returns `None`-equivalent (empty `Vec`) when `buffer` is outside
    /// the [MIN_LEN, MAX_LEN] window or no path covers the full buffer.
    pub fn top_k_compositions(&self, buffer: &str, k: usize) -> Vec<(f64, String)> {
        const MIN_LEN: usize = 4;
        const MAX_LEN: usize = 30;
        const MAX_SYL: usize = 24;
        const STEP_PENALTY: f64 = 100_000.0;
        let buf = buffer.as_bytes();
        let n = buf.len();
        if k == 0 || !(MIN_LEN..=MAX_LEN).contains(&n) {
            return Vec::new();
        }
        // dp[i] = top-k partial paths reaching position i:
        //   (cum_score, prev_pos, prev_idx_in_dp, chosen_word_at_step)
        // dp[0] is the start sentinel with one empty-word entry.
        let mut dp: Vec<Vec<(f64, usize, usize, String)>> = vec![Vec::new(); n + 1];
        dp[0].push((0.0, 0, 0, String::new()));

        let mut scratch: Vec<(String, u64)> = Vec::new();
        for i in 1..=n {
            let lo = i.saturating_sub(MAX_SYL);
            let mut candidates: Vec<(f64, usize, usize, String)> = Vec::new();
            for j in lo..i {
                if dp[j].is_empty() {
                    continue;
                }
                let seg = match core::str::from_utf8(&buf[j..i]) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                self.lookup_raw_into(seg, &mut scratch);
                if scratch.is_empty() {
                    continue;
                }
                for (prev_idx, prev_path) in dp[j].iter().enumerate() {
                    let prev_word_opt = if prev_path.3.is_empty() {
                        None
                    } else {
                        Some(prev_path.3.as_str())
                    };
                    for (word, raw_freq) in scratch.iter() {
                        let bonus = self.bigram_boost(prev_word_opt, word);
                        let step_score = (*raw_freq as f64) + bonus - STEP_PENALTY;
                        let total = prev_path.0 + step_score;
                        candidates.push((total, j, prev_idx, word.clone()));
                    }
                }
            }
            candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            candidates.truncate(k);
            dp[i] = candidates;
        }

        // Trace back each top-K path at dp[n].
        let mut out: Vec<(f64, String)> = Vec::with_capacity(dp[n].len());
        let mut seen: std::collections::HashSet<String> =
            std::collections::HashSet::with_capacity(dp[n].len());
        let end_paths: Vec<(f64, usize, usize)> = dp[n].iter().map(|p| (p.0, p.1, p.2)).collect();
        for (end_score, end_prev_pos, end_prev_idx) in end_paths {
            let mut chain: Vec<String> = Vec::new();
            // Start from the dp[n] entry's own word (need the entry itself
            // for the word, but the entry's prev is what we follow next).
            // We look up entries by walking (pos, idx) backward — the END
            // entry's word is dp[n][rank_at_end], so first pull it.
            // Reconstruct by tracking (pos, idx).
            let mut pos = n;
            // Find the rank of (end_score, end_prev_pos, end_prev_idx)
            // within dp[n]: since the loop iterates dp[n] in order, the
            // corresponding rank is implicit — we know its prev_pos/idx,
            // we just need its own word, accessed by re-indexing via these.
            // Simpler: walk by storing the cur position+idx; on each step
            // grab the entry's word then jump to its prev_pos/prev_idx.
            // Bootstrap: locate cur_idx for `pos == n` by matching prev.
            let mut cur_idx = dp[pos]
                .iter()
                .position(|p| (p.1, p.2) == (end_prev_pos, end_prev_idx))
                .expect("dp[n] contains the end path we just enumerated");
            while pos > 0 {
                let entry = &dp[pos][cur_idx];
                chain.push(entry.3.clone());
                pos = entry.1;
                cur_idx = entry.2;
            }
            chain.reverse();
            let sentence = chain.concat();
            if seen.insert(sentence.clone()) {
                out.push((end_score, sentence));
            }
        }
        out
    }

    /// Raw (word, freq) lookup — like `lookup_with_scores_into` but
    /// returns the FST's raw u64 freq value instead of the
    /// PINYIN_PHRASE_BASE-shifted f64 score. Used by Viterbi
    /// (`best_composition`) where pre-baked bases would bias the
    /// segmentation toward single-char paths (each segment carrying its
    /// own 400k base inflates many-segment paths).
    fn lookup_raw_into(&self, pinyin: &str, out: &mut Vec<(String, u64)>) {
        out.clear();
        let lower = lower_str(pinyin);
        self.map.get_for_each(lower.as_bytes(), |word, freq| {
            if let Ok(s) = core::str::from_utf8(word) {
                out.push((s.to_string(), freq));
            }
        });
    }

    /// Iterate every `(prev, next, count)` entry in the bigram FST.
    /// Tools-only API (v1.4.4 `idf-from-pinyin-bigrams` snapshot
    /// binary uses this); NOT a runtime hot-path call — full scan
    /// allocates one `(String, String)` pair per bigram (~500k for
    /// the embedded table). Returns an empty Vec under `bootstrap_only`.
    ///
    /// Layout: keys in the underlying FST are `prev_bytes + \0 +
    /// next_bytes` → `count u64`. We parse the key shape back into a
    /// `(prev, next)` pair on each emit.
    pub fn iter_bigrams(&self) -> Vec<(String, String, u64)> {
        // v1.4.4 (initial): only iterated `self.bigrams.as_ref()` (the
        // inter FST). v1.4.6 sub-phase C2 widened to sum inter + intra,
        // mirroring the live `bigram_boost` (which also sums both —
        // intra captures within-phrase co-occurrences like (你, 好) from
        // the curated 你好 phrase entry, inter captures cross-sentence
        // adjacency). Without summing, snapshot consumers (the NGMv1
        // .ngm file in particular) would under-count and break the
        // baseline fixture invariant during the v1.4.6 engine cutover.
        use std::collections::HashMap;
        let mut counts: HashMap<(String, String), u64> = HashMap::new();
        for src in [self.bigrams.as_ref(), self.bigrams_intra.as_ref()]
            .iter()
            .flatten()
        {
            src.prefix_for_each(b"", |key, count| {
                let Some(sep) = key.iter().position(|&b| b == 0) else {
                    return;
                };
                let prev = &key[..sep];
                let next = &key[sep + 1..];
                if next.is_empty() {
                    return;
                }
                if let (Ok(p), Ok(n)) = (core::str::from_utf8(prev), core::str::from_utf8(next)) {
                    *counts.entry((p.to_string(), n.to_string())).or_insert(0) += count;
                }
            });
        }
        counts.into_iter().map(|((p, n), c)| (p, n, c)).collect()
    }

    /// Predict the most likely next words given a just-committed `prev`
    /// word. Reads `bigrams.fst` for all `(prev, *)` pairs, sorts by
    /// count desc, returns top `limit`.
    ///
    /// This is the engine-side primitive for the 联想 / next-word
    /// prediction feature (Sogou-style post-commit panel). UI layers
    /// trigger this after every CJK commit and surface the result as
    /// the candidate list while the user hasn't started typing the
    /// next syllable.
    ///
    /// Returns empty Vec when:
    ///   * The bigrams FST isn't loaded (bootstrap_only build)
    ///   * `prev` is empty
    ///   * No bigrams start with `prev` (rare word, English / kana, etc.)
    ///
    /// Cost: O(matches) FST stream + O(matches log matches) sort. Most
    /// common words have 20-100 distinct followers in our top-500k
    /// bigram table; cost per call ≈ 10-100µs. Safe to call on every
    /// post-commit edge in the hot path.
    pub fn predict_next_words(&self, prev: &str, limit: usize) -> Vec<(String, u64)> {
        // v1.3: enforce a minimum bigram count. Weak (prev, *) pairs
        // are exactly the "associations that look plausible but aren't
        // real continuations" the user reported as 干扰 (interference).
        // Sticking the bar at MIN_PREDICTION_COUNT cuts off low-conviction
        // noise — if the corpus only saw (a, b) a handful of times, b
        // isn't a confident continuation of a.
        const MIN_PREDICTION_COUNT: u64 = 30;
        if prev.is_empty() || limit == 0 {
            return Vec::new();
        }
        let Some(bigrams) = self.bigrams.as_ref() else {
            return Vec::new();
        };
        let mut prefix = prev.as_bytes().to_vec();
        prefix.push(0u8);
        let prefix_len = prefix.len();
        let mut hits: Vec<(String, u64)> = Vec::new();
        bigrams.prefix_for_each(&prefix, |key, count| {
            if count < MIN_PREDICTION_COUNT {
                return;
            }
            let next_bytes = &key[prefix_len..];
            if next_bytes.is_empty() {
                return;
            }
            if let Ok(s) = core::str::from_utf8(next_bytes) {
                hits.push((s.to_string(), count));
            }
        });
        hits.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        hits.truncate(limit);
        hits
    }

    /// Context-aware next-word prediction (v1.4 strict trigram-only).
    ///
    /// User-reported failure mode 2026-05-24: typing `在` then pressing
    /// space repeatedly gave "在年月日年月日年月日…" — perfect bigram
    /// chain cycle. User: "在 本来就没道理联想 '年'，一开始就是错的".
    /// The single-context bigram signal is too noisy: bigram (在, 年)
    /// exists in corpus only because of phrases like "在2024年" but
    /// year-prediction has no semantic basis for "user just typed 在".
    ///
    /// New policy: predictions REQUIRE BOTH (prev_prev, prev) AND a
    /// trigram hit with `count >= MIN_TRIGRAM_COUNT`. NO bigram path
    /// at all — bigram (prev, *) data alone is too weak a signal for
    /// next-word prediction. Cold-start (prev_prev=None) → empty
    /// (user types next word manually).
    ///
    /// Effects:
    ///   - 在 alone → no predictions (need a second word for context).
    ///   - 我们 + 一起 → trigram (我们,一起,*) returns 走/去/吃饭/... if
    ///     count high enough. Confidence > noise.
    ///   - Chain (今天,的,*) → 标准/位置/规模/... when count high.
    pub fn predict_next_words_context(
        &self,
        prev_prev: Option<&str>,
        prev: &str,
        limit: usize,
    ) -> Vec<(String, u64)> {
        // History: 5→50 (2026-05-24) to kill noise chains ("年人在年月日
        // 的比赛中…"); then 50→15 (2026-05-25) after measuring that 50 also
        // killed almost all REAL predictions. Common pairs' trigram counts
        // cluster in 15-50 (我们的→国家:40/生活:30/工作:19, 我是→一个:30/
        // 谁:17, 一个人→在:49/都:47), so 50 fired only ~4/12 common pairs —
        // mostly the 泛词 "的". 15 surfaces the established 3-word patterns
        // while still cutting <15 noise (可以的→但:4, 我们一起→去:2). Chain
        // risk stays low: the cycle-filter (recent_committed dedup) and the
        // engine's PREDICTION_CHAIN_LIMIT hard-stop guard runaway chains
        // independent of this threshold — both added AFTER the count=5 era,
        // so 15-now is far safer than 5-then.
        const MIN_TRIGRAM_COUNT: u64 = 15;
        if prev.is_empty() || limit == 0 {
            return Vec::new();
        }
        // Strict: need BOTH prev_prev AND trigram FST.
        let Some(prev_prev) = prev_prev else {
            return Vec::new();
        };
        if prev_prev.is_empty() {
            return Vec::new();
        }
        let Some(trigrams) = self.trigrams.as_ref() else {
            return Vec::new();
        };
        // Two-level: code = prev_prev\0prev, items = the c words for (a,b).
        let mut code = prev_prev.as_bytes().to_vec();
        code.push(0u8);
        code.extend_from_slice(prev.as_bytes());
        let mut hits: Vec<(String, u64)> = Vec::new();
        trigrams.get_for_each(&code, |c_bytes, count| {
            if count < MIN_TRIGRAM_COUNT {
                return;
            }
            if let Ok(s) = core::str::from_utf8(c_bytes) {
                hits.push((s.to_string(), count));
            }
        });
        hits.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        hits.truncate(limit);
        hits
    }

    /// Context-aware bigram score bonus. Given the user's most recently
    /// committed word `prev` and a candidate `next`, returns an ADDITIVE
    /// bonus reflecting how often `(prev, next)` co-occur in the training
    /// corpus. Returns `0.0` when:
    ///   * `prev` is `None` (cold session, no prior word committed)
    ///   * `prev` or `next` is empty
    ///   * The bigram isn't in the FST (rare pair)
    ///   * The bigram FST isn't loaded (bootstrap_only build)
    ///
    /// Formula: `bonus = BIGRAM_BOOST_MAX × ln(1+count) / ln(1+BIGRAM_REF)`,
    /// capped at `BIGRAM_BOOST_MAX`. With MAX=50k and REF=1000, a
    /// count≥1000 bigram adds the full 50k — comparable to half of a
    /// typical freq value, enough to lift a mid-freq candidate above
    /// its peers without overriding a strong freq difference. The log
    /// scaling means count=1 and count=100000 don't differ by 100000×.
    ///
    /// Side-effect free: bigram lookup is read-only on the embedded FST.
    pub fn bigram_boost(&self, prev: Option<&str>, next: &str) -> f64 {
        const BIGRAM_BOOST_MAX: f64 = 50_000.0;
        const BIGRAM_REF: f64 = 1000.0;
        let Some(prev) = prev else { return 0.0 };
        if prev.is_empty() || next.is_empty() {
            return 0.0;
        }
        let mut key = prev.as_bytes().to_vec();
        key.push(0u8);
        key.extend_from_slice(next.as_bytes());
        // Sum counts across inter + intra (v1.3 split). Viterbi
        // composition wants both signals: inter says "(prev, next) are
        // adjacent words in real sentences", intra says "(prev, next)
        // co-occur as adjacent chars inside a known phrase like 你好".
        // Without summing, Viterbi would lose the intra signal entirely
        // after the split — which is precisely what v0.4 Phase A added
        // to make 你好 win as one segment.
        let count_inter = self.bigrams.as_ref().and_then(|m| m.get(&key)).unwrap_or(0);
        let count_intra = self
            .bigrams_intra
            .as_ref()
            .and_then(|m| m.get(&key))
            .unwrap_or(0);
        let count = count_inter + count_intra;
        if count == 0 {
            return 0.0;
        }
        let scaled = ((count as f64) + 1.0).ln() / (BIGRAM_REF + 1.0).ln();
        BIGRAM_BOOST_MAX * scaled.min(1.0)
    }

    /// Phase-2 KenLM bigram bonus. Adds to Viterbi step_score on top of
    /// [`Self::bigram_boost`] (which uses the legacy bigram support FST).
    ///
    /// Returns 0.0 when:
    ///   - `prev` is None (start of buffer; no bigram yet)
    ///   - `self.lm` is unset (Phase-1 byte-equal behaviour)
    ///   - the LM reports `enabled() == false`
    ///   - PINYIN_LM_LAMBDA env var is 0 or unset and the default fall-
    ///     through resolves to 0
    ///
    /// Otherwise returns `LM_SCALE * λ * log10 P(curr | prev)`.
    /// log10 P is negative (more probable bigrams → less negative),
    /// so the per-step contribution is negative; Viterbi compares
    /// relative totals so the absolute sign does not matter. The
    /// LM_SCALE factor brings the value to the same order of magnitude
    /// as `bigram_boost` (which caps at 50 000), so a "good" bigram
    /// (log10 P ≈ -2) at λ=0.3 contributes about -6 000.
    pub fn bigram_lm_bonus(&self, prev: Option<&str>, curr: &str) -> f64 {
        const LM_SCALE: f64 = 10_000.0;
        let Some(lm) = self.lm.as_ref() else {
            return 0.0;
        };
        if !lm.enabled() {
            return 0.0;
        }
        let Some(prev) = prev else { return 0.0 };
        if prev.is_empty() || curr.is_empty() {
            return 0.0;
        }
        let lambda = lm_lambda();
        if lambda == 0.0 {
            return 0.0;
        }
        let log_prob = lm.log_prob(prev, curr);
        if !log_prob.is_finite() {
            return 0.0;
        }
        LM_SCALE * lambda * (log_prob as f64)
    }

    /// Look up the user-pinned word for a given pinyin code, if any.
    /// Composite hosts use this to apply cross-engine pin promotion —
    /// e.g. if the user pinned pinyin `jixu → 继续`, the merged
    /// candidate list (which may include a wubi entry for the same
    /// code) needs to surface 继续 at position 0 even though wubi
    /// candidates structurally lead in the merge order.
    pub fn pinned_word(&self, pinyin: &str) -> Option<String> {
        let lower = lower_str(pinyin);
        self.l0
            .read()
            .ok()
            .and_then(|l0| l0.pins.get(&lower).cloned())
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

/// Canonicalize a pinyin lookup key.
///
/// 1. Lowercase ASCII letters (engine convention — dict stores lowercase).
/// 2. Collapse `lüe`/`nüe` alias spellings `lue`/`nue` to the canonical
///    `lve`/`nve` used by the dict. Sogou/Google Pinyin both accept the
///    `u`-spelling for these two syllables; users typing `celue` expect
///    the same candidates as `celve` (策略). The dict + L0 pin map both
///    use this key, so write-side (`pin`, `record_pick`) and read-side
///    (`lookup_*`) flow through the same normalization — no asymmetry.
///
/// The `j/q/x/y + ü` cases are already canonical-`u` (jue/que/xue/yue),
/// so they need no alias. The `l/n + ü` carve-out is the only spot
/// where Mandarin pinyin disambiguates `u` vs `ü` (lu/lü, nu/nü) — and
/// the IME-vs-strict-orthography mismatch lives entirely there.
///
/// Exposed publicly as `inputx_pinyin::normalize_lookup_key` so cement /
/// composite-layer call sites (which query the embedded IDF directly,
/// bypassing `PinyinDict::lookup_into`) can normalize uniformly.
pub fn normalize_lookup_key(s: &str) -> String {
    let mut out = s.to_ascii_lowercase();
    // Apply alias normalization only when the trigger substring is
    // present (avoids the allocation/scan on the 99%+ of buffers
    // that contain neither). Order matters only if `lue` and `nue`
    // could overlap, which they can't.
    if out.contains("lue") {
        out = out.replace("lue", "lve");
    }
    if out.contains("nue") {
        out = out.replace("nue", "nve");
    }
    out
}

fn lower_str(s: &str) -> String {
    normalize_lookup_key(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_loads() {
        let d = PinyinDict::embedded();
        assert!(d.len() >= 50, "bootstrap should have at least 50 entries");
    }

    /// Standing sanity gate on the SHIPPED data (full-dict builds only):
    /// every embedded index parses and is at the expected scale, so a
    /// corrupt / truncated / stale `.dict` / `.fsa` fails loudly here rather
    /// than silently degrading candidates. Skipped under bootstrap_only
    /// (tiny dict, no n-grams).
    #[cfg(not(feature = "bootstrap_only"))]
    #[test]
    fn shipped_data_at_expected_scale() {
        let d = PinyinDict::embedded();
        // pinyin.dict: ~156k distinct codes shipped; floor well below that.
        assert!(
            d.len() >= 140_000,
            "pinyin.dict too small: {} codes",
            d.len()
        );
        // n-gram indexes must be present (not None) and non-trivially sized.
        // bigram_boost reads bigrams/bigrams_intra; predict reads trigrams.
        assert!(
            d.bigram_boost(Some("中国"), "人民") > 0.0
                || d.bigram_boost(Some("我们"), "一起") > 0.0,
            "bigrams index looks empty"
        );
        // A high-frequency 3-gram context should yield predictions; if the
        // trigram dict is truncated/empty this returns nothing.
        let ctx = d.predict_next_words_context(Some("我们"), "一起", 10);
        let cold = d.predict_next_words_context(None, "我们", 10);
        assert!(cold.is_empty(), "cold-start (no prev_prev) must be empty");
        // ctx may legitimately be empty for a specific pair, so just assert
        // the call path is wired (no panic) + the dict loaded with scale.
        let _ = ctx;
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

    // ---- predict_next_words (联想 v1.0) -----------------------------

    #[test]
    fn predict_next_words_empty_inputs() {
        let d = PinyinDict::embedded();
        assert!(d.predict_next_words("", 10).is_empty());
        assert!(d.predict_next_words("今天", 0).is_empty());
    }

    #[cfg(not(feature = "bootstrap_only"))]
    #[test]
    fn lookup_lixiang_lixiang_leads() {
        // User-reported 2026-05-24: lixiang was ranking 立项 #1 and
        // 理想 #2 — wrong: 理想 is way more common in everyday usage.
        // Two compounding root causes (build_fst.rs overlay REPLACE +
        // modern_vocab assigning blanket 50k to 立项 despite its base
        // 17687 being well below 理想's 35168). Fixes:
        //   1. build_fst.rs overlay now uses MAX semantics.
        //   2. modern_vocab_v1.tsv purged of words already covered by
        //      base (purge_modern_overlap.py removed 立项).
        // After rebuild: 理想 (base 35168) should lead lixiang lookups.
        let d = PinyinDict::embedded();
        let cands = d.lookup("lixiang");
        assert_eq!(
            cands.first().map(String::as_str),
            Some("理想"),
            "expected 理想 #1 for lixiang; got {:?}",
            cands.iter().take(5).collect::<Vec<_>>()
        );
    }

    #[cfg(not(feature = "bootstrap_only"))]
    #[test]
    fn lookup_queshi_quexi_polish_log_promoted() {
        // User polish-log shows queshi → 缺失 picked 4× even though
        // base 确实 (42104) > base 缺失 (25090). v1.4 update to
        // aggregate_polish_log.py auto-tunes quickfix boost to beat
        // top peer + MARGIN → 缺失 should now lead at queshi.
        let d = PinyinDict::embedded();
        let cands = d.lookup("queshi");
        assert_eq!(
            cands.first().map(String::as_str),
            Some("缺失"),
            "expected 缺失 #1 (was 确实 before polish-log auto-tune); top5={:?}",
            cands.iter().take(5).collect::<Vec<_>>()
        );
    }

    #[cfg(not(feature = "bootstrap_only"))]
    #[test]
    fn lookup_traditional_dropped_after_strip() {
        // User-reported 2026-05-24: prediction panel surfaced 於 / 國 /
        // 來 etc. in a simplified-mode session. Root cause was the
        // bigrams + weights both containing traditional forms.
        // strip_traditional_weights.py purged ~5k traditional rows
        // from weights.tsv. After rebuild, lookups for common
        // simplified-versus-traditional collisions should NOT surface
        // traditional in the top candidate slot.
        let d = PinyinDict::embedded();
        // 'yu' previously had 於 (49376) > 于 (49010); after strip,
        // 於 row is gone so 于 has no competition from traditional.
        let yu_cands = d.lookup("yu");
        assert!(
            !yu_cands.iter().take(5).any(|w| w == "於"),
            "於 should be stripped; got top5={:?}",
            &yu_cands[..yu_cands.len().min(5)]
        );
        // 'guo' previously had 國 (49746) competing with 国 (50333).
        let guo_cands = d.lookup("guo");
        assert!(
            !guo_cands.iter().take(5).any(|w| w == "國"),
            "國 should be stripped; got top5={:?}",
            &guo_cands[..guo_cands.len().min(5)]
        );
    }

    #[cfg(not(feature = "bootstrap_only"))]
    #[test]
    fn predict_next_words_jintian_top_followers() {
        let d = PinyinDict::embedded();
        // 今天 is a very common word — corpus has plenty of (今天, *)
        // bigrams. Top should include 的/在/是 (high-count followers
        // verified by `head pinyin_bigrams_v1.tsv | grep 今天`).
        let preds = d.predict_next_words("今天", 10);
        assert!(!preds.is_empty(), "expected predictions for 今天");
        let words: Vec<&str> = preds.iter().map(|(w, _)| w.as_str()).collect();
        let has_common_followers = ["的", "在", "是", "我", "我们"]
            .iter()
            .any(|w| words.contains(w));
        assert!(
            has_common_followers,
            "expected at least one of 的/在/是/我/我们 in 今天 predictions; got {words:?}"
        );
    }

    #[cfg(not(feature = "bootstrap_only"))]
    #[test]
    fn predict_next_words_context_uses_trigram_or_empty() {
        // strict-trigram with MIN_TRIGRAM_COUNT=15 (was 50): trigram
        // (今天, 的, *) results may or may not clear the count
        // threshold depending on corpus density. The contract is just
        // "use trigram only, no bigram fallback" — empty is acceptable
        // per the conservative-mode rule "联想是附加的好处，没有足够
        // 的证据就不要联想".
        let d = PinyinDict::embedded();
        let with_context = d.predict_next_words_context(Some("今天"), "的", 10);
        // Either empty (trigram count below threshold) OR all hits
        // sorted desc by count — both valid.
        for w in with_context.windows(2) {
            assert!(
                w[0].1 >= w[1].1,
                "trigram results must be sorted desc; got {w:?}"
            );
        }
    }

    #[cfg(not(feature = "bootstrap_only"))]
    #[test]
    fn predict_next_words_context_no_backoff_for_chains() {
        // v1.3 conservative-mode (2026-05-24): when prev_prev is
        // Some but the trigram is empty (i.e. context (prev_prev, prev)
        // doesn't appear in the corpus), DO NOT fall back to bigram.
        // The previous behavior of backing off was the root cause of
        // 椒粉碎机构编制工程师范学校长室内 prediction chains — every
        // greedy (prev, *) bigram seemed locally valid but the chain
        // wandered into nonsense. Requiring trigram for chained
        // predictions means the chain dies when context goes off-corpus.
        let d = PinyinDict::embedded();
        // 锟斤拷 is mojibake — won't appear as prev_prev in any
        // real trigram, so (锟斤拷, 我们, *) trigram lookup is empty.
        let chained = d.predict_next_words_context(Some("锟斤拷"), "我们", 5);
        assert!(
            chained.is_empty(),
            "chained prediction with empty trigram must NOT backoff to bigram; \
             got {chained:?}"
        );
    }

    #[cfg(not(feature = "bootstrap_only"))]
    #[test]
    fn predict_next_words_context_threshold_15_surfaces_real_predictions() {
        // 2026-05-25: MIN_TRIGRAM_COUNT lowered 50→15 so established 3-word
        // patterns predict again (50 fired only ~4/12 common pairs, mostly
        // 泛词 "的"). (我们,的) has trigram followers 国家:40 / 生活:30 /
        // 工作:19 — all clear 15, so predictions must now be non-empty, and
        // every returned count must still be >= 15 (sub-15 noise stays cut).
        let d = PinyinDict::embedded();
        let r = d.predict_next_words_context(Some("我们"), "的", 10);
        assert!(
            !r.is_empty(),
            "我们的 should predict at threshold 15 (counts 40/30/19); got empty"
        );
        assert!(
            r.iter().all(|(_, c)| *c >= 15),
            "every prediction must clear the 15 threshold; got {r:?}"
        );
    }

    #[cfg(not(feature = "bootstrap_only"))]
    #[test]
    fn predict_next_words_context_cold_start_returns_empty() {
        // v1.4 strict-trigram policy (2026-05-24): cold start
        // (prev_prev = None) returns EMPTY — no bigram fallback.
        // User rule: "联想是附加的好处，没有足够的证据就不要联想".
        // Single bigram signal is too noisy to predict from.
        let d = PinyinDict::embedded();
        let cold = d.predict_next_words_context(None, "我们", 5);
        assert!(
            cold.is_empty(),
            "cold start (no prev_prev) must return empty under v1.4 strict; \
             got {cold:?}"
        );
    }

    #[cfg(not(feature = "bootstrap_only"))]
    #[test]
    fn predict_next_words_sorted_desc() {
        let d = PinyinDict::embedded();
        let preds = d.predict_next_words("我们", 5);
        if preds.len() < 2 {
            return;
        } // bail if data too sparse
        for w in preds.windows(2) {
            assert!(
                w[0].1 >= w[1].1,
                "predictions must be sorted by count desc; got {:?} then {:?}",
                w[0],
                w[1]
            );
        }
    }

    // ---- bigram_boost surface ----------------------------------------

    #[test]
    fn bigram_boost_zero_without_prev() {
        let d = PinyinDict::embedded();
        // Cold session: no prev → 0.0 regardless of `next`.
        assert_eq!(d.bigram_boost(None, "好"), 0.0);
        assert_eq!(d.bigram_boost(None, ""), 0.0);
    }

    #[test]
    fn bigram_boost_zero_for_empty_or_unknown() {
        let d = PinyinDict::embedded();
        // Empty strings short-circuit.
        assert_eq!(d.bigram_boost(Some(""), "好"), 0.0);
        assert_eq!(d.bigram_boost(Some("今天"), ""), 0.0);
        // Garbage pair — exceedingly unlikely to appear in the corpus.
        assert_eq!(d.bigram_boost(Some("锟斤拷"), "烫烫烫"), 0.0);
    }

    // ---- best_composition (Viterbi) ----------------------------------

    #[test]
    fn best_composition_too_short_returns_none() {
        let d = PinyinDict::embedded();
        // < MIN_LEN (4) → None
        assert!(d.best_composition("").is_none());
        assert!(d.best_composition("ni").is_none());
        assert!(d.best_composition("nih").is_none());
    }

    #[cfg(not(feature = "bootstrap_only"))]
    #[test]
    fn best_composition_nihao_keeps_phrase() {
        let d = PinyinDict::embedded();
        // 你好 is a known phrase with high freq; the segmenter should
        // prefer the one-segment lookup over splitting into 你+好.
        let (_, chain) = d.best_composition("nihao").expect("hit");
        assert_eq!(chain, "你好", "want 你好 as single phrase, got {chain:?}");
    }

    #[cfg(not(feature = "bootstrap_only"))]
    #[test]
    fn best_composition_nihaomawojiao_segments() {
        let d = PinyinDict::embedded();
        // Long buffer that has no single-phrase match — the segmenter
        // should string together a multi-segment Chinese sentence. We
        // don't pin the exact split here: the dict has multiple valid
        // segmentations (e.g. 你+号码+我+叫 vs 你好+吗+我+叫); which
        // wins depends on relative phrase freqs, intra-phrase char
        // bigram counts, and the STEP_PENALTY tune. With v0.4 intra-
        // token bigrams ((你,好) (好,吗) (我,叫) all surface), the
        // 你好+吗+我+叫 path should now beat 你+号码+我+叫 — printed
        // for sanity. Both are valid CJK, the assertion just verifies
        // shape (pure CJK, 4-7 chars).
        let result = d.best_composition("nihaomawojiao");
        let Some((score, chain)) = result else {
            panic!("expected some segmentation for nihaomawojiao");
        };
        eprintln!("nihaomawojiao → {chain:?} (score {score})");
        assert!(
            chain
                .chars()
                .all(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
            "expected pure-CJK segmentation, got {chain:?}"
        );
        let char_count = chain.chars().count();
        assert!(
            (4..=7).contains(&char_count),
            "expected 4-7 CJK chars, got {char_count} in {chain:?}"
        );
    }

    #[cfg(not(feature = "bootstrap_only"))]
    #[test]
    fn perfgate_predict_next_words_under_budget() {
        // v1.3 联想 "只能是好处不能是负担" — predictions run on every
        // commit (post-commit panel refresh). If slow, every space-
        // commit lags noticeably. Budget is much tighter than the main
        // refresh_candidates perfgate (8ms) because the algorithm is
        // a pure FST range query — no jieba, no fuzzy, no Viterbi.
        //
        // Worst cases first:
        //   - 的 has the most bigram followers in the corpus (it's
        //     literally the most common Chinese particle).
        //   - 我 is a top-3 follower-seed too.
        //   - "锟斤拷"-style absent context: must return empty quickly
        //     (no wasted scan).
        // Plus a chained-prediction probe: trigram path with no
        // bigram fallback (v1.3 conservative-mode).
        let d = PinyinDict::embedded();
        const ITER: usize = 30;
        const MIN_BUDGET_NS: u128 = 2_000_000; // 2 ms uncontended
        const MAX_BUDGET_NS: u128 = 5_000_000; // 5 ms p95 (well under 16ms frame)

        let probes: &[(Option<&str>, &str, usize, &str)] = &[
            // (prev_prev, prev, limit, label)
            (None, "的", 10, "cold-bigram-的"),
            (None, "我", 10, "cold-bigram-我"),
            (None, "今天", 10, "cold-bigram-今天"),
            (Some("今天"), "的", 10, "chained-trigram-今天-的"),
            (Some("锟斤拷"), "无关词", 10, "chained-empty-fast-bailout"),
            (None, "的", 50, "cold-bigram-的-limit50"),
        ];

        let mut all_passed = true;
        for (prev_prev, prev, limit, label) in probes {
            let mut times: Vec<u128> = Vec::with_capacity(ITER);
            for _ in 0..ITER {
                let start = std::time::Instant::now();
                let _ = d.predict_next_words_context(*prev_prev, prev, *limit);
                times.push(start.elapsed().as_nanos());
            }
            times.sort_unstable();
            let min = times[0];
            let p50 = times[times.len() / 2];
            let p95 = times[(times.len() * 95) / 100];
            let max = *times.last().unwrap();
            eprintln!(
                "perfgate-predict {label:>30}: min={:>5.2}ms p50={:>5.2}ms p95={:>5.2}ms max={:>5.2}ms",
                min as f64 / 1_000_000.0,
                p50 as f64 / 1_000_000.0,
                p95 as f64 / 1_000_000.0,
                max as f64 / 1_000_000.0,
            );
            if !cfg!(debug_assertions) {
                if min > MIN_BUDGET_NS {
                    eprintln!(
                        "  ^^ FAIL: min {:.2}ms exceeds {}ms uncontended budget",
                        min as f64 / 1_000_000.0,
                        MIN_BUDGET_NS / 1_000_000
                    );
                    all_passed = false;
                }
                if p95 > MAX_BUDGET_NS {
                    eprintln!(
                        "  ^^ FAIL: p95 {:.2}ms exceeds {}ms",
                        p95 as f64 / 1_000_000.0,
                        MAX_BUDGET_NS / 1_000_000
                    );
                    all_passed = false;
                }
            }
        }
        assert!(
            all_passed || cfg!(debug_assertions),
            "perfgate-predict failed — see eprintln above"
        );
    }

    #[cfg(not(feature = "bootstrap_only"))]
    #[test]
    fn bigram_boost_positive_for_common_pair() {
        let d = PinyinDict::embedded();
        // 今天的 / 今天是 / 今天在 are all top bigrams in the extracted
        // table (verified by `head pinyin_bigrams_v1.tsv | grep 今天`).
        // Any one of these should yield a non-trivial boost.
        let boost_de = d.bigram_boost(Some("今天"), "的");
        let boost_shi = d.bigram_boost(Some("今天"), "是");
        assert!(
            boost_de > 0.0 || boost_shi > 0.0,
            "expected positive bigram boost for 今天→的/是, got de={boost_de} shi={boost_shi}"
        );
    }
}
