//! Read side: parse the serialized buffer and answer `get` / `prefix` /
//! `iter` over the embedded bytes with no decompression step. Generic over
//! the byte container `D: AsRef<[u8]>` so the same reader works on a
//! `Vec<u8>`, a `&[u8]`, or a future memory-mapped file.

const HEADER_LEN: usize = 18; // magic4 + ver1 + width1 + state_count4 + value_count4 + root4
const TRANS_LEN: usize = 9; // label1 + target4 + num4

/// Error parsing an FSA buffer.
#[derive(Debug, PartialEq, Eq)]
pub enum FsaError {
    BadMagic,
    BadVersion(u8),
    Truncated,
}

/// A read-only minimal-FSA map over a byte container.
pub struct Fsa<D> {
    data: D,
    value_width: usize,
    state_count: u32,
    value_count: u32,
    root: u32,
    blob_start: usize,
    values_start: usize,
}

#[inline]
fn rd_u16(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}
#[inline]
fn rd_u32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

impl<D: AsRef<[u8]>> Fsa<D> {
    pub fn new(data: D) -> Result<Self, FsaError> {
        let b = data.as_ref();
        if b.len() < HEADER_LEN {
            return Err(FsaError::Truncated);
        }
        if &b[0..4] != b"IXFA" {
            return Err(FsaError::BadMagic);
        }
        if b[4] != 1 {
            return Err(FsaError::BadVersion(b[4]));
        }
        let value_width = b[5] as usize;
        let state_count = rd_u32(b, 6);
        let value_count = rd_u32(b, 10);
        let root = rd_u32(b, 14);
        let blob_start = HEADER_LEN + state_count as usize * 4;
        let values_len = value_count as usize * value_width;
        if b.len() < blob_start || b.len() < values_len {
            return Err(FsaError::Truncated);
        }
        let values_start = b.len() - values_len;
        if values_start < blob_start {
            return Err(FsaError::Truncated);
        }
        Ok(Self {
            data,
            value_width,
            state_count,
            value_count,
            root,
            blob_start,
            values_start,
        })
    }

    /// Number of keys stored.
    pub fn len(&self) -> u64 {
        u64::from(self.value_count)
    }

    pub fn is_empty(&self) -> bool {
        self.value_count == 0
    }

    #[inline]
    fn state_offset(&self, id: u32) -> usize {
        let b = self.data.as_ref();
        self.blob_start + rd_u32(b, HEADER_LEN + id as usize * 4) as usize
    }

    #[inline]
    fn read_value(&self, ord: u64) -> u64 {
        let b = self.data.as_ref();
        let at = self.values_start + ord as usize * self.value_width;
        let mut v = 0u64;
        for i in 0..self.value_width {
            v |= (b[at + i] as u64) << (8 * i);
        }
        v
    }

    /// Look up `key`. Returns its value if present.
    pub fn get(&self, key: &[u8]) -> Option<u64> {
        if self.value_count == 0 {
            return None;
        }
        let b = self.data.as_ref();
        let mut state = self.root;
        let mut ord: u64 = 0;
        for &byte in key {
            let so = self.state_offset(state);
            if b[so] & 1 != 0 {
                ord += 1; // the (shorter) word ending here sorts before our continuation
            }
            let ntrans = rd_u16(b, so + 1) as usize;
            let trans = so + 3;
            let mut next = None;
            for t in 0..ntrans {
                let rec = trans + t * TRANS_LEN;
                let label = b[rec];
                if label < byte {
                    ord += rd_u32(b, rec + 5) as u64; // skip all keys under this branch
                } else if label == byte {
                    next = Some(rd_u32(b, rec + 1));
                    break;
                } else {
                    break; // transitions are label-sorted
                }
            }
            state = next?;
        }
        let so = self.state_offset(state);
        if b[so] & 1 != 0 {
            Some(self.read_value(ord))
        } else {
            None
        }
    }

    /// `true` if any stored key starts with `prefix`.
    pub fn contains_prefix(&self, prefix: &[u8]) -> bool {
        self.walk_to(prefix).is_some()
    }

    /// All (key, value) pairs whose key starts with `prefix`, in sorted
    /// order. An empty prefix yields the whole map.
    pub fn prefix(&self, prefix: &[u8]) -> Vec<(Vec<u8>, u64)> {
        let Some((state, ord)) = self.walk_to(prefix) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut cur = prefix.to_vec();
        let mut ord = ord;
        self.collect(state, &mut cur, &mut ord, &mut out);
        out
    }

    /// All (key, value) pairs in sorted order.
    pub fn iter(&self) -> Vec<(Vec<u8>, u64)> {
        self.prefix(b"")
    }

    /// Walk `prefix` from the root, accumulating the number of keys that sort
    /// strictly before the prefix's subtree. Returns `(state, ord)` at the
    /// node reached, or `None` if no key has this prefix.
    fn walk_to(&self, prefix: &[u8]) -> Option<(u32, u64)> {
        if self.value_count == 0 {
            return None;
        }
        let b = self.data.as_ref();
        let mut state = self.root;
        let mut ord: u64 = 0;
        for &byte in prefix {
            let so = self.state_offset(state);
            if b[so] & 1 != 0 {
                ord += 1;
            }
            let ntrans = rd_u16(b, so + 1) as usize;
            let trans = so + 3;
            let mut next = None;
            for t in 0..ntrans {
                let rec = trans + t * TRANS_LEN;
                let label = b[rec];
                if label < byte {
                    ord += rd_u32(b, rec + 5) as u64;
                } else if label == byte {
                    next = Some(rd_u32(b, rec + 1));
                    break;
                } else {
                    break;
                }
            }
            state = next?;
        }
        Some((state, ord))
    }

    /// Depth-first, label-sorted traversal from `state`, appending each
    /// accepted (key, value) and advancing `ord`. Recursion depth is bounded
    /// by key length.
    fn collect(&self, state: u32, cur: &mut Vec<u8>, ord: &mut u64, out: &mut Vec<(Vec<u8>, u64)>) {
        let b = self.data.as_ref();
        let so = self.state_offset(state);
        if b[so] & 1 != 0 {
            out.push((cur.clone(), self.read_value(*ord)));
            *ord += 1;
        }
        let ntrans = rd_u16(b, so + 1) as usize;
        let trans = so + 3;
        for t in 0..ntrans {
            let rec = trans + t * TRANS_LEN;
            let label = b[rec];
            let target = rd_u32(b, rec + 1);
            cur.push(label);
            self.collect(target, cur, ord, out);
            cur.pop();
        }
    }

    /// Total number of states in the automaton (diagnostics).
    pub fn state_count(&self) -> u32 {
        self.state_count
    }
}
