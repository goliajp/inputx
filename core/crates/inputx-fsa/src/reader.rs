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
pub(crate) fn rd_u32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// Read an unsigned LEB128 starting at `*p`, advancing `*p` past it.
/// Bounds-safe: returns `None` if the varint runs off the end of `b` or
/// is malformed (>10 bytes), so a corrupt buffer degrades instead of
/// panicking.
#[inline]
pub(crate) fn rd_uvarint(b: &[u8], p: &mut usize) -> Option<u64> {
    let mut v = 0u64;
    let mut shift = 0u32;
    loop {
        let byte = *b.get(*p)?;
        *p += 1;
        v |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(v);
        }
        shift += 7;
        if shift >= 64 {
            return None; // malformed: varint too long
        }
    }
}

/// Max key length walked during a prefix DFS. Valid IME keys are tiny; this
/// only bounds recursion depth so a corrupt/cyclic buffer can't overflow the
/// stack (defense for untrusted input).
const MAX_WALK_DEPTH: usize = 4096;

/// Resolve a transition's back-delta to a target rel-offset: must be `> 0`
/// (strictly earlier — no self-loop) and not underflow. `None` on corrupt.
#[inline]
fn decode_target(rel: u32, delta: u64) -> Option<u32> {
    u32::try_from(delta)
        .ok()
        .filter(|&d| d > 0)
        .and_then(|d| rel.checked_sub(d))
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
        if b[4] != 3 {
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
            v |= u64::from(b.get(at + i).copied().unwrap_or(0)) << (8 * i);
        }
        v
    }

    #[inline]
    fn is_final(&self, rel: u32) -> bool {
        self.data
            .as_ref()
            .get(self.blob_start + rel as usize)
            .is_some_and(|x| x & 1 != 0)
    }

    /// Walk one state: from state at `rel`, take `byte`. Returns the target
    /// state's relative offset, adding to `ord` the rank contribution of
    /// everything that sorts before that branch. `None` if no such transition
    /// (or on any out-of-bounds read — a corrupt buffer degrades to "no
    /// match" rather than panicking).
    #[inline]
    fn step(&self, rel: u32, byte: u8, ord: &mut u64) -> Option<u32> {
        let b = self.data.as_ref();
        let mut p = self.blob_start + rel as usize;
        let flags = *b.get(p)?;
        p += 1;
        if flags & 1 != 0 {
            *ord += 1; // final: the (shorter) word ending here sorts first
        }
        if flags & 0b10 != 0 {
            // single-transition form: [flags, label, delta], no count.
            let label = *b.get(p)?;
            p += 1;
            let delta = rd_uvarint(b, &mut p)?;
            // Only one edge: take it iff it matches; its count is never needed.
            if label == byte {
                return rel.checked_sub(u32::try_from(delta).ok()?);
            }
            return None;
        }
        let ntrans = rd_uvarint(b, &mut p)?;
        for _ in 0..ntrans {
            let label = *b.get(p)?;
            p += 1;
            let delta = rd_uvarint(b, &mut p)?;
            let numt = rd_uvarint(b, &mut p)?;
            if label < byte {
                *ord += numt;
            } else if label == byte {
                return rel.checked_sub(u32::try_from(delta).ok()?);
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
        let mut out = Vec::new();
        self.prefix_for_each(prefix, |k, v| out.push((k.to_vec(), v)));
        out
    }

    /// Streaming variant: invoke `visit(key, value)` for every (key, value)
    /// whose key starts with `prefix`, in sorted order, without allocating a
    /// result vector. The `key` slice is valid only for the call. This is the
    /// hot-path entry — a bare-letter prefix can match tens of thousands of
    /// keys, and materializing them all would dominate cost.
    pub fn prefix_for_each<F: FnMut(&[u8], u64)>(&self, prefix: &[u8], mut visit: F) {
        if let Some((rel, ord)) = self.walk_to(prefix) {
            let mut cur = prefix.to_vec();
            let mut ord = ord;
            self.visit_subtree(rel, &mut cur, &mut ord, &mut visit);
        }
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

    /// DFS in label-sorted order from `rel`, invoking `visit(key, value)` for
    /// each accepted key and advancing `ord`. Recursion depth ≤ longest key.
    fn visit_subtree<F: FnMut(&[u8], u64)>(
        &self,
        rel: u32,
        cur: &mut Vec<u8>,
        ord: &mut u64,
        visit: &mut F,
    ) {
        // Depth guard: bounds recursion on a corrupt/cyclic buffer.
        if cur.len() > MAX_WALK_DEPTH {
            return;
        }
        let b = self.data.as_ref();
        let mut p = self.blob_start + rel as usize;
        let Some(&flags) = b.get(p) else { return };
        p += 1;
        if flags & 1 != 0 {
            visit(cur, self.read_value(*ord));
            *ord += 1;
        }
        if flags & 0b10 != 0 {
            // single-transition form: [flags, label, delta], no count.
            let Some(&label) = b.get(p) else { return };
            p += 1;
            let Some(delta) = rd_uvarint(b, &mut p) else { return };
            if let Some(target) = decode_target(rel, delta) {
                cur.push(label);
                self.visit_subtree(target, cur, ord, visit);
                cur.pop();
            }
            return;
        }
        let Some(ntrans) = rd_uvarint(b, &mut p) else { return };
        for _ in 0..ntrans {
            let Some(&label) = b.get(p) else { return };
            p += 1;
            let Some(delta) = rd_uvarint(b, &mut p) else { return };
            let Some(_num) = rd_uvarint(b, &mut p) else { return };
            if let Some(target) = decode_target(rel, delta) {
                cur.push(label);
                self.visit_subtree(target, cur, ord, visit);
                cur.pop();
            }
        }
    }
}
