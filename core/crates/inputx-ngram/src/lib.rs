//! `inputx-ngram` — n-gram log-probability lookup table for IME engines.
//!
//! Binary format **NGMv1**:
//!
//! ```text
//! +---------+---------------+--------------------+--------------------+
//! | Header  | String pool   | Triplet table      | FST ctx index      |
//! | 64 + 32 | varlen, pad8  | N × 8 B            | varlen (optional)  |
//! +---------+---------------+--------------------+--------------------+
//! ```
//!
//! - **Header** (64 B + 32 B sha trailer) — magic `b"NGMv"`,
//!   format_version (1), `max_n` (2 for bigram, 3+ trigram), entry_count,
//!   section offsets, sha256_of_payload.
//! - **String pool** — deduplicated UTF-8, null-terminated, u24 offsets.
//!   For bigram (max_n=2) each ctx is a single word; the pool stores it
//!   the same way as the next-word side.
//! - **Triplet table** — 8 B per entry: `ctx_offset` (u24) + `next_offset`
//!   (u24) + `log_prob` (i16). Sorted by (ctx_offset, log_prob desc, next).
//! - **FST ctx index** (optional, may be empty) — `inputx_fsa::Fsa` over
//!   ctx_bytes → first_triplet_index. v1.4.4 ships with empty section;
//!   reader falls back to a linear scan (acceptable: v1.4.4 is
//!   snapshot-tool work, runtime cement consumes this in v1.4.5+).
//!
//! Public API:
//!
//! ```rust,no_run
//! # use inputx_ngram::{NgramTable, NgramBuilder};
//! // Reader (zero-copy mmap):
//! let table = NgramTable::open("bigrams.ngm")?;
//! let lp = table.log_prob(&["今天"], "是");        // Some(i16 Q4) or None
//! let top = table.top_k(&["今天"], 10);              // Vec<(String, i16)>
//!
//! // Writer (snapshot tooling):
//! let mut b = NgramBuilder::new(2);                  // bigram
//! b.add(&["今天"], "是", 250);                       // Q4 log-prob ≈ +15.6 units
//! let sha = b.build("bigrams.ngm".as_ref())?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
extern crate std;

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::convert::TryInto;

/// Magic bytes for v1 NGM files.
pub const MAGIC: [u8; 4] = *b"NGMv";

/// Fixed header size (before sha256 trailer).
pub const HEADER_SIZE: usize = 64;

/// sha256 trailer length.
pub const SHA256_SIZE: usize = 32;

/// Total on-disk header region (header + sha trailer). Sections begin
/// at this offset.
pub const FULL_HEADER_SIZE: usize = HEADER_SIZE + SHA256_SIZE;

/// Fixed per-triplet size in bytes.
pub const TRIPLET_SIZE: usize = 8;

/// Q4 fixed-point scale (mirrors `inputx_scoring::Q4 = 16`). Reproduced
/// here so `inputx-ngram` stays zero-dep on `inputx-scoring`.
pub const Q4: i32 = 16;

/// Version of the NGM binary format.
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Version {
    V1 = 1,
}

impl Version {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            1 => Some(Self::V1),
            _ => None,
        }
    }
}

/// Open / read error.
#[derive(Debug)]
pub enum OpenError {
    #[cfg(feature = "std")]
    Io(std::io::Error),
    TooShort,
    BadMagic,
    UnsupportedVersion(u8),
    UnsupportedMaxN(u8),
    CorruptOffsets,
    Sha256Mismatch,
}

#[cfg(feature = "std")]
impl From<std::io::Error> for OpenError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// Header (host-byte view).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub magic: [u8; 4],
    pub format_version: u8,
    pub max_n: u8, // 2 for bigram, 3 trigram, ...
    pub reserved_a: [u8; 2],
    pub entry_count: u32,
    pub string_pool_offset: u32,
    pub string_pool_size: u32,
    pub triplet_table_offset: u32,
    pub triplet_table_size: u32,
    pub fst_ctx_index_offset: u32,
    pub fst_ctx_index_size: u32,
    pub reserved_b: [u8; 24],
    pub sha256_of_payload: [u8; 32],
}

impl Header {
    pub fn to_bytes(&self) -> [u8; HEADER_SIZE] {
        let mut buf = [0u8; HEADER_SIZE];
        buf[0..4].copy_from_slice(&self.magic);
        buf[4] = self.format_version;
        buf[5] = self.max_n;
        buf[6..8].copy_from_slice(&self.reserved_a);
        buf[8..12].copy_from_slice(&self.entry_count.to_le_bytes());
        buf[12..16].copy_from_slice(&self.string_pool_offset.to_le_bytes());
        buf[16..20].copy_from_slice(&self.string_pool_size.to_le_bytes());
        buf[20..24].copy_from_slice(&self.triplet_table_offset.to_le_bytes());
        buf[24..28].copy_from_slice(&self.triplet_table_size.to_le_bytes());
        buf[28..32].copy_from_slice(&self.fst_ctx_index_offset.to_le_bytes());
        buf[32..36].copy_from_slice(&self.fst_ctx_index_size.to_le_bytes());
        buf[36..60].copy_from_slice(&self.reserved_b);
        // Remaining 60..64 left as zero padding.
        buf
    }

    pub fn parse(buf: &[u8]) -> Option<Self> {
        if buf.len() < FULL_HEADER_SIZE {
            return None;
        }
        if buf[0..4] != MAGIC {
            return None;
        }
        let mut reserved_a = [0u8; 2];
        reserved_a.copy_from_slice(&buf[6..8]);
        let mut reserved_b = [0u8; 24];
        reserved_b.copy_from_slice(&buf[36..60]);
        let mut sha = [0u8; 32];
        sha.copy_from_slice(&buf[HEADER_SIZE..HEADER_SIZE + 32]);
        Some(Self {
            magic: MAGIC,
            format_version: buf[4],
            max_n: buf[5],
            reserved_a,
            entry_count: u32::from_le_bytes(buf[8..12].try_into().ok()?),
            string_pool_offset: u32::from_le_bytes(buf[12..16].try_into().ok()?),
            string_pool_size: u32::from_le_bytes(buf[16..20].try_into().ok()?),
            triplet_table_offset: u32::from_le_bytes(buf[20..24].try_into().ok()?),
            triplet_table_size: u32::from_le_bytes(buf[24..28].try_into().ok()?),
            fst_ctx_index_offset: u32::from_le_bytes(buf[28..32].try_into().ok()?),
            fst_ctx_index_size: u32::from_le_bytes(buf[32..36].try_into().ok()?),
            reserved_b,
            sha256_of_payload: sha,
        })
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct Triplet {
    ctx_offset: u32,  // u24 on disk
    next_offset: u32, // u24 on disk
    log_prob: i16,
}

impl Triplet {
    fn to_bytes(self) -> [u8; TRIPLET_SIZE] {
        let mut buf = [0u8; TRIPLET_SIZE];
        let co = self.ctx_offset.to_le_bytes();
        buf[0..3].copy_from_slice(&co[0..3]);
        let no = self.next_offset.to_le_bytes();
        buf[3..6].copy_from_slice(&no[0..3]);
        buf[6..8].copy_from_slice(&self.log_prob.to_le_bytes());
        buf
    }
    fn parse(buf: &[u8; TRIPLET_SIZE]) -> Self {
        let mut co = [0u8; 4];
        co[0..3].copy_from_slice(&buf[0..3]);
        let mut no = [0u8; 4];
        no[0..3].copy_from_slice(&buf[3..6]);
        Self {
            ctx_offset: u32::from_le_bytes(co),
            next_offset: u32::from_le_bytes(no),
            log_prob: i16::from_le_bytes([buf[6], buf[7]]),
        }
    }
}

/// The n-gram lookup table. Parameterized over the byte backing
/// (`Vec<u8>` for in-memory builds; `memmap2::Mmap` for zero-copy file
/// loads).
pub struct NgramTable<B: AsRef<[u8]>> {
    bytes: B,
    header: Header,
    /// Lazy in-memory ctx → triplet-range index. Built on first call
    /// to `log_prob` / `top_k`. Eliminates the O(N) linear scan that
    /// dominates per-keystroke cost in the IME runtime (~96% of main-
    /// thread time per `sample(1)` on a real session).
    ///
    /// Memory layout: `Vec<CtxIndexEntry>` sorted by ctx string content
    /// (resolved through `string_pool[ctx_offset..]`). Binary search by
    /// the query's ctx bytes. 12 bytes per entry; for a 50k-ctx bigram
    /// table that's ~600 KB plus one Vec alloc — vs the prior
    /// `HashMap<Vec<u8>, _>` design which churned 50k Vec<u8> allocs
    /// + HashMap bucket overhead for ~3 MB on the heap. Lookup is now
    /// O(log N) (~16 byte-compares for 50k entries) instead of O(1)
    /// hash, a few hundred ns worth of speed traded for ~2.4 MB heap.
    ///
    /// On-disk header reserves an FST ctx index section which the
    /// v1.4.4 snapshot tooling left empty (see header docs); until
    /// that's wired through the writer, the reader builds this
    /// in-memory equivalent at load-on-first-use.
    #[cfg(feature = "std")]
    ctx_index: std::sync::OnceLock<std::vec::Vec<CtxIndexEntry>>,
}

/// Compact ctx-index entry — 12 bytes, stored in a sorted `Vec`
/// resolved against the string pool via `ctx_offset` at compare time.
#[derive(Copy, Clone, Debug)]
#[cfg(feature = "std")]
struct CtxIndexEntry {
    /// Offset into `string_pool` to the null-terminated ctx string.
    ctx_offset: u32,
    /// Index of the first triplet for this ctx in the sorted
    /// triplet table.
    first_idx: u32,
    /// Number of consecutive triplets sharing this ctx.
    count: u32,
}

impl<B: AsRef<[u8]>> NgramTable<B> {
    /// Build a reader from an in-memory byte region. Validates magic,
    /// version, max_n, section bounds, sha256.
    pub fn from_bytes(bytes: B) -> Result<Self, OpenError> {
        let buf = bytes.as_ref();
        if buf.len() < FULL_HEADER_SIZE {
            return Err(OpenError::TooShort);
        }
        if buf[0..4] != MAGIC {
            return Err(OpenError::BadMagic);
        }
        let header = Header::parse(buf).ok_or(OpenError::BadMagic)?;
        if Version::from_byte(header.format_version).is_none() {
            return Err(OpenError::UnsupportedVersion(header.format_version));
        }
        if header.max_n < 2 || header.max_n > 4 {
            return Err(OpenError::UnsupportedMaxN(header.max_n));
        }
        let file_len = buf.len() as u32;
        for (off, sz) in [
            (header.string_pool_offset, header.string_pool_size),
            (header.triplet_table_offset, header.triplet_table_size),
            (header.fst_ctx_index_offset, header.fst_ctx_index_size),
        ] {
            if off > file_len || off.saturating_add(sz) > file_len {
                return Err(OpenError::CorruptOffsets);
            }
        }
        #[cfg(feature = "std")]
        {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(&buf[FULL_HEADER_SIZE..]);
            let got: [u8; 32] = hasher.finalize().into();
            if got != header.sha256_of_payload {
                return Err(OpenError::Sha256Mismatch);
            }
        }
        Ok(Self {
            bytes,
            header,
            #[cfg(feature = "std")]
            ctx_index: std::sync::OnceLock::new(),
        })
    }
    // `ctx_index` is now `OnceLock<Vec<CtxIndexEntry>>` per the field
    // declaration — the `OnceLock::new()` above initializes the new
    // type identically.

    pub fn header(&self) -> &Header {
        &self.header
    }
    pub fn max_n(&self) -> u8 {
        self.header.max_n
    }
    pub fn entry_count(&self) -> u32 {
        self.header.entry_count
    }
    pub fn sha256(&self) -> [u8; 32] {
        self.header.sha256_of_payload
    }

    /// Look up `log_prob(next | ctx)` for a context of length 1
    /// (bigram) or longer (max_n=3 trigram, …). Returns `None` if the
    /// context or the (ctx, next) pair is missing from the table.
    pub fn log_prob(&self, ctx: &[&str], next: &str) -> Option<i16> {
        let ctx_blob = encode_ctx(ctx);
        #[cfg(feature = "std")]
        {
            let entry = self.find_ctx(ctx_blob.as_bytes())?;
            let pool = self.string_pool();
            let next_bytes = next.as_bytes();
            for i in entry.first_idx..entry.first_idx + entry.count {
                let t = self.triplet_at(i as usize);
                if read_string_bytes(pool, t.next_offset) == next_bytes {
                    return Some(t.log_prob);
                }
            }
            None
        }
        #[cfg(not(feature = "std"))]
        {
            for t in self.triplets() {
                let stored_ctx = read_string(self.string_pool(), t.ctx_offset);
                let stored_next = read_string(self.string_pool(), t.next_offset);
                if stored_ctx == ctx_blob && stored_next == next {
                    return Some(t.log_prob);
                }
            }
            None
        }
    }

    /// Top-`k` next-word continuations for a given context, ordered by
    /// `log_prob` desc (tiebreaker: next-word ascending for determinism).
    pub fn top_k(&self, ctx: &[&str], k: usize) -> Vec<(String, i16)> {
        if k == 0 {
            return Vec::new();
        }
        let ctx_blob = encode_ctx(ctx);
        let mut all: Vec<(String, i16)> = Vec::new();
        #[cfg(feature = "std")]
        {
            if let Some(entry) = self.find_ctx(ctx_blob.as_bytes()) {
                let pool = self.string_pool();
                for i in entry.first_idx..entry.first_idx + entry.count {
                    let t = self.triplet_at(i as usize);
                    let stored_next = read_string(pool, t.next_offset).to_string();
                    all.push((stored_next, t.log_prob));
                }
            }
        }
        #[cfg(not(feature = "std"))]
        {
            for t in self.triplets() {
                let stored_ctx = read_string(self.string_pool(), t.ctx_offset);
                if stored_ctx == ctx_blob {
                    let stored_next = read_string(self.string_pool(), t.next_offset).to_string();
                    all.push((stored_next, t.log_prob));
                }
            }
        }
        all.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        all.truncate(k);
        all
    }

    /// Parse the `i`-th triplet directly (no iterator overhead). Used
    /// by the hot path once `ctx_index()` has located the contiguous
    /// triplet range for a given ctx.
    fn triplet_at(&self, i: usize) -> Triplet {
        let buf = self.bytes.as_ref();
        let off = self.header.triplet_table_offset as usize + i * TRIPLET_SIZE;
        let bytes: [u8; TRIPLET_SIZE] = buf[off..off + TRIPLET_SIZE]
            .try_into()
            .expect("triplet slice");
        Triplet::parse(&bytes)
    }

    /// Lazy ctx → (first_triplet_idx, count) index. Built on first call;
    /// subsequent calls find the entry via binary search.
    ///
    /// Triplets on disk are sorted by (ctx_offset, log_prob desc, next),
    /// so same-ctx_offset triplets form contiguous runs. The builder
    /// walks the triplet table once batching by ctx_offset, then sorts
    /// the resulting entries by ctx string content (resolved through
    /// the string pool) so binary search by ctx bytes works.
    ///
    /// Construction cost: O(N) walk + O(M log M) sort, where N =
    /// entry_count and M = unique ctxs. Amortized across all lookups
    /// for the lifetime of the table (mmap'd once per process).
    #[cfg(feature = "std")]
    fn ctx_index(&self) -> &[CtxIndexEntry] {
        self.ctx_index.get_or_init(|| {
            let pool = self.string_pool();
            let n = self.header.entry_count;
            if n == 0 {
                return std::vec::Vec::new();
            }
            let mut entries: std::vec::Vec<CtxIndexEntry> = std::vec::Vec::new();
            let mut current_offset: u32 = u32::MAX;
            let mut current_start: u32 = 0;
            for i in 0..n {
                let t = self.triplet_at(i as usize);
                if t.ctx_offset != current_offset {
                    if current_offset != u32::MAX {
                        let count = i - current_start;
                        // Skip the (rare) empty-ctx case — pool offset
                        // pointing at a leading \0 byte.
                        if !read_string_bytes(pool, current_offset).is_empty() {
                            entries.push(CtxIndexEntry {
                                ctx_offset: current_offset,
                                first_idx: current_start,
                                count,
                            });
                        }
                    }
                    current_offset = t.ctx_offset;
                    current_start = i;
                }
            }
            let count = n - current_start;
            if !read_string_bytes(pool, current_offset).is_empty() {
                entries.push(CtxIndexEntry {
                    ctx_offset: current_offset,
                    first_idx: current_start,
                    count,
                });
            }
            // Sort by ctx string content so binary search by ctx bytes
            // (in `find_ctx`) is correct.
            entries.sort_by(|a, b| {
                let pa = read_string_bytes(pool, a.ctx_offset);
                let pb = read_string_bytes(pool, b.ctx_offset);
                pa.cmp(pb)
            });
            entries.shrink_to_fit();
            entries
        })
    }

    /// Resolve a ctx-bytes query to its triplet range. Prefers the
    /// FST embedded in the NGM file (zero heap allocation, O(|ctx|)
    /// traversal of mmap'd state automata). Falls back to the
    /// in-memory sorted-vec binary search when the FST section is
    /// empty — only happens with NGM files generated before the v1.6
    /// perf push.
    #[cfg(feature = "std")]
    fn find_ctx(&self, target: &[u8]) -> Option<CtxIndexEntry> {
        // Fast path: embedded FST. Lookup walks the mmap'd FSA bytes
        // directly — no heap allocation.
        if self.header.fst_ctx_index_size > 0 {
            let buf = self.bytes.as_ref();
            let off = self.header.fst_ctx_index_offset as usize;
            let sz = self.header.fst_ctx_index_size as usize;
            if let Ok(fsa) = inputx_fsa::Fsa::new(&buf[off..off + sz]) {
                let value = fsa.get(target)?;
                let first_idx = (value & 0xFFFF_FFFF) as u32;
                let count = (value >> 32) as u32;
                return Some(CtxIndexEntry {
                    ctx_offset: 0, // unused on the FST path
                    first_idx,
                    count,
                });
            }
        }
        // Fallback: in-memory sorted Vec built at first use.
        let entries = self.ctx_index();
        let pool = self.string_pool();
        let idx = entries
            .binary_search_by(|e| {
                let stored = read_string_bytes(pool, e.ctx_offset);
                stored.cmp(target)
            })
            .ok()?;
        Some(entries[idx])
    }

    fn string_pool(&self) -> &[u8] {
        let buf = self.bytes.as_ref();
        let s = self.header.string_pool_offset as usize;
        let e = s + self.header.string_pool_size as usize;
        &buf[s..e]
    }

    #[cfg_attr(feature = "std", allow(dead_code))]
    fn triplets(&self) -> impl Iterator<Item = Triplet> + '_ {
        let buf = self.bytes.as_ref();
        let off = self.header.triplet_table_offset as usize;
        let n = self.header.entry_count as usize;
        (0..n).map(move |i| {
            let start = off + i * TRIPLET_SIZE;
            let bytes: [u8; TRIPLET_SIZE] = buf[start..start + TRIPLET_SIZE]
                .try_into()
                .expect("triplet slice");
            Triplet::parse(&bytes)
        })
    }
}

#[cfg(feature = "std")]
impl NgramTable<memmap2::Mmap> {
    /// Open an NGMv1 file at `path` via mmap (zero-copy).
    pub fn open<P: AsRef<std::path::Path>>(path: P) -> Result<Self, OpenError> {
        let file = std::fs::File::open(path)?;
        let mmap = unsafe { memmap2::Mmap::map(&file)? };
        Self::from_bytes(mmap)
    }
}

fn read_string(pool: &[u8], offset: u32) -> &str {
    let s = offset as usize;
    if s >= pool.len() {
        return "";
    }
    let rest = &pool[s..];
    let end = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
    core::str::from_utf8(&rest[..end]).unwrap_or("")
}

/// Hot-path string slice without UTF-8 validation. The pool was
/// written as deduplicated, null-terminated UTF-8 by `NgramBuilder`;
/// integrity is already vouched for by the file-level sha256 trailer
/// (checked once in `from_bytes`), so per-call `from_utf8` here is
/// pure overhead. Returns raw bytes for byte-equality comparison
/// against the query (which is the only operation in `log_prob`'s
/// hot loop). On sample profiles this single change removes ~25% of
/// per-keystroke main-thread cost.
fn read_string_bytes(pool: &[u8], offset: u32) -> &[u8] {
    let s = offset as usize;
    if s >= pool.len() {
        return &[];
    }
    let rest = &pool[s..];
    let end = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
    &rest[..end]
}

/// Encode a context (1+ words) as a single string-pool blob. For
/// bigram (single-word ctx) this is just the word; for trigram+ the
/// ctx words are joined by a `\u{1F}` (Unit Separator) byte so the
/// blob round-trips through the pool's `\0`-terminator semantics.
fn encode_ctx(ctx: &[&str]) -> String {
    let mut s = String::new();
    for (i, w) in ctx.iter().enumerate() {
        if i > 0 {
            s.push('\u{1F}');
        }
        s.push_str(w);
    }
    s
}

// ─── Writer (std-only) ────────────────────────────────────────────────

#[cfg(feature = "std")]
pub use writer::NgramBuilder;

#[cfg(feature = "std")]
mod writer {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::collections::HashMap;
    use std::io::{self, Write};
    use std::path::Path;

    /// Deterministic NGMv1 writer.
    pub struct NgramBuilder {
        max_n: u8,
        entries: Vec<(String, String, i16)>, // (ctx_blob, next, log_prob)
    }

    impl NgramBuilder {
        pub fn new(max_n: u8) -> Self {
            assert!(
                (2..=4).contains(&max_n),
                "max_n must be in 2..=4 (got {max_n})",
            );
            Self {
                max_n,
                entries: Vec::new(),
            }
        }

        /// Queue one (ctx, next, log_prob) triplet. `ctx.len()` must be
        /// `1..=max_n - 1` (e.g. for bigram pass a 1-slice).
        pub fn add(&mut self, ctx: &[&str], next: &str, log_prob: i16) {
            assert!(
                !ctx.is_empty() && ctx.len() <= (self.max_n - 1) as usize,
                "ctx length {} outside 1..={} for max_n={}",
                ctx.len(),
                self.max_n - 1,
                self.max_n,
            );
            self.entries
                .push((encode_ctx(ctx), next.to_string(), log_prob));
        }

        pub fn pending_count(&self) -> usize {
            self.entries.len()
        }

        pub fn build(mut self, path: &Path) -> io::Result<[u8; 32]> {
            // Dedup + sort for determinism.
            self.entries.sort_by(|a, b| {
                a.0.cmp(&b.0)
                    .then_with(|| b.2.cmp(&a.2)) // log_prob desc within ctx
                    .then_with(|| a.1.cmp(&b.1))
            });
            self.entries.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
            let entry_count = self.entries.len() as u32;

            // String pool — sorted unique.
            let mut unique: Vec<&str> = Vec::with_capacity(self.entries.len() * 2);
            for (ctx, next, _) in &self.entries {
                unique.push(ctx.as_str());
                unique.push(next.as_str());
            }
            unique.sort_unstable();
            unique.dedup();
            let mut pool_bytes: Vec<u8> = Vec::new();
            let mut pool_offsets: HashMap<&str, u32> = HashMap::with_capacity(unique.len());
            for s in &unique {
                let off = pool_bytes.len() as u32;
                if off > 0xFF_FFFF {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "string pool exceeds u24 addressable range (16 MiB)",
                    ));
                }
                pool_offsets.insert(s, off);
                pool_bytes.extend_from_slice(s.as_bytes());
                pool_bytes.push(0);
            }
            let string_pool_size = pool_bytes.len() as u32;
            while pool_bytes.len() % 8 != 0 {
                pool_bytes.push(0);
            }

            // Triplet table.
            let mut triplet_bytes: Vec<u8> = Vec::with_capacity(self.entries.len() * TRIPLET_SIZE);
            for (ctx, next, log_prob) in &self.entries {
                let t = Triplet {
                    ctx_offset: pool_offsets[ctx.as_str()],
                    next_offset: pool_offsets[next.as_str()],
                    log_prob: *log_prob,
                };
                triplet_bytes.extend_from_slice(&t.to_bytes());
            }

            // FST ctx index — populated since the v1.6.x perf push.
            // Maps ctx_bytes → packed `(first_idx u32 in low 32 bits |
            // count u32 in high 32 bits)`. Reader uses this for
            // zero-alloc O(|ctx|) lookups straight from the mmap'd
            // region; the in-memory ctx-index path stays only as a
            // backward-compat fallback for older NGM files that left
            // this section empty.
            //
            // The entries are already sorted by (ctx asc, log_prob
            // desc, next asc), so contiguous same-ctx runs give us
            // (first_idx, count) directly without a second pass.
            let fst_ctx_index: Vec<u8> = {
                use inputx_fsa::Builder as FsaBuilder;
                let mut fsa = FsaBuilder::new();
                let mut current_ctx: Option<&str> = None;
                let mut current_start: u32 = 0;
                for (i, (ctx, _, _)) in self.entries.iter().enumerate() {
                    let i = i as u32;
                    let same = matches!(current_ctx, Some(c) if c == ctx.as_str());
                    if !same {
                        if let Some(prev) = current_ctx {
                            let count = i - current_start;
                            let value = (current_start as u64) | ((count as u64) << 32);
                            fsa.insert(prev.as_bytes(), value);
                        }
                        current_ctx = Some(ctx.as_str());
                        current_start = i;
                    }
                }
                if let Some(prev) = current_ctx {
                    let count = entry_count - current_start;
                    let value = (current_start as u64) | ((count as u64) << 32);
                    fsa.insert(prev.as_bytes(), value);
                }
                fsa.finish()
            };
            // No trailing padding: FST is the last section in the
            // file, and `Fsa::new` (the reader) reads `value_count *
            // value_width` from the END of its input slice, so any
            // trailing zeros would be mis-interpreted as values.

            // Layout.
            let string_pool_offset = FULL_HEADER_SIZE as u32;
            let triplet_table_offset = string_pool_offset + pool_bytes.len() as u32;
            let fst_ctx_index_offset = triplet_table_offset + triplet_bytes.len() as u32;

            let header = Header {
                magic: MAGIC,
                format_version: 1,
                max_n: self.max_n,
                reserved_a: [0; 2],
                entry_count,
                string_pool_offset,
                string_pool_size,
                triplet_table_offset,
                triplet_table_size: triplet_bytes.len() as u32,
                fst_ctx_index_offset,
                fst_ctx_index_size: fst_ctx_index.len() as u32,
                reserved_b: [0; 24],
                sha256_of_payload: [0; 32],
            };
            let header_bytes = header.to_bytes();

            // Payload sha.
            let mut hasher = Sha256::new();
            hasher.update(&pool_bytes);
            hasher.update(&triplet_bytes);
            hasher.update(&fst_ctx_index);
            let sha: [u8; 32] = hasher.finalize().into();

            // Atomic write.
            let tmp = path.with_extension("ngm.tmp");
            {
                let mut f = std::fs::File::create(&tmp)?;
                f.write_all(&header_bytes)?;
                f.write_all(&sha)?;
                f.write_all(&pool_bytes)?;
                f.write_all(&triplet_bytes)?;
                f.write_all(&fst_ctx_index)?;
                f.sync_all()?;
            }
            std::fs::rename(&tmp, path)?;
            Ok(sha)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "std")]
    use tempfile::tempdir;

    #[test]
    fn header_size_constants() {
        assert_eq!(HEADER_SIZE, 64);
        assert_eq!(SHA256_SIZE, 32);
        assert_eq!(FULL_HEADER_SIZE, 96);
        assert_eq!(TRIPLET_SIZE, 8);
    }

    #[test]
    fn header_round_trip() {
        let h = Header {
            magic: MAGIC,
            format_version: 1,
            max_n: 2,
            reserved_a: [0; 2],
            entry_count: 123,
            string_pool_offset: 96,
            string_pool_size: 1024,
            triplet_table_offset: 96 + 1024,
            triplet_table_size: 123 * 8,
            fst_ctx_index_offset: 96 + 1024 + 123 * 8,
            fst_ctx_index_size: 0,
            reserved_b: [0; 24],
            sha256_of_payload: [0x5a; 32],
        };
        let bytes = h.to_bytes();
        let mut full = [0u8; FULL_HEADER_SIZE];
        full[..HEADER_SIZE].copy_from_slice(&bytes);
        full[HEADER_SIZE..].copy_from_slice(&h.sha256_of_payload);
        let h2 = Header::parse(&full).expect("parse");
        assert_eq!(h2, h);
    }

    #[test]
    fn header_rejects_wrong_magic() {
        let mut buf = [0u8; FULL_HEADER_SIZE];
        buf[0..4].copy_from_slice(b"XXXX");
        assert!(Header::parse(&buf).is_none());
    }

    #[test]
    fn triplet_round_trip_preserves_fields() {
        let t = Triplet {
            ctx_offset: 0xFE_DCBA,
            next_offset: 0x01_2345,
            log_prob: -300,
        };
        let bytes = t.to_bytes();
        let t2 = Triplet::parse(&bytes);
        assert_eq!(t2, t);
    }

    #[test]
    fn encode_ctx_joins_with_unit_separator() {
        assert_eq!(encode_ctx(&["今天"]), "今天");
        assert_eq!(encode_ctx(&["今天", "是"]), "今天\u{1F}是");
    }

    #[cfg(feature = "std")]
    #[test]
    fn bigram_round_trip_recovers_log_prob() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("bg.ngm");
        let mut b = NgramBuilder::new(2);
        b.add(&["今天"], "是", 250);
        b.add(&["今天"], "的", 200);
        b.add(&["我们"], "的", 300);
        b.add(&["我们"], "是", 180);
        b.build(&path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let t = NgramTable::from_bytes(bytes).unwrap();
        assert_eq!(t.max_n(), 2);
        assert_eq!(t.entry_count(), 4);
        assert_eq!(t.log_prob(&["今天"], "是"), Some(250));
        assert_eq!(t.log_prob(&["今天"], "的"), Some(200));
        assert_eq!(t.log_prob(&["我们"], "的"), Some(300));
        assert_eq!(t.log_prob(&["unknown"], "x"), None);
    }

    #[cfg(feature = "std")]
    #[test]
    fn top_k_sorted_desc_with_lex_tiebreaker() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("tk.ngm");
        let mut b = NgramBuilder::new(2);
        b.add(&["今天"], "是", 250);
        b.add(&["今天"], "的", 200);
        b.add(&["今天"], "我", 250); // tie with 是; lex 我 > 是 so 是 first
        b.add(&["今天"], "了", 150);
        b.build(&path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let t = NgramTable::from_bytes(bytes).unwrap();
        let top = t.top_k(&["今天"], 3);
        assert_eq!(top.len(), 3);
        assert_eq!(top[0], ("我".to_string(), 250)); // lex: 我 < 是
        assert_eq!(top[1], ("是".to_string(), 250));
        assert_eq!(top[2], ("的".to_string(), 200));
    }

    #[cfg(feature = "std")]
    #[test]
    fn two_builds_same_input_produce_identical_sha256() {
        let dir = tempdir().unwrap();
        let p1 = dir.path().join("a.ngm");
        let p2 = dir.path().join("b.ngm");
        let mk = || {
            let mut b = NgramBuilder::new(2);
            for (ctx, next, lp) in [
                ("今天", "是", 250),
                ("今天", "的", 200),
                ("我们", "的", 300),
                ("我们", "是", 180),
                ("一个", "新", 220),
            ] {
                b.add(&[ctx], next, lp);
            }
            b
        };
        let s1 = mk().build(&p1).unwrap();
        let s2 = mk().build(&p2).unwrap();
        assert_eq!(s1, s2, "deterministic build");
        assert_eq!(std::fs::read(&p1).unwrap(), std::fs::read(&p2).unwrap());
    }

    #[cfg(feature = "std")]
    #[test]
    fn duplicate_ctx_next_pair_deduped() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("d.ngm");
        let mut b = NgramBuilder::new(2);
        b.add(&["今天"], "是", 250);
        b.add(&["今天"], "是", 999); // dedup keeps higher log_prob (sort puts 999 first)
        b.build(&path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let t = NgramTable::from_bytes(bytes).unwrap();
        assert_eq!(t.entry_count(), 1);
        assert_eq!(t.log_prob(&["今天"], "是"), Some(999));
    }

    #[cfg(feature = "std")]
    #[test]
    fn trigram_max_n_3_round_trip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("tg.ngm");
        let mut b = NgramBuilder::new(3);
        b.add(&["今天", "我"], "去", 280);
        b.add(&["今天", "你"], "好", 260);
        b.add(&["昨天"], "是", 200); // bigram entry in a trigram file (max_n=3 allows ctx 1..=2)
        b.build(&path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let t = NgramTable::from_bytes(bytes).unwrap();
        assert_eq!(t.max_n(), 3);
        assert_eq!(t.entry_count(), 3);
        assert_eq!(t.log_prob(&["今天", "我"], "去"), Some(280));
        assert_eq!(t.log_prob(&["今天", "你"], "好"), Some(260));
        assert_eq!(t.log_prob(&["昨天"], "是"), Some(200));
        assert_eq!(t.log_prob(&["今天"], "去"), None); // wrong ctx length
    }

    #[cfg(feature = "std")]
    #[test]
    fn sha256_mismatch_rejected() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("tamper.ngm");
        let mut b = NgramBuilder::new(2);
        b.add(&["x"], "y", 100);
        b.build(&path).unwrap();
        let mut bytes = std::fs::read(&path).unwrap();
        let target = bytes.len() - 5;
        bytes[target] ^= 0xFF;
        assert!(matches!(
            NgramTable::from_bytes(bytes),
            Err(OpenError::Sha256Mismatch)
        ));
    }

    #[cfg(feature = "std")]
    #[test]
    fn unsupported_max_n_rejected() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("badn.ngm");
        let b = NgramBuilder::new(2);
        b.build(&path).unwrap();
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[5] = 9; // max_n
        // Tamper invalidates sha; re-sign over post-header payload.
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(&bytes[FULL_HEADER_SIZE..]);
        let sha: [u8; 32] = hasher.finalize().into();
        bytes[HEADER_SIZE..HEADER_SIZE + 32].copy_from_slice(&sha);
        let r = NgramTable::from_bytes(bytes);
        assert!(matches!(r, Err(OpenError::UnsupportedMaxN(9))));
    }
}
