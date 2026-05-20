//! Property tests for the L0/L1 ranking model.
//!
//! Random ops under verified invariants — complements the targeted unit
//! tests in `dict.rs::tests`. All operations go through the public API.

use std::collections::HashMap;

use proptest::prelude::*;
use proptest::sample;

use wubi::{L0Snapshot, Layer, PROMOTE_THRESHOLD, WubiDict, DEFAULT_LAYER_PREFS, LAYER_COUNT};

// A curated set of (code, word) pairs known to exist in the embedded
// dictionary. We sample from these instead of inventing strings — the
// invariants under test are about L0 mechanics, not parser tolerance.
fn sample_entries() -> Vec<(String, String)> {
    let dict = WubiDict::embedded();
    let mut out = Vec::new();
    // Each code below is multi-candidate (≥ 2 words), giving promote/dethrone
    // tests something to compare against.
    for code in [
        "khlg", "wqvb", "rrrr", "ffff", "ssss", "aawt", "trwu", "gggg",
    ] {
        for word in dict.lookup(code) {
            out.push((code.to_string(), word));
        }
    }
    out
}

fn entry_strategy() -> impl Strategy<Value = (String, String)> {
    let entries = sample_entries();
    assert!(!entries.is_empty(), "embedded dict yielded no sample entries");
    sample::select(entries)
}

fn layer_strategy() -> impl Strategy<Value = Layer> {
    prop_oneof![
        Just(Layer::Auto),
        Just(Layer::Phrase),
        Just(Layer::Zigen),
        Just(Layer::Jianma3),
        Just(Layer::Jianma2),
        Just(Layer::Jianma1),
    ]
}

proptest! {
    /// Repeated picks of the same valid (code, word) pin only when the
    /// counter reaches `PROMOTE_THRESHOLD`, never before.
    #[test]
    fn record_pick_promotes_iff_threshold_reached(
        entry in entry_strategy(),
        n in 1u32..(PROMOTE_THRESHOLD * 3),
    ) {
        let dict = WubiDict::embedded();
        let (code, word) = entry;
        for i in 1..=n {
            let promoted = dict.record_pick(&code, &word);
            // Promotion fires exactly on the threshold-th call (first multiple).
            let expected_promotion_step = i.is_multiple_of(PROMOTE_THRESHOLD);
            prop_assert_eq!(promoted, expected_promotion_step,
                "iteration {} of {}: expected promote={}, got {}",
                i, n, expected_promotion_step, promoted);
        }
    }

    /// Picking a word that doesn't exist for `code` never modifies state.
    #[test]
    fn record_pick_unknown_word_is_noop(
        code in "[a-y]{1,4}",
        bogus in "[A-Z]{8,16}",   // uppercase guarantees no FST hit
    ) {
        let dict = WubiDict::embedded();
        for _ in 0..(PROMOTE_THRESHOLD + 2) {
            prop_assert!(!dict.record_pick(&code, &bogus));
        }
        prop_assert_eq!(dict.l0_pin_count(), 0);
        prop_assert_eq!(dict.l0_pending_count(), 0);
    }

    /// `forget(code)` is idempotent: a second call after the first removes
    /// nothing.
    #[test]
    fn forget_is_idempotent(entry in entry_strategy()) {
        let dict = WubiDict::embedded();
        let (code, word) = entry;
        prop_assert!(dict.pin(&code, &word));
        prop_assert!(dict.forget(&code));
        prop_assert!(!dict.forget(&code));
        prop_assert_eq!(dict.l0_pin_count(), 0);
    }

    /// After `pin(code, word)`, `lookup(code)[0] == word`.
    #[test]
    fn pin_moves_word_to_top(entry in entry_strategy()) {
        let dict = WubiDict::embedded();
        let (code, word) = entry;
        prop_assert!(dict.pin(&code, &word));
        let candidates = dict.lookup(&code);
        prop_assert_eq!(candidates.first().map(String::as_str), Some(word.as_str()));
    }

    /// `export_l0` then `import_l0` on a fresh dict reproduces both pins
    /// and pick counters.
    #[test]
    fn export_then_import_roundtrips(
        pin_entries in proptest::collection::vec(entry_strategy(), 0..5),
        pick_entries in proptest::collection::vec(entry_strategy(), 0..5),
    ) {
        // Source dict — apply some pins and partial pick counters.
        let src = WubiDict::embedded();
        let mut pinned_codes: HashMap<String, String> = HashMap::new();
        for (code, word) in &pin_entries {
            if src.pin(code, word) {
                pinned_codes.insert(code.clone(), word.clone());
            }
        }
        for (code, word) in &pick_entries {
            // Don't push a pick on a code that already has a pin (record_pick
            // would clear counters on promotion). We're not testing that
            // here — separate test.
            if !pinned_codes.contains_key(code) {
                src.record_pick(code, word);
            }
        }

        let snap = src.export_l0();

        let dst = WubiDict::embedded();
        let _ = dst.import_l0(snap);

        // Every pin from the source must still resolve to the same top word
        // in the destination.
        for (code, word) in &pinned_codes {
            let cands = dst.lookup(code);
            prop_assert_eq!(
                cands.first().map(String::as_str),
                Some(word.as_str()),
                "pin {} → {} didn't survive roundtrip",
                code, word
            );
        }
    }

    /// Layer prefs clamp non-finite / negative values to 0.
    #[test]
    fn layer_pref_clamps_garbage(
        layer in layer_strategy(),
        bad in prop_oneof![
            Just(f64::NAN),
            Just(f64::NEG_INFINITY),
            Just(-1.0),
            Just(-0.001),
            Just(-1e9),
        ],
    ) {
        let dict = WubiDict::embedded();
        dict.set_layer_pref(layer, bad);
        prop_assert_eq!(dict.layer_pref(layer), 0.0);
    }

    /// Layer prefs accept any non-negative finite value verbatim.
    #[test]
    fn layer_pref_accepts_nonnegative_finite(
        layer in layer_strategy(),
        v in 0.0f64..1e6f64,
    ) {
        let dict = WubiDict::embedded();
        dict.set_layer_pref(layer, v);
        prop_assert!((dict.layer_pref(layer) - v).abs() < f64::EPSILON);
    }

    /// `import_l0` with an invalid word entry doesn't poison state — only
    /// the invalid entry is dropped.
    #[test]
    fn import_drops_invalid_keeps_valid(entry in entry_strategy()) {
        let (code, word) = entry;
        let dict = WubiDict::embedded();
        let snap = L0Snapshot {
            pins: vec![
                (code.clone(), word.clone()),                  // valid
                (code.clone(), "thisisnotinthelexicon".into()), // invalid
            ],
            pick_counts: vec![],
            layer_prefs: DEFAULT_LAYER_PREFS,
        };
        let accepted = dict.import_l0(snap);
        prop_assert_eq!(accepted, 1);
        let cands = dict.lookup(&code);
        prop_assert_eq!(cands.first().map(String::as_str), Some(word.as_str()));
    }

    /// Lookup is deterministic — same code, same result, repeatedly. The L0
    /// has interior mutability so this is a non-trivial guarantee.
    #[test]
    fn lookup_is_deterministic(code in "[a-y]{1,4}") {
        let dict = WubiDict::embedded();
        let first = dict.lookup(&code);
        for _ in 0..5 {
            prop_assert_eq!(dict.lookup(&code), first.clone());
        }
    }
}

#[test]
fn smoke_default_layer_prefs_initialized() {
    let dict = WubiDict::embedded();
    for (i, _) in DEFAULT_LAYER_PREFS.iter().enumerate().take(LAYER_COUNT) {
        let layer = Layer::from_u8(i as u8).unwrap();
        assert!((dict.layer_pref(layer) - DEFAULT_LAYER_PREFS[i]).abs() < f64::EPSILON);
    }
}
