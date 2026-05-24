//! Read side: parse the serialized buffer and answer `get` / `prefix` /
//! `iter` over the embedded bytes with no decompression step. Generic over
//! the byte container `D: AsRef<[u8]>` so the same reader works on a
//! `Vec<u8>`, a `&[u8]`, or a future memory-mapped file.
//!
//! Format v2 (see `builder::serialize`): states are byte-offset addressed
//! (no offset table); a transition stores `label`, a LEB128 back-delta to
//! the target state, and the target's LEB128 right-language count (for the
//! ordinal walk).

const HEADER_LEN: usize = 18; // magic4 + ver1 + width1 + value_count4 + root_off4 + state_count4

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
    value_count: u32,
    state_count: u32,
    root_rel: u32,
    blob_start: usize,
    values_start: usize,
}

#[inline]
fn rd_u32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// Read an unsigned LEB128 starting at `*p`, advancing `*p` past it.
#[inline]
fn rd_uvarint(b: &[u8], p: &mut usize) -> u64 {
    let mut v = 0u64;
    let mut shift = 0u32;
    loop {
        let byte = b[*p];
        *p += 1;
        v |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    v
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
        if b[4] != 2 {
            return Err(FsaError::BadVersion(b[4]));
        }
        let value_width = b[5] as usize;
        let value_count = rd_u32(b, 6);
        let root_rel = rd_u32(b, 10);
        let state_count = rd_u32(b, 14);
        let blob_start = HEADER_LEN;
        let values_len = value_count as usize * value_width;
        if b.len() < blob_start + values_len {
            return Err(FsaError::Truncated);
        }
        let values_start = b.len() - values_len;
        Ok(Self {
            data,
            value_width,
            value_count,
            state_count,
            root_rel,
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

    /// Total number of states in the automaton (diagnostics).
    pub fn state_count(&self) -> u32 {
        self.state_count
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

    #[inline]
    fn is_final(&self, rel: u32) -> bool {
        self.data.as_ref()[self.blob_start + rel as usize] & 1 != 0
    }

    /// Walk one state: from state at `rel`, take `byte`. Returns the target
    /// state's relative offset, adding to `ord` the rank contribution of
    /// everything that sorts before that branch. `None` if no such transition.
    #[inline]
    fn step(&self, rel: u32, byte: u8, ord: &mut u64) -> Option<u32> {
        let b = self.data.as_ref();
        let mut p = self.blob_start + rel as usize;
        let final_ = b[p] & 1 != 0;
        p += 1;
        if final_ {
            *ord += 1; // the (shorter) word ending here sorts first
        }
        let ntrans = rd_uvarint(b, &mut p);
        for _ in 0..ntrans {
            let label = b[p];
            p += 1;
            let delta = rd_uvarint(b, &mut p);
            let numt = rd_uvarint(b, &mut p);
            if label < byte {
                *ord += numt;
            } else if label == byte {
                return Some(rel - delta as u32);
            } else {
                break; // label-sorted
            }
        }
        None
    }

    /// Look up `key`.
    pub fn get(&self, key: &[u8]) -> Option<u64> {
        if self.value_count == 0 {
            return None;
        }
        let mut rel = self.root_rel;
        let mut ord = 0u64;
        for &byte in key {
            rel = self.step(rel, byte, &mut ord)?;
        }
        if self.is_final(rel) {
            Some(self.read_value(ord))
        } else {
            None
        }
    }

    /// `true` if any stored key starts with `prefix`.
    pub fn contains_prefix(&self, prefix: &[u8]) -> bool {
        self.walk_to(prefix).is_some()
    }

    /// All (key, value) pairs whose key starts with `prefix`, sorted.
    pub fn prefix(&self, prefix: &[u8]) -> Vec<(Vec<u8>, u64)> {
        let Some((rel, ord)) = self.walk_to(prefix) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut cur = prefix.to_vec();
        let mut ord = ord;
        self.collect(rel, &mut cur, &mut ord, &mut out);
        out
    }

    /// All (key, value) pairs in sorted order.
    pub fn iter(&self) -> Vec<(Vec<u8>, u64)> {
        self.prefix(b"")
    }

    fn walk_to(&self, prefix: &[u8]) -> Option<(u32, u64)> {
        if self.value_count == 0 {
            return None;
        }
        let mut rel = self.root_rel;
        let mut ord = 0u64;
        for &byte in prefix {
            rel = self.step(rel, byte, &mut ord)?;
        }
        Some((rel, ord))
    }

    /// DFS in label-sorted order from `rel`, appending accepted (key, value)
    /// pairs and advancing `ord`. Recursion depth ≤ longest key.
    fn collect(&self, rel: u32, cur: &mut Vec<u8>, ord: &mut u64, out: &mut Vec<(Vec<u8>, u64)>) {
        let b = self.data.as_ref();
        let mut p = self.blob_start + rel as usize;
        let final_ = b[p] & 1 != 0;
        p += 1;
        if final_ {
            out.push((cur.clone(), self.read_value(*ord)));
            *ord += 1;
        }
        let ntrans = rd_uvarint(b, &mut p);
        for _ in 0..ntrans {
            let label = b[p];
            p += 1;
            let delta = rd_uvarint(b, &mut p);
            let _num = rd_uvarint(b, &mut p);
            let target = rel - delta as u32;
            cur.push(label);
            self.collect(target, cur, ord, out);
            cur.pop();
        }
    }
}
