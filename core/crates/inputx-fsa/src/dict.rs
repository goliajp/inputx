//! `Dict` — a one-code-to-many-items map: each byte *code* maps to an ordered
//! list of `(item_bytes, u64)` pairs. Built for IME dictionaries where a code
//! (a pinyin string / a wubi code) yields several candidate words each with a
//! score.
//!
//! This is the two-level design that makes the index small: the [`Fsa`]
//! automaton holds only the *codes* (heavy prefix sharing → tiny), and the
//! items live once in a packed blob the automaton points into. Compared to
//! flattening `code\0word → score` into a single automaton, this keeps the
//! unique word bytes out of the state graph — on the shipped pinyin set it is
//! ~3.98 MB vs ~7.4 MB flattened (and beats the general-purpose `fst` crate's
//! 4.54 MB), at zero dependencies.
//!
//! Combined buffer layout: `magic "IXDC" (4) · fsa_len u32 · fsa_bytes · blob`.
//! Blob record (per code, addressed by the automaton's u64 value = byte
//! offset): `n_items uvarint · [item_len uvarint · item_bytes · value uvarint]×n`.

use alloc::vec::Vec;

use crate::builder::{Builder, write_uvarint};
use crate::reader::{Fsa, FsaError, rd_u32, rd_uvarint};

const MAGIC: &[u8; 4] = b"IXDC";

/// Accumulates `(code, item, value)` triples and serializes a [`Dict`].
#[derive(Default)]
pub struct DictBuilder {
    triples: Vec<(Vec<u8>, Vec<u8>, u64)>,
}

impl DictBuilder {
    pub fn new() -> Self {
        Self {
            triples: Vec::new(),
        }
    }

    /// Add `code → (item, value)`. A code may be inserted many times (one per
    /// item). Insertion order across codes does not matter (the builder
    /// groups + sorts); within a code, items are stored sorted by value
    /// descending, then item bytes — so position 0 is the top-scoring item
    /// and the output is fully deterministic.
    pub fn insert(&mut self, code: &[u8], item: &[u8], value: u64) {
        self.triples.push((code.to_vec(), item.to_vec(), value));
    }

    pub fn finish(mut self) -> Vec<u8> {
        // Group by code. Sort triples by (code, value desc, item) so each
        // code's run is contiguous and internally ranked.
        self.triples.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then(b.2.cmp(&a.2)) // value descending
                .then(a.1.cmp(&b.1))
        });

        let mut blob: Vec<u8> = Vec::new();
        let mut fsa = Builder::new();

        let mut i = 0;
        while i < self.triples.len() {
            let code = &self.triples[i].0;
            // span of this code
            let mut j = i;
            while j < self.triples.len() && &self.triples[j].0 == code {
                j += 1;
            }
            let offset = blob.len() as u64;
            fsa.insert(code, offset);
            write_uvarint(&mut blob, (j - i) as u64);
            for t in &self.triples[i..j] {
                write_uvarint(&mut blob, t.1.len() as u64);
                blob.extend_from_slice(&t.1);
                write_uvarint(&mut blob, t.2);
            }
            i = j;
        }

        let fsa_bytes = fsa.finish();
        let mut out = Vec::with_capacity(8 + fsa_bytes.len() + blob.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&(fsa_bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&fsa_bytes);
        out.extend_from_slice(&blob);
        out
    }
}

/// Read-only two-level dictionary over a byte container.
pub struct Dict<D> {
    data: D,
    fsa_lo: usize,
    fsa_hi: usize,
    blob_lo: usize,
}

impl<D: AsRef<[u8]>> Dict<D> {
    pub fn new(data: D) -> Result<Self, FsaError> {
        let b = data.as_ref();
        if b.len() < 8 {
            return Err(FsaError::Truncated);
        }
        if &b[0..4] != MAGIC {
            return Err(FsaError::BadMagic);
        }
        let fsa_len = rd_u32(b, 4) as usize;
        let fsa_lo = 8;
        let fsa_hi = fsa_lo + fsa_len;
        if b.len() < fsa_hi {
            return Err(FsaError::Truncated);
        }
        // Validate the embedded automaton up front.
        Fsa::new(&b[fsa_lo..fsa_hi])?;
        Ok(Self {
            data,
            fsa_lo,
            fsa_hi,
            blob_lo: fsa_hi,
        })
    }

    #[inline]
    fn fsa(&self) -> Fsa<&[u8]> {
        // Header already validated in `new`; this re-parse is ~constant.
        Fsa::new(&self.data.as_ref()[self.fsa_lo..self.fsa_hi])
            .expect("embedded fsa validated in Dict::new")
    }

    /// Number of distinct codes.
    pub fn len(&self) -> u64 {
        self.fsa().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Items for an exact `code`, in stored (value-desc) order. Empty if the
    /// code is absent. Allocates a `Vec`; hot paths should prefer the
    /// allocation-free [`get_for_each`](Self::get_for_each).
    pub fn get(&self, code: &[u8]) -> Vec<(Vec<u8>, u64)> {
        let mut out = Vec::new();
        self.get_for_each(code, |item, val| out.push((item.to_vec(), val)));
        out
    }

    /// Streaming variant of [`get`](Self::get): invoke `visit(item, value)`
    /// for each item of an exact `code` with no result allocation and no
    /// per-item copy (the `item` slice is valid only for the call). This is
    /// the per-keystroke entry the IME dict layer should use.
    pub fn get_for_each<F: FnMut(&[u8], u64)>(&self, code: &[u8], mut visit: F) {
        let Some(off) = self.fsa().get(code) else {
            return;
        };
        let b = self.data.as_ref();
        let mut p = self.blob_lo + off as usize;
        let Some(n) = rd_uvarint(b, &mut p) else {
            return;
        };
        for _ in 0..n {
            let Some(len) = rd_uvarint(b, &mut p).map(|l| l as usize) else {
                return;
            };
            let Some(end) = p.checked_add(len) else {
                return;
            };
            let Some(item) = b.get(p..end) else { return };
            p = end;
            let Some(val) = rd_uvarint(b, &mut p) else {
                return;
            };
            visit(item, val);
        }
    }

    /// `true` if any code starts with `prefix`.
    pub fn contains_prefix(&self, prefix: &[u8]) -> bool {
        self.fsa().contains_prefix(prefix)
    }

    /// All `(code, item, value)` triples whose code starts with `prefix`,
    /// codes in sorted order, items in stored order within each code.
    pub fn prefix(&self, prefix: &[u8]) -> Vec<(Vec<u8>, Vec<u8>, u64)> {
        let mut out = Vec::new();
        self.prefix_for_each(prefix, |code, item, val| {
            out.push((code.to_vec(), item.to_vec(), val))
        });
        out
    }

    /// Streaming variant of [`prefix`](Self::prefix): invoke `visit(code,
    /// item, value)` per item without materializing the result — the hot
    /// path (a bare-letter code prefix can match tens of thousands of items).
    /// The `code` and `item` slices are valid only for the call.
    pub fn prefix_for_each<F: FnMut(&[u8], &[u8], u64)>(&self, prefix: &[u8], mut visit: F) {
        let fsa = self.fsa();
        let b = self.data.as_ref();
        let blob_lo = self.blob_lo;
        fsa.prefix_for_each(prefix, |code, off| {
            let mut p = blob_lo + off as usize;
            let Some(n) = rd_uvarint(b, &mut p) else {
                return;
            };
            for _ in 0..n {
                let Some(len) = rd_uvarint(b, &mut p).map(|l| l as usize) else {
                    return;
                };
                let Some(end) = p.checked_add(len) else {
                    return;
                };
                let Some(item) = b.get(p..end) else { return };
                p = end;
                let Some(val) = rd_uvarint(b, &mut p) else {
                    return;
                };
                visit(code, item, val);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn basic() {
        let mut b = DictBuilder::new();
        b.insert(b"wo", "我".as_bytes(), 100);
        b.insert(b"wo", "握".as_bytes(), 40);
        b.insert(b"women", "我们".as_bytes(), 90);
        let dict = Dict::new(b.finish()).unwrap();
        // value-desc order within code
        assert_eq!(
            dict.get(b"wo"),
            vec![
                ("我".as_bytes().to_vec(), 100),
                ("握".as_bytes().to_vec(), 40)
            ]
        );
        assert_eq!(dict.get(b"women"), vec![("我们".as_bytes().to_vec(), 90)]);
        assert_eq!(dict.get(b"nope"), Vec::<(Vec<u8>, u64)>::new());
        assert_eq!(dict.len(), 2);
        // prefix "wo" → both codes
        let pre = dict.prefix(b"wo");
        assert_eq!(pre.len(), 3);
        assert_eq!(pre[0].0, b"wo");
        assert!(dict.contains_prefix(b"wom"));
        assert!(!dict.contains_prefix(b"x"));
    }

    #[test]
    fn edge_cases() {
        let mut b = DictBuilder::new();
        // code + item containing 0x00 / 0xFF; value at width boundaries.
        b.insert(b"a\x00b", b"\xff\x00", 0);
        b.insert(b"a\x00b", b"item2", u64::MAX);
        b.insert(b"", b"empty-code", 65_536); // empty code is a valid key
        let dict = Dict::new(b.finish()).unwrap();
        let wo = dict.get(b"a\x00b");
        assert_eq!(wo.len(), 2);
        // value-desc: u64::MAX item first
        assert_eq!(wo[0], (b"item2".to_vec(), u64::MAX));
        assert_eq!(wo[1], (b"\xff\x00".to_vec(), 0));
        assert_eq!(dict.get(b""), vec![(b"empty-code".to_vec(), 65_536)]);
    }

    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig { cases: 200, ..ProptestConfig::default() })]

        #[test]
        fn diff_against_oracle(
            triples in proptest::collection::vec(
                (
                    proptest::collection::vec(b'a'..=b'd', 1..5),
                    proptest::collection::vec(b'x'..=b'z', 1..4),
                    any::<u64>(),
                ),
                0..48,
            ),
            probes in proptest::collection::vec(proptest::collection::vec(b'a'..=b'd', 0..5), 0..16),
        ) {
            // Oracle: code → sorted (value desc, item) list, dedup exact (code,item) last-wins.
            let mut latest: BTreeMap<(Vec<u8>, Vec<u8>), u64> = BTreeMap::new();
            for (c, it, v) in &triples {
                latest.insert((c.clone(), it.clone()), *v);
            }
            let mut oracle: BTreeMap<Vec<u8>, Vec<(Vec<u8>, u64)>> = BTreeMap::new();
            for ((c, it), v) in &latest {
                oracle.entry(c.clone()).or_default().push((it.clone(), *v));
            }
            for items in oracle.values_mut() {
                items.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            }

            let mut b = DictBuilder::new();
            // Insert deduped (so builder sees last-wins values, matching oracle).
            for ((c, it), v) in &latest {
                b.insert(c, it, *v);
            }
            let dict = Dict::new(b.finish()).unwrap();

            prop_assert_eq!(dict.len(), oracle.len() as u64);
            for (c, items) in &oracle {
                prop_assert_eq!(&dict.get(c), items, "get {:?}", c);
                // get_for_each must yield exactly what get returns.
                let mut streamed: Vec<(Vec<u8>, u64)> = Vec::new();
                dict.get_for_each(c, |it, v| streamed.push((it.to_vec(), v)));
                prop_assert_eq!(&streamed, items, "get_for_each {:?}", c);
            }
            for p in &probes {
                let want: Vec<(Vec<u8>, Vec<u8>, u64)> = oracle
                    .iter()
                    .filter(|(c, _)| c.starts_with(p))
                    .flat_map(|(c, items)| items.iter().map(move |(it, v)| (c.clone(), it.clone(), *v)))
                    .collect();
                prop_assert_eq!(dict.prefix(p), want, "prefix {:?}", p);
            }
        }
    }
}
