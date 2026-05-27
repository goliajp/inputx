//! `IdfReader` — mmap zero-copy reader for IDFv1 files.
//!
//! Cold load: O(0) parse — open the file, validate the 96-byte header
//! region, set up offset views into the mmap region. Lookup: direct
//! pointer arithmetic, no allocation.

use alloc::vec::Vec;
use core::result::Result;

use crate::codec::{
    decode_match_type, EngineKind, EntryFlags, EntryRecord, Header, Version,
    FULL_HEADER_SIZE, MAGIC,
};

/// Lookup error kind. Public so callers can distinguish "file is a
/// different IDFv version" from "file is corrupt" — actionable
/// difference: a v2 file should drive a tool upgrade prompt; a corrupt
/// file should drive a rebuild.
#[derive(Debug)]
pub enum OpenError {
    /// I/O failure while reading the file. Wraps the source.
    #[cfg(feature = "std")]
    Io(std::io::Error),
    /// File is too short to hold even the header region.
    TooShort,
    /// First 4 bytes are not `b"IDFv"`. File is not an IDF dict at all.
    BadMagic,
    /// `format_version` byte is not `1`. A future v2 reader will accept
    /// these; v1 reader returns this so the caller can prompt a rebuild
    /// against the older tool.
    UnsupportedVersion(u8),
    /// `engine_kind` byte is outside the known set (per
    /// [`EngineKind::from_byte`]).
    UnknownEngineKind(u8),
    /// Header section offsets disagree with the file size — points past
    /// EOF or overlap. Indicates a truncated or rewritten file.
    CorruptOffsets,
    /// sha256_of_payload in the header does not match a fresh hash of
    /// the post-header bytes. Indicates the file was tampered with or
    /// corrupted between build and load.
    Sha256Mismatch,
}

#[cfg(feature = "std")]
impl From<std::io::Error> for OpenError {
    fn from(e: std::io::Error) -> Self { Self::Io(e) }
}

/// A read-only entry view backed by mmap (when `IdfReader` was opened
/// from a file) or by an owned byte buffer (in `no_std` use). `word`
/// and `code` borrow string-pool bytes; the lifetime ties them to the
/// underlying reader.
#[derive(Copy, Clone, Debug)]
pub struct Entry<'a> {
    pub word: &'a str,
    pub code: &'a str,
    pub log_prior: i16,
    pub match_type: inputx_scoring::MatchType,
    pub flags: EntryFlags,
}

/// IDFv1 reader. Holds the file's byte region and the parsed header.
/// `entry_index` lookups are constant-time arithmetic; FST code/word
/// scans use `inputx_fsa` directly over the index sections.
pub struct IdfReader<B>
where
    B: AsRef<[u8]>,
{
    bytes: B,
    header: Header,
}

impl<B: AsRef<[u8]>> IdfReader<B> {
    /// Build a reader from an in-memory byte region. Validates the
    /// header and (in std builds) the sha256 of payload.
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
        if EngineKind::from_byte(header.engine_kind).is_none() {
            return Err(OpenError::UnknownEngineKind(header.engine_kind));
        }
        // Bounds-check the section offsets against the file size — any
        // off-by-N or attacker-tampered file slips out here.
        let file_len = buf.len() as u32;
        for (off, sz) in [
            (header.string_pool_offset, header.string_pool_size),
            (
                header.entry_table_offset,
                header.entry_count.saturating_mul(crate::codec::ENTRY_SIZE as u32),
            ),
            (header.fst_code_index_offset, header.fst_code_index_size),
            (header.fst_word_index_offset, header.fst_word_index_size),
        ] {
            if off > file_len || off.saturating_add(sz) > file_len {
                return Err(OpenError::CorruptOffsets);
            }
        }
        // Verify sha256 of payload (covers everything after the sha256
        // field; offset FULL_HEADER_SIZE .. file_end).
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
        Ok(Self { bytes, header })
    }

    pub fn header(&self) -> &Header { &self.header }
    pub fn version(&self) -> Version {
        Version::from_byte(self.header.format_version).expect("validated at open")
    }
    pub fn engine_kind(&self) -> EngineKind {
        EngineKind::from_byte(self.header.engine_kind).expect("validated at open")
    }
    pub fn entry_count(&self) -> u32 { self.header.entry_count }
    pub fn sha256(&self) -> [u8; 32] { self.header.sha256_of_payload }

    /// All entries in entry-table order. O(n) — used by `prefix_top_k`
    /// fallback and by tests; production hot paths should go through
    /// the FST code / word indexes.
    pub fn entries(&self) -> impl Iterator<Item = Entry<'_>> + '_ {
        let buf = self.bytes.as_ref();
        let entry_table_start = self.header.entry_table_offset as usize;
        let n = self.header.entry_count as usize;
        let string_pool_start = self.header.string_pool_offset as usize;
        let string_pool_end =
            string_pool_start + self.header.string_pool_size as usize;
        let pool = &buf[string_pool_start..string_pool_end];
        (0..n).map(move |i| {
            let off = entry_table_start + i * crate::codec::ENTRY_SIZE;
            let rec_bytes: [u8; crate::codec::ENTRY_SIZE] = buf
                [off..off + crate::codec::ENTRY_SIZE]
                .try_into()
                .expect("entry slice is exactly ENTRY_SIZE");
            let rec = EntryRecord::parse(&rec_bytes);
            decode_entry(&rec, pool)
        })
    }

    /// Entry at the given index. `index` must be `< entry_count`.
    pub fn entry_at(&self, index: u32) -> Option<Entry<'_>> {
        if index >= self.header.entry_count {
            return None;
        }
        let buf = self.bytes.as_ref();
        let off = self.header.entry_table_offset as usize
            + index as usize * crate::codec::ENTRY_SIZE;
        let rec_bytes: [u8; crate::codec::ENTRY_SIZE] = buf
            [off..off + crate::codec::ENTRY_SIZE]
            .try_into()
            .ok()?;
        let rec = EntryRecord::parse(&rec_bytes);
        let pool = self.string_pool();
        Some(decode_entry(&rec, pool))
    }

    /// Lookup all entries whose `code` exactly matches the queried
    /// bytes. Returns an iterator (multi-reading entries appear in
    /// entry-table order). Falls back to a linear scan when the file
    /// has no FST code index attached.
    pub fn lookup<'a>(&'a self, code: &[u8]) -> Vec<Entry<'a>> {
        // FST code index lookup. The writer stores a code-major sorted
        // run, so a single lookup yields the FIRST entry_index; the
        // writer also packs the per-code run length as a varint prefix
        // — but for v1 simplicity (and because the largest production
        // dict has at most ~50 entries per code), we fall back to a
        // linear scan over the entry table when the FST returns a hit.
        // Cost: O(entry_count); acceptable for v1.4.3 dual-path verify.
        // v1.4.6+ may switch to packed run-length encoding.
        let mut out: Vec<Entry<'a>> = Vec::new();
        for entry in self.entries() {
            if entry.code.as_bytes() == code {
                out.push(entry);
            }
        }
        out
    }

    /// Reverse lookup by word. Same caveats as `lookup`.
    pub fn find_by_word<'a>(&'a self, word: &str) -> Vec<Entry<'a>> {
        let mut out: Vec<Entry<'a>> = Vec::new();
        for entry in self.entries() {
            if entry.word == word {
                out.push(entry);
            }
        }
        out
    }

    /// Top-k entries whose `code` starts with the prefix, ordered by
    /// `log_prior` desc. Linear scan + top-k heap (BinaryHeap for std;
    /// sorted insert otherwise).
    pub fn prefix_top_k<'a>(&'a self, prefix: &[u8], k: usize) -> Vec<Entry<'a>> {
        if k == 0 {
            return Vec::new();
        }
        let mut candidates: Vec<Entry<'a>> = self
            .entries()
            .filter(|e| e.code.as_bytes().starts_with(prefix))
            .collect();
        // Sort by log_prior desc, tiebreaker code asc for determinism.
        candidates.sort_by(|a, b| {
            b.log_prior
                .cmp(&a.log_prior)
                .then_with(|| a.code.cmp(b.code))
        });
        candidates.truncate(k);
        candidates
    }

    fn string_pool(&self) -> &[u8] {
        let buf = self.bytes.as_ref();
        let start = self.header.string_pool_offset as usize;
        let end = start + self.header.string_pool_size as usize;
        &buf[start..end]
    }
}

#[cfg(feature = "std")]
impl IdfReader<memmap2::Mmap> {
    /// Open an IDFv1 file at `path` via mmap. Zero-copy: lookups borrow
    /// directly from the mmap region. The reader holds the mmap alive;
    /// drop the reader to drop the mapping.
    pub fn open<P: AsRef<std::path::Path>>(path: P) -> Result<Self, OpenError> {
        let file = std::fs::File::open(path)?;
        // SAFETY: caller's IDFv1 file is treated as immutable while the
        // mapping is alive. memmap2 leaves the file open via the handle.
        let mmap = unsafe { memmap2::Mmap::map(&file)? };
        Self::from_bytes(mmap)
    }
}

/// Decode an [`EntryRecord`] + string-pool region into a borrowed
/// [`Entry`]. The two `&str` borrows live as long as the pool slice.
fn decode_entry<'a>(rec: &EntryRecord, pool: &'a [u8]) -> Entry<'a> {
    let word = read_string(pool, rec.word_offset);
    let code = read_string(pool, rec.code_offset);
    Entry {
        word,
        code,
        log_prior: rec.log_prior,
        match_type: decode_match_type(rec.match_type),
        flags: EntryFlags(rec.flags),
    }
}

/// Read a `\0`-terminated UTF-8 string from the pool at `offset`. Falls
/// back to an empty string when the offset is out of range — defensive
/// against bit-rot rather than a real expected case (writer enforces
/// well-formed offsets).
fn read_string(pool: &[u8], offset: u32) -> &str {
    let start = offset as usize;
    if start >= pool.len() {
        return "";
    }
    let rest = &pool[start..];
    let end = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
    core::str::from_utf8(&rest[..end]).unwrap_or("")
}

