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
//! # Format (v1, little-endian)
//!
//! ```text
//! magic "IXFA" (4) · version u8 · value_width u8 (1|2|4|8)
//! state_count u32 · value_count u32 · root u32
//! offsets:  [u32; state_count]   (byte offset of each state within the states blob)
//! states:   per state → final u8(bit0) · n_trans u16 · [label u8, target u32, num u32]×n
//! values:   [value_width bytes; value_count]   (always the tail of the buffer)
//! ```

#![forbid(unsafe_code)]

mod builder;
mod dict;
mod reader;

pub use builder::Builder;
pub use dict::{Dict, DictBuilder};
pub use reader::{Fsa, FsaError};

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
        let bytes = build(&[
            (b"nation", 1),
            (b"ration", 2),
            (b"station", 3),
        ]);
        let fsa = Fsa::new(bytes).unwrap();
        assert_eq!(fsa.get(b"nation"), Some(1));
        assert_eq!(fsa.get(b"ration"), Some(2));
        assert_eq!(fsa.get(b"station"), Some(3));
        assert_eq!(fsa.get(b"ation"), None);
    }

    #[test]
    fn prefix_scan() {
        let bytes = build(&[(b"a", 1), (b"ab", 2), (b"ac", 3), (b"b", 4)]);
        let fsa = Fsa::new(bytes).unwrap();
        let got: Vec<_> = fsa.prefix(b"a");
        assert_eq!(
            got,
            vec![
                (b"a".to_vec(), 1),
                (b"ab".to_vec(), 2),
                (b"ac".to_vec(), 3)
            ]
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

    /// Real-data size + correctness check against the shipped pinyin
    /// weights. Ignored by default (reads a large file, ~seconds to build).
    /// Run: `cargo test -p inputx-fsa --release real_pinyin -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_pinyin_size_and_correctness() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../inputx-pinyin/data/weights/weights.tsv"
        );
        let text = std::fs::read_to_string(path).expect("weights.tsv");
        let mut oracle: BTreeMap<Vec<u8>, u64> = BTreeMap::new();
        let mut b = Builder::new();
        for line in text.lines() {
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            let mut it = line.split('\t');
            let (Some(py), Some(word), Some(freq)) = (it.next(), it.next(), it.next()) else {
                continue;
            };
            let freq: u64 = freq.parse().unwrap_or(0);
            let mut key = py.as_bytes().to_vec();
            key.push(0);
            key.extend_from_slice(word.as_bytes());
            b.insert(&key, freq);
            oracle.insert(key, freq);
        }
        let bytes = b.finish();
        let fsa = Fsa::new(bytes.as_slice()).unwrap();
        eprintln!(
            "[inputx-fsa] pinyin: {} keys, {} states, {} bytes = {:.2} MB  (baseline fst pinyin.fst = 4.54 MB)",
            fsa.len(),
            fsa.state_count(),
            bytes.len(),
            bytes.len() as f64 / 1_048_576.0
        );
        for (k, v) in &oracle {
            assert_eq!(fsa.get(k), Some(*v), "get mismatch for {k:?}");
        }
        assert_eq!(fsa.len(), oracle.len() as u64);
    }

    /// Size probe for the two-level (code → word-list blob) design on real
    /// pinyin data. Ignored by default.
    /// Run: `cargo test -p inputx-fsa --release real_pinyin_twolevel -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_pinyin_twolevel_size() {
        fn uvarint(out: &mut Vec<u8>, mut v: u64) {
            loop {
                let mut byte = (v & 0x7f) as u8;
                v >>= 7;
                if v != 0 {
                    byte |= 0x80;
                }
                out.push(byte);
                if v == 0 {
                    break;
                }
            }
        }
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../inputx-pinyin/data/weights/weights.tsv"
        );
        let text = std::fs::read_to_string(path).expect("weights.tsv");
        // Measure on both the full set and the shipped set (MIN_FREQ=100,
        // what pinyin-build-fst actually ships → the fair 4.54MB comparison).
        for &min_freq in &[0u64, 100] {
            let mut groups: BTreeMap<String, Vec<(String, u64)>> = BTreeMap::new();
            let mut n_entries = 0usize;
            for line in text.lines() {
                if line.starts_with('#') || line.is_empty() {
                    continue;
                }
                let mut it = line.split('\t');
                let (Some(py), Some(word), Some(freq)) = (it.next(), it.next(), it.next()) else {
                    continue;
                };
                let freq: u64 = freq.parse().unwrap_or(0);
                if freq < min_freq {
                    continue;
                }
                groups
                    .entry(py.to_string())
                    .or_default()
                    .push((word.to_string(), freq));
                n_entries += 1;
            }
            // two-level: code → word-list blob
            let mut blob: Vec<u8> = Vec::new();
            let mut b = Builder::new();
            for (code, words) in &groups {
                b.insert(code.as_bytes(), blob.len() as u64);
                uvarint(&mut blob, words.len() as u64);
                for (w, f) in words {
                    uvarint(&mut blob, w.len() as u64);
                    blob.extend_from_slice(w.as_bytes());
                    uvarint(&mut blob, *f);
                }
            }
            let fsa_bytes = b.finish();
            let total = fsa_bytes.len() + blob.len();
            // single-level: code\0word → freq (for comparison)
            let mut sb = Builder::new();
            for (code, words) in &groups {
                for (w, f) in words {
                    let mut key = code.as_bytes().to_vec();
                    key.push(0);
                    key.extend_from_slice(w.as_bytes());
                    sb.insert(&key, *f);
                }
            }
            let single = sb.finish().len();
            eprintln!(
                "[min_freq={min_freq}] {n_entries} entries / {} codes  |  two-level {:.2}MB (fsa {:.2}+blob {:.2})  |  single-level {:.2}MB  |  baseline fst 4.54MB",
                groups.len(),
                total as f64 / 1_048_576.0,
                fsa_bytes.len() as f64 / 1_048_576.0,
                blob.len() as f64 / 1_048_576.0,
                single as f64 / 1_048_576.0
            );
        }
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
}
