//! Read side: parse the serialized buffer and answer `get` / `prefix` /
//! `iter` over the embedded bytes with no decompression step. Generic over
//! the byte container `D: AsRef<[u8]>` so the same reader works on a
//! `Vec<u8>`, a `&[u8]`, or a future memory-mapped file.
//!
//! Format v3 (see `builder::serialize`): states are byte-offset addressed
//! (no offset table). A flags byte (bit0=final, bit1=single-transition)
//! precedes the transitions; single-transition nodes drop the arity + the
//! count. A transition stores `label`, a LEB128 back-delta to the target,
//! and (multi-node only) the target's LEB128 right-language count.

use alloc::vec::Vec;

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
    /// Load an FSA from a serialized byte buffer. Accepts anything that
    /// implements `AsRef<[u8]>` (`&[u8]`, `Vec<u8>`, `mmap::Mmap`, etc.).
    /// The buffer is parsed header-only — no allocation per entry.
    ///
    /// Returns [`FsaError::BadMagic`] / [`FsaError::BadVersion`] /
    /// [`FsaError::Truncated`] on invalid input; never panics.
    ///
    /// ```
    /// use inputx_fsa::{Builder, Fsa, FsaError};
    /// let mut b = Builder::new();
    /// b.insert(b"hello", 42);
    /// let bytes = b.finish();
    ///
    /// let fsa = Fsa::new(&bytes[..]).unwrap();
    /// assert_eq!(fsa.len(), 1);
    ///
    /// // Corrupt magic → graceful error, no panic.
    /// let mut bad = bytes.clone();
    /// bad[0] = 0;
    /// assert!(matches!(Fsa::new(&bad[..]), Err(FsaError::BadMagic)));
    /// ```
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

    /// Look up `key` — `Some(value)` for an exact match, `None` otherwise.
    /// Constant cost per byte; the buffer is never copied.
    ///
    /// ```
    /// use inputx_fsa::{Builder, Fsa};
    /// let mut b = Builder::new();
    /// b.insert(b"ni", 1);
    /// b.insert(b"nihao", 100);
    /// let fsa = Fsa::new(b.finish()).unwrap();
    /// assert_eq!(fsa.get(b"ni"), Some(1));
    /// assert_eq!(fsa.get(b"nihao"), Some(100));
    /// assert_eq!(fsa.get(b"niha"), None);  // prefix-only, not a key
    /// assert_eq!(fsa.get(b"absent"), None);
    /// ```
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
    ///
    /// ```
    /// use inputx_fsa::{Builder, Fsa};
    /// let mut b = Builder::new();
    /// for (k, v) in [(&b"apple"[..], 1u64), (b"apply", 2), (b"banana", 3)] {
    ///     b.insert(k, v);
    /// }
    /// let fsa = Fsa::new(b.finish()).unwrap();
    ///
    /// let mut count = 0;
    /// let mut last_value = 0;
    /// fsa.prefix_for_each(b"app", |_key, value| {
    ///     count += 1;
    ///     last_value = value;
    /// });
    /// assert_eq!(count, 2);          // apple + apply
    /// assert_eq!(last_value, 2);     // "apply" sorts last
    /// ```
    pub fn prefix_for_each<F: FnMut(&[u8], u64)>(&self, prefix: &[u8], mut visit: F) {
        if let Some((rel, ord)) = self.walk_to(prefix) {
            let mut cur = prefix.to_vec();
            let mut ord = ord;
            self.visit_subtree(rel, &mut cur, &mut ord, &mut visit);
        }
    }

    /// Lazy iterator over all (key, value) pairs in sorted order.
    pub fn iter(&self) -> FsaIter<'_, D> {
        self.range(b"")
    }

    /// Lazy iterator over (key, value) pairs whose key starts with `prefix`,
    /// in sorted order. Unlike [`prefix`](Self::prefix) it allocates no result
    /// vector and supports early termination (`.take`, `.find`, `break`).
    pub fn range(&self, prefix: &[u8]) -> FsaIter<'_, D> {
        let (to_enter, ord) = match self.walk_to(prefix) {
            Some((rel, ord)) => (Some(rel), ord),
            None => (None, 0),
        };
        FsaIter {
            fsa: self,
            stack: Vec::new(),
            cur: prefix.to_vec(),
            ord,
            to_enter,
        }
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
            let Some(delta) = rd_uvarint(b, &mut p) else {
                return;
            };
            if let Some(target) = decode_target(rel, delta) {
                cur.push(label);
                self.visit_subtree(target, cur, ord, visit);
                cur.pop();
            }
            return;
        }
        let Some(ntrans) = rd_uvarint(b, &mut p) else {
            return;
        };
        for _ in 0..ntrans {
            let Some(&label) = b.get(p) else { return };
            p += 1;
            let Some(delta) = rd_uvarint(b, &mut p) else {
                return;
            };
            let Some(_num) = rd_uvarint(b, &mut p) else {
                return;
            };
            if let Some(target) = decode_target(rel, delta) {
                cur.push(label);
                self.visit_subtree(target, cur, ord, visit);
                cur.pop();
            }
        }
    }
}

/// One state being expanded on the iterator's explicit DFS stack.
struct IterFrame {
    rel: u32,
    p: usize,        // byte cursor: next transition record to read
    remaining: u64,  // transitions left in this state
    base_len: usize, // `cur.len()` at this state (truncate target between children)
    single: bool,    // single-transition form (no per-edge count)
}

/// Lazy, allocation-light iterator over an [`Fsa`]'s (key, value) pairs in
/// sorted order — see [`Fsa::iter`] / [`Fsa::range`]. Pre-order DFS via an
/// explicit stack (depth = key length), so it composes with the `Iterator`
/// adapters and stops early without walking the whole automaton. On a corrupt
/// buffer it simply ends (consistent with the bounds-safe reader).
pub struct FsaIter<'a, D> {
    fsa: &'a Fsa<D>,
    stack: Vec<IterFrame>,
    cur: Vec<u8>,
    ord: u64,
    to_enter: Option<u32>,
}

impl<D: AsRef<[u8]>> Iterator for FsaIter<'_, D> {
    type Item = (Vec<u8>, u64);

    fn next(&mut self) -> Option<Self::Item> {
        let b = self.fsa.data.as_ref();
        loop {
            if let Some(rel) = self.to_enter.take() {
                // Enter a state: push its frame; if final, yield its key now.
                let mut p = self.fsa.blob_start + rel as usize;
                let flags = *b.get(p)?;
                p += 1;
                let single = flags & 0b10 != 0;
                let final_ = flags & 1 != 0;
                let remaining = if single { 1 } else { rd_uvarint(b, &mut p)? };
                self.stack.push(IterFrame {
                    rel,
                    p,
                    remaining,
                    base_len: self.cur.len(),
                    single,
                });
                if final_ {
                    let v = self.fsa.read_value(self.ord);
                    self.ord += 1;
                    return Some((self.cur.clone(), v));
                }
                continue;
            }
            let frame = self.stack.last_mut()?;
            // Drop the previous child's label before taking the next edge.
            self.cur.truncate(frame.base_len);
            if frame.remaining == 0 {
                self.stack.pop();
                continue;
            }
            let mut p = frame.p;
            let label = *b.get(p)?;
            p += 1;
            let delta = rd_uvarint(b, &mut p)?;
            if !frame.single {
                rd_uvarint(b, &mut p)?; // skip the count
            }
            frame.p = p;
            frame.remaining -= 1;
            let target = decode_target(frame.rel, delta)?;
            self.cur.push(label);
            self.to_enter = Some(target);
        }
    }
}
