//! `inputx-fsa` — a compact, zero-dependency ordered map from byte keys to
//! `u64`, backed by a **minimal acyclic finite-state automaton** (MA-FSA /
//! DAWG) plus a parallel value table addressed by lexicographic rank.
//!
//! This is a clean-room implementation written for the Inputx IME to replace
//! the `fst` crate on the shipped read path — same role (compressed,
//! mmap-friendly dictionary index with `get` + prefix/range scan), no
//! external dependencies. The algorithms (Revuz minimization, MA-FSA as a
//! minimal perfect hash via per-transition right-language counts) are
//! textbook; the code is original.
//!
//! # Model
//!
//! The automaton recognizes the *set* of keys (it carries no values inside
//! it — that keeps it minimal regardless of the values). Each accepted key
//! has a stable **ordinal** = its rank in sorted order, computed during the
//! walk by summing, at every step, the number of keys that sort before the
//! branch we take. Values live in a separate fixed-width array indexed by
//! that ordinal. This is the "numbered MA-FSA / minimal perfect hash"
//! construction.
//!
//! # Format (v3, little-endian)
//!
//! ```text
//! magic "IXFA" (4) · version u8(=3) · value_width u8 (1|2|4|8)
//! value_count u32 · root_off u32 · state_count u32        (18-byte header)
//! states:  byte-offset addressed (no offset table), post-order so every
//!          target precedes its source. Per state, a flags byte:
//!            bit0 = final, bit1 = single-transition.
//!          · single  → [flags, label u8, delta uvarint]   (count omitted)
//!          · multi   → [flags, n uvarint, (label u8, delta uvarint,
//!                       count uvarint) × n]
//!          `delta` is a back-distance to the target state; `count` is the
//!          target's right-language size (for the ordinal/rank walk).
//! values:  [value_width bytes; value_count]   (tail of the buffer)
//! ```
//!
//! # Example
//!
//! ```
//! use inputx_fsa::{Builder, Fsa};
//!
//! let mut b = Builder::new();
//! b.insert(b"apple", 1);
//! b.insert(b"apply", 2);
//! b.insert(b"banana", 3);
//! let fsa = Fsa::new(b.finish()).unwrap();
//!
//! assert_eq!(fsa.get(b"apple"), Some(1));
//! assert_eq!(fsa.get(b"grape"), None);
//!
//! // lazy, composable prefix scan
//! let app: Vec<_> = fsa.range(b"app").collect();
//! assert_eq!(app, vec![(b"apple".to_vec(), 1), (b"apply".to_vec(), 2)]);
//! ```

#![forbid(unsafe_code)]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

mod builder;
mod dict;
mod reader;

pub use builder::Builder;
pub use dict::{Dict, DictBuilder};
pub use reader::{Fsa, FsaError, FsaIter};

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// Build an Fsa from pairs and return the serialized bytes.
    fn build(pairs: &[(&[u8], u64)]) -> Vec<u8> {
        let mut b = Builder::new();
        for &(k, v) in pairs {
            b.insert(k, v);
        }
        b.finish()
    }

    #[test]
    fn empty() {
        let bytes = build(&[]);
        let fsa = Fsa::new(bytes).unwrap();
        assert_eq!(fsa.get(b"anything"), None);
        assert_eq!(fsa.len(), 0);
    }

    #[test]
    fn basic_get() {
        let bytes = build(&[(b"a", 1), (b"ab", 2), (b"ac", 3), (b"b", 4)]);
        let fsa = Fsa::new(bytes).unwrap();
        assert_eq!(fsa.get(b"a"), Some(1));
        assert_eq!(fsa.get(b"ab"), Some(2));
        assert_eq!(fsa.get(b"ac"), Some(3));
        assert_eq!(fsa.get(b"b"), Some(4));
        assert_eq!(fsa.get(b"c"), None);
        assert_eq!(fsa.get(b"abc"), None);
        assert_eq!(fsa.get(b""), None);
        assert_eq!(fsa.len(), 4);
    }

    #[test]
    fn empty_key_member() {
        let bytes = build(&[(b"", 7), (b"x", 9)]);
        let fsa = Fsa::new(bytes).unwrap();
        assert_eq!(fsa.get(b""), Some(7));
        assert_eq!(fsa.get(b"x"), Some(9));
    }

    #[test]
    fn shared_suffix_minimization() {
        // "ation" suffix shared → DAWG should merge those states. We can't
        // easily assert state count here, but correctness must hold.
        let bytes = build(&[(b"nation", 1), (b"ration", 2), (b"station", 3)]);
        let fsa = Fsa::new(bytes).unwrap();
        assert_eq!(fsa.get(b"nation"), Some(1));
        assert_eq!(fsa.get(b"ration"), Some(2));
        assert_eq!(fsa.get(b"station"), Some(3));
        assert_eq!(fsa.get(b"ation"), None);
    }

    #[test]
    fn lazy_iter() {
        let bytes = build(&[(b"a", 1), (b"ab", 2), (b"ac", 3), (b"b", 4)]);
        let fsa = Fsa::new(bytes).unwrap();
        // lazy iter == eager prefix(b"")
        assert_eq!(fsa.iter().collect::<Vec<_>>(), fsa.prefix(b""));
        // range == prefix; early termination via take
        assert_eq!(fsa.range(b"a").collect::<Vec<_>>(), fsa.prefix(b"a"));
        assert_eq!(
            fsa.range(b"a").take(2).collect::<Vec<_>>(),
            vec![(b"a".to_vec(), 1), (b"ab".to_vec(), 2)]
        );
        // composes + stops early without walking the rest
        assert_eq!(
            fsa.iter().find(|(k, _)| k == b"ac"),
            Some((b"ac".to_vec(), 3))
        );
        assert_eq!(fsa.range(b"z").next(), None);
    }

    #[test]
    fn prefix_scan() {
        let bytes = build(&[(b"a", 1), (b"ab", 2), (b"ac", 3), (b"b", 4)]);
        let fsa = Fsa::new(bytes).unwrap();
        let got: Vec<_> = fsa.prefix(b"a");
        assert_eq!(
            got,
            vec![(b"a".to_vec(), 1), (b"ab".to_vec(), 2), (b"ac".to_vec(), 3)]
        );
        assert_eq!(fsa.prefix(b"b"), vec![(b"b".to_vec(), 4)]);
        assert_eq!(fsa.prefix(b"z"), Vec::<(Vec<u8>, u64)>::new());
        assert_eq!(fsa.prefix(b"").len(), 4); // empty prefix → all
    }

    #[test]
    fn value_widths() {
        // Force each width tier and verify round-trip.
        for &maxv in &[0xFFu64, 0xFFFF, 0xFFFF_FFFF, 0xFFFF_FFFF_FF] {
            let bytes = build(&[(b"lo", 0), (b"hi", maxv)]);
            let fsa = Fsa::new(bytes).unwrap();
            assert_eq!(fsa.get(b"hi"), Some(maxv));
            assert_eq!(fsa.get(b"lo"), Some(0));
        }
    }

    #[test]
    fn duplicate_key_last_wins() {
        let bytes = build(&[(b"k", 1), (b"k", 2), (b"k", 3)]);
        let fsa = Fsa::new(bytes).unwrap();
        assert_eq!(fsa.get(b"k"), Some(3));
        assert_eq!(fsa.len(), 1);
    }

    // ─── Differential testing against a BTreeMap oracle (zero-dep) ────────
    //
    // The oracle is trivially correct; the FSA must match it for `get` and
    // `prefix` across every key + many random probes. This is what makes the
    // clean-room build *provably* correct without leaning on `fst`.

    /// Perf regression gate (run in release): `get` and a prefix scan must
    /// stay well under budget. Generous thresholds (~4x measured) so it
    /// flags real regressions, not scheduling jitter. Ignored by default;
    /// run: `cargo test -p inputx-fsa --release perfgate -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn perfgate_get_prefix_under_budget() {
        let mut b = DictBuilder::new();
        for i in 0..20_000u32 {
            let code = format!("code{i:05}");
            for j in 0..5u32 {
                b.insert(
                    code.as_bytes(),
                    format!("word{i}_{j}").as_bytes(),
                    u64::from(i * 10 + j),
                );
            }
        }
        let dict = Dict::new(b.finish()).unwrap();
        let codes: Vec<String> = (0..1000)
            .map(|i| format!("code{:05}", (i * 19) % 20_000))
            .collect();

        let iters = 50;
        let t = std::time::Instant::now();
        for _ in 0..iters {
            for c in &codes {
                std::hint::black_box(dict.get(c.as_bytes()));
            }
        }
        let per_get = t.elapsed().as_nanos() as f64 / (iters * codes.len()) as f64;
        eprintln!("[perfgate] get = {per_get:.0} ns/op");
        assert!(
            per_get < 20_000.0,
            "get regressed: {per_get:.0} ns/op (budget 20µs)"
        );

        let t = std::time::Instant::now();
        for _ in 0..iters {
            let mut n = 0u64;
            dict.prefix_for_each(b"code0", |_, _, v| n = n.wrapping_add(v));
            std::hint::black_box(n);
        }
        let per_pref = t.elapsed().as_nanos() as f64 / iters as f64;
        eprintln!("[perfgate] prefix(code0* = 5000 items) = {per_pref:.0} ns/op");
        assert!(
            per_pref < 5_000_000.0,
            "prefix regressed: {per_pref:.0} ns (budget 5ms)"
        );
    }

    use proptest::prelude::*;

    fn key_strategy() -> impl Strategy<Value = Vec<u8>> {
        proptest::collection::vec(b'a'..=b'e', 0..6)
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 200, ..ProptestConfig::default() })]

        #[test]
        fn diff_get_and_prefix(
            entries in proptest::collection::vec((key_strategy(), any::<u64>()), 0..64),
            probes in proptest::collection::vec(key_strategy(), 0..32),
        ) {
            // Oracle (last-write-wins on dup, matching Builder).
            let mut oracle: BTreeMap<Vec<u8>, u64> = BTreeMap::new();
            for (k, v) in &entries {
                oracle.insert(k.clone(), *v);
            }

            let mut b = Builder::new();
            for (k, v) in &entries {
                b.insert(k, *v);
            }
            let fsa = Fsa::new(b.finish()).unwrap();

            prop_assert_eq!(fsa.len(), oracle.len() as u64);

            // lazy iter yields exactly the eager prefix(b"") sequence.
            prop_assert_eq!(fsa.iter().collect::<Vec<_>>(), fsa.prefix(b""));

            // get matches on every oracle key + random probes.
            for (k, v) in &oracle {
                prop_assert_eq!(fsa.get(k), Some(*v), "get({:?})", k);
            }
            for p in &probes {
                prop_assert_eq!(fsa.get(p), oracle.get(p).copied(), "get probe {:?}", p);
            }

            // prefix matches the oracle's sorted range for every probe prefix.
            for p in &probes {
                let want: Vec<(Vec<u8>, u64)> = oracle
                    .iter()
                    .filter(|(k, _)| k.starts_with(p))
                    .map(|(k, v)| (k.clone(), *v))
                    .collect();
                prop_assert_eq!(fsa.prefix(p), want, "prefix {:?}", p);
            }
        }
    }

    // ─── Robustness + wide-byte coverage ─────────────────────────────────

    #[test]
    fn rejects_bad_buffers() {
        assert!(matches!(Fsa::new(&b""[..]), Err(FsaError::Truncated)));
        assert!(matches!(
            Fsa::new(&b"XXXX..............."[..]),
            Err(FsaError::BadMagic)
        ));
        let mut bad = b"IXFA".to_vec();
        bad.push(99); // version
        bad.extend(std::iter::repeat_n(0u8, 20));
        assert!(matches!(
            Fsa::new(bad.as_slice()),
            Err(FsaError::BadVersion(99))
        ));
    }

    #[test]
    fn keys_with_zero_and_high_bytes() {
        // Keys are arbitrary bytes — 0x00 / 0xFF carry no special meaning.
        let bytes = build(&[(b"\x00", 1), (b"\x00\xff", 2), (b"\xff", 3), (b"a\x00b", 4)]);
        let fsa = Fsa::new(bytes).unwrap();
        assert_eq!(fsa.get(b"\x00"), Some(1));
        assert_eq!(fsa.get(b"\x00\xff"), Some(2));
        assert_eq!(fsa.get(b"\xff"), Some(3));
        assert_eq!(fsa.get(b"a\x00b"), Some(4));
        assert_eq!(fsa.get(b"\x00\x00"), None);
        assert_eq!(fsa.prefix(b"\x00").len(), 2);
    }

    #[test]
    fn wide_alphabet_single_state() {
        // A root with all 256 labels exercises the u16 n_trans path.
        let pairs: Vec<(Vec<u8>, u64)> =
            (0u16..256).map(|b| (vec![b as u8], u64::from(b))).collect();
        let mut bld = Builder::new();
        for (k, v) in &pairs {
            bld.insert(k, *v);
        }
        let fsa = Fsa::new(bld.finish()).unwrap();
        assert_eq!(fsa.len(), 256);
        for (k, v) in &pairs {
            assert_eq!(fsa.get(k), Some(*v));
        }
    }

    fn wide_key() -> impl Strategy<Value = Vec<u8>> {
        proptest::collection::vec(any::<u8>(), 0..5)
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 200, ..ProptestConfig::default() })]

        /// Full-byte-range keys (0x00 / 0xFF included) — get + prefix match oracle.
        #[test]
        fn diff_wide_bytes(
            entries in proptest::collection::vec((wide_key(), any::<u64>()), 0..48),
            probes in proptest::collection::vec(wide_key(), 0..24),
        ) {
            let mut oracle: BTreeMap<Vec<u8>, u64> = BTreeMap::new();
            for (k, v) in &entries { oracle.insert(k.clone(), *v); }
            let mut b = Builder::new();
            for (k, v) in &entries { b.insert(k, *v); }
            let fsa = Fsa::new(b.finish()).unwrap();
            for (k, v) in &oracle { prop_assert_eq!(fsa.get(k), Some(*v)); }
            for p in &probes {
                let want: Vec<(Vec<u8>, u64)> = oracle.iter()
                    .filter(|(k, _)| k.starts_with(p))
                    .map(|(k, v)| (k.clone(), *v)).collect();
                prop_assert_eq!(fsa.prefix(p), want);
            }
        }

        /// `Fsa::new` never panics on arbitrary input — Ok or Err, never crash.
        #[test]
        fn new_no_panic(bytes in proptest::collection::vec(any::<u8>(), 0..128)) {
            let _ = Fsa::new(bytes.as_slice());
        }

        /// Corrupt-buffer robustness: take a VALID Fsa/Dict, flip arbitrary
        /// bytes (header or body), then new + get + prefix must never panic —
        /// at worst Err on new, or graceful empty/None results. This is the
        /// untrusted-input contract for the published crate.
        #[test]
        fn fuzz_mutated_buffer_no_panic(
            muts in proptest::collection::vec((any::<usize>(), any::<u8>()), 0..40),
            probe in proptest::collection::vec(any::<u8>(), 0..6),
        ) {
            // ── Fsa ──
            let mut fb = Builder::new();
            for i in 0..60u32 {
                fb.insert(format!("key{i:03}").as_bytes(), u64::from(i) * 7);
            }
            let mut bytes = fb.finish();
            for (idx, val) in &muts {
                if !bytes.is_empty() {
                    let i = idx % bytes.len();
                    bytes[i] = *val;
                }
            }
            if let Ok(fsa) = Fsa::new(bytes.as_slice()) {
                let _ = fsa.get(&probe);
                let _ = fsa.contains_prefix(&probe);
                let _ = fsa.prefix(&probe); // walk + visit_subtree
            }

            // ── Dict ──
            let mut db = DictBuilder::new();
            for i in 0..60u32 {
                db.insert(format!("c{}", i % 12).as_bytes(), format!("item{i}").as_bytes(), u64::from(i));
            }
            let mut dbytes = db.finish();
            for (idx, val) in &muts {
                if !dbytes.is_empty() {
                    let i = idx % dbytes.len();
                    dbytes[i] = *val;
                }
            }
            if let Ok(dict) = Dict::new(dbytes.as_slice()) {
                let _ = dict.get(&probe);
                let _ = dict.prefix(&probe);
                dict.get_for_each(&probe, |_, _| {});
            }
        }
    }
}
