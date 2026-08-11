//! Property tests for the L0 ranking model.
//!
//! Random ops under verified invariants — complements the targeted unit
//! tests in `dict.rs::tests`. All operations go through the public API.
//!
//! Gated to default features: bootstrap dict has too few multi-candidate
//! entries to make pin-vs-default observable in most cases.

#![cfg(not(feature = "bootstrap_only"))]

use std::collections::HashMap;

use proptest::prelude::*;
use proptest::sample;

use inputx_pinyin::{L0Snapshot, PinyinDict};

/// A curated set of (pinyin, word) pairs known to exist in the embedded
/// dictionary AND known to have ≥ 2 candidates per pinyin (gives
/// promote/dethrone tests something to compare against).
fn sample_entries() -> Vec<(String, String)> {
    let dict = PinyinDict::embedded();
    let mut out = Vec::new();
    for code in [
        "shi", "wo", "ni", "ma", "ta", "ji", "li", "ba", "fa", "ge", "he", "ke",
    ] {
        for word in dict.lookup(code) {
            out.push((code.to_string(), word));
        }
    }
    assert!(
        out.len() >= 20,
        "embedded dict yielded too few sample entries"
    );
    out
}

fn entry_strategy() -> impl Strategy<Value = (String, String)> {
    let entries = sample_entries();
    sample::select(entries)
}

proptest! {
    /// Repeated picks NEVER pin, however many times (auto-pin removed
    /// 2026-07-20). Candidate order stays at the dictionary's ruling
    /// unless the user explicitly pins.
    #[test]
    fn record_pick_never_promotes(
        entry in entry_strategy(),
        n in 1u32..12,
    ) {
        let dict = PinyinDict::embedded();
        let (pinyin, word) = entry;
        let before = dict.lookup(&pinyin);
        for _ in 1..=n {
            dict.record_pick(&pinyin, &word);
        }
        prop_assert_eq!(dict.l0_pin_count(), 0);
        prop_assert_eq!(dict.lookup(&pinyin), before);
    }

    /// Picking a word that doesn't exist for `pinyin` never modifies state.
    #[test]
    fn record_pick_unknown_word_is_noop(
        pinyin in "[a-z]{1,5}",
        bogus in "[A-Z]{8,16}",   // uppercase ascii guarantees no FST hit
    ) {
        let dict = PinyinDict::embedded();
        for _ in 0..5 {
            dict.record_pick(&pinyin, &bogus);
        }
        prop_assert_eq!(dict.l0_pin_count(), 0);
        prop_assert_eq!(dict.l0_pending_count(), 0);
    }

    /// `forget(pinyin)` is idempotent: a second call after the first
    /// removes nothing.
    #[test]
    fn forget_is_idempotent(entry in entry_strategy()) {
        let dict = PinyinDict::embedded();
        let (pinyin, word) = entry;
        prop_assert!(dict.pin(&pinyin, &word));
        prop_assert!(dict.forget(&pinyin));
        prop_assert!(!dict.forget(&pinyin));
        prop_assert_eq!(dict.l0_pin_count(), 0);
    }

    /// After `pin(pinyin, word)`, `lookup(pinyin)[0] == word`.
    #[test]
    fn pin_moves_word_to_top(entry in entry_strategy()) {
        let dict = PinyinDict::embedded();
        let (pinyin, word) = entry;
        prop_assert!(dict.pin(&pinyin, &word));
        let candidates = dict.lookup(&pinyin);
        prop_assert_eq!(candidates.first().map(String::as_str), Some(word.as_str()));
    }

    /// `export_l0` then `import_l0` on a fresh dict reproduces both pins
    /// and pick counters (where they survive validation).
    #[test]
    fn export_then_import_roundtrips(
        pin_entries in proptest::collection::vec(entry_strategy(), 0..5),
        pick_entries in proptest::collection::vec(entry_strategy(), 0..5),
    ) {
        let src = PinyinDict::embedded();
        let mut pinned: HashMap<String, String> = HashMap::new();
        for (pinyin, word) in &pin_entries {
            if src.pin(pinyin, word) {
                pinned.insert(pinyin.clone(), word.clone());
            }
        }
        for (pinyin, word) in &pick_entries {
            // Don't push picks onto pins (record_pick clears counters on
            // promotion — separate test covers that).
            if !pinned.contains_key(pinyin) {
                src.record_pick(pinyin, word);
            }
        }

        let snap = src.export_l0();
        let dst = PinyinDict::embedded();
        let _ = dst.import_l0(snap);

        for (pinyin, word) in &pinned {
            let cands = dst.lookup(pinyin);
            prop_assert_eq!(
                cands.first().map(String::as_str),
                Some(word.as_str()),
                "pin {} → {} didn't survive roundtrip",
                pinyin, word
            );
        }
    }

    /// `import_l0` with an invalid word entry doesn't poison state — only
    /// the invalid entry is dropped.
    #[test]
    fn import_drops_invalid_keeps_valid(entry in entry_strategy()) {
        let (pinyin, word) = entry;
        let dict = PinyinDict::embedded();
        let snap = L0Snapshot {
            pins: vec![
                (pinyin.clone(), word.clone()),                 // valid
                (pinyin.clone(), "thisisnotinthelexicon".into()), // invalid
            ],
            pick_counts: vec![],
            user_bigram: vec![],
        };
        let accepted = dict.import_l0(snap);
        prop_assert_eq!(accepted, 1);
        let cands = dict.lookup(&pinyin);
        prop_assert_eq!(cands.first().map(String::as_str), Some(word.as_str()));
    }

    /// Lookup is deterministic — same input, same result, repeatedly. L0
    /// has interior mutability so this is a non-trivial guarantee.
    #[test]
    fn lookup_is_deterministic(pinyin in "[a-z]{1,5}") {
        let dict = PinyinDict::embedded();
        let first = dict.lookup(&pinyin);
        for _ in 0..5 {
            prop_assert_eq!(dict.lookup(&pinyin), first.clone());
        }
    }
}

#[test]
fn smoke_l0_starts_empty() {
    let dict = PinyinDict::embedded();
    assert_eq!(dict.l0_pin_count(), 0);
    assert_eq!(dict.l0_pending_count(), 0);
}
