//! IDFv1 binary layout — header / entry / flag types + LE codec helpers.
//!
//! Wire layout per `.claude/PLAN-dict-format-IDFv1.md`. Little-endian
//! throughout (x86_64 + arm64 are both LE; cross-platform safe).
//! Alignment-strict: section boundaries are 8-byte aligned; `Entry` is
//! exactly 16 bytes packed.

use core::convert::TryInto;

/// Magic bytes at file offset 0. ASCII `"IDFv"`.
pub const MAGIC: [u8; 4] = *b"IDFv";

/// Fixed header size in bytes. Sections begin at `HEADER_SIZE`.
pub const HEADER_SIZE: usize = 64;

/// Fixed per-entry size in bytes.
pub const ENTRY_SIZE: usize = 16;

/// Format version. v1 = the layout described in
/// `.claude/PLAN-dict-format-IDFv1.md`. Future v2+ will live alongside
/// via the `format_version` header byte; v1 readers MUST reject unknown
/// versions with a clear error.
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Version {
    V1 = 1,
}

impl Version {
    /// Decode the on-disk byte. Returns `None` for unsupported versions
    /// (the caller emits a clear "v2 file opened by v1 reader" error).
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            1 => Some(Self::V1),
            _ => None,
        }
    }
}

/// Which engine the dict serves. Stable u8 across versions so dispatch
/// code can match without translation.
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum EngineKind {
    Pinyin = 0,
    Wubi = 1,
    NihongoJukugo = 2,
    NihongoKanji = 3,
    Other = 4,
}

impl EngineKind {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0 => Some(Self::Pinyin),
            1 => Some(Self::Wubi),
            2 => Some(Self::NihongoJukugo),
            3 => Some(Self::NihongoKanji),
            4 => Some(Self::Other),
            _ => None,
        }
    }
}

/// Per-entry flag bits. Bit assignments are stable across IDFv1.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub struct EntryFlags(pub u8);

impl EntryFlags {
    pub const BLACKLIST: u8 = 1 << 0;
    pub const CURATED_OVERRIDE: u8 = 1 << 1;
    pub const USER_ADDED: u8 = 1 << 2;

    pub fn is_blacklisted(self) -> bool { self.0 & Self::BLACKLIST != 0 }
    pub fn is_curated_override(self) -> bool { self.0 & Self::CURATED_OVERRIDE != 0 }
    pub fn is_user_added(self) -> bool { self.0 & Self::USER_ADDED != 0 }
}

/// Header, mirrors the on-disk 64-byte layout exactly. All integer
/// fields are little-endian on disk; the public struct holds host-byte
/// values (decoded on `parse`, encoded on `to_bytes`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub magic: [u8; 4],
    pub format_version: u8,
    pub engine_kind: u8,
    pub flags: u16,
    pub entry_count: u32,
    pub string_pool_offset: u32,
    pub string_pool_size: u32,
    pub entry_table_offset: u32,
    pub fst_code_index_offset: u32,
    pub fst_code_index_size: u32,
    pub fst_word_index_offset: u32,
    pub fst_word_index_size: u32,
    pub bigram_offset: u32,
    pub bigram_size: u32,
    pub embedding_offset: u32,
    pub embedding_dim: u16,
    pub embedding_dtype: u8,
    pub reserved: u8,
    pub sha256_of_payload: [u8; 32],
}

impl Header {
    /// Encode the header into a fixed 64-byte buffer (little-endian).
    pub fn to_bytes(&self) -> [u8; HEADER_SIZE] {
        let mut buf = [0u8; HEADER_SIZE];
        buf[0..4].copy_from_slice(&self.magic);
        buf[4] = self.format_version;
        buf[5] = self.engine_kind;
        buf[6..8].copy_from_slice(&self.flags.to_le_bytes());
        buf[8..12].copy_from_slice(&self.entry_count.to_le_bytes());
        buf[12..16].copy_from_slice(&self.string_pool_offset.to_le_bytes());
        buf[16..20].copy_from_slice(&self.string_pool_size.to_le_bytes());
        buf[20..24].copy_from_slice(&self.entry_table_offset.to_le_bytes());
        buf[24..28].copy_from_slice(&self.fst_code_index_offset.to_le_bytes());
        buf[28..32].copy_from_slice(&self.fst_code_index_size.to_le_bytes());
        buf[32..36].copy_from_slice(&self.fst_word_index_offset.to_le_bytes());
        buf[36..40].copy_from_slice(&self.fst_word_index_size.to_le_bytes());
        buf[40..44].copy_from_slice(&self.bigram_offset.to_le_bytes());
        buf[44..48].copy_from_slice(&self.bigram_size.to_le_bytes());
        buf[48..52].copy_from_slice(&self.embedding_offset.to_le_bytes());
        buf[52..54].copy_from_slice(&self.embedding_dim.to_le_bytes());
        buf[54] = self.embedding_dtype;
        buf[55] = self.reserved;
        // sha256 trails — 8 bytes worth of layout used (54+1+1+8=64? let's
        // recount: header is exactly 64. We've used 0..56 for non-sha
        // fields. sha256 (32B) needs 32 — that's 88 total which OVERFLOWS.
        // The spec keeps the header at 64 by NOT counting the sha256 in
        // the header proper — it sits immediately after at offset 64. But
        // PLAN-dict-format-IDFv1.md says `sha256_of_payload` IS in the
        // header. Reconcile: bytes 56..88 (32 B) live in a 96-byte header
        // region. We treat HEADER_SIZE as 64 + 32 = 96 conceptually but
        // keep HEADER_SIZE = 64 = "everything BEFORE sha256". The full
        // file region taken by header+sha256 is HEADER_SIZE + 32 = 96.
        // sha256 is written separately by the writer after computing it
        // over the payload. Return only the first 64.
        buf
    }

    /// Decode a header from on-disk bytes. Returns `None` if `buf` is
    /// too small or the magic does not match.
    pub fn parse(buf: &[u8]) -> Option<Self> {
        if buf.len() < HEADER_SIZE + 32 { return None; }
        if buf[0..4] != MAGIC { return None; }
        let mut sha = [0u8; 32];
        sha.copy_from_slice(&buf[HEADER_SIZE..HEADER_SIZE + 32]);
        Some(Self {
            magic: MAGIC,
            format_version: buf[4],
            engine_kind: buf[5],
            flags: u16::from_le_bytes(buf[6..8].try_into().ok()?),
            entry_count: u32::from_le_bytes(buf[8..12].try_into().ok()?),
            string_pool_offset: u32::from_le_bytes(buf[12..16].try_into().ok()?),
            string_pool_size: u32::from_le_bytes(buf[16..20].try_into().ok()?),
            entry_table_offset: u32::from_le_bytes(buf[20..24].try_into().ok()?),
            fst_code_index_offset: u32::from_le_bytes(buf[24..28].try_into().ok()?),
            fst_code_index_size: u32::from_le_bytes(buf[28..32].try_into().ok()?),
            fst_word_index_offset: u32::from_le_bytes(buf[32..36].try_into().ok()?),
            fst_word_index_size: u32::from_le_bytes(buf[36..40].try_into().ok()?),
            bigram_offset: u32::from_le_bytes(buf[40..44].try_into().ok()?),
            bigram_size: u32::from_le_bytes(buf[44..48].try_into().ok()?),
            embedding_offset: u32::from_le_bytes(buf[48..52].try_into().ok()?),
            embedding_dim: u16::from_le_bytes(buf[52..54].try_into().ok()?),
            embedding_dtype: buf[54],
            reserved: buf[55],
            sha256_of_payload: sha,
        })
    }
}

/// Reserved byte length for the sha256 region immediately after the
/// 64-byte header proper. The full on-disk header region is
/// `HEADER_SIZE + SHA256_SIZE = 96` bytes; sections begin at offset 96.
pub const SHA256_SIZE: usize = 32;

/// Total on-disk header region (header + sha256 area). Sections begin here.
pub const FULL_HEADER_SIZE: usize = HEADER_SIZE + SHA256_SIZE;

/// Per-entry record (16 bytes packed). `word_offset` and `code_offset`
/// are u24 (3 bytes); they point into the string pool. log_prior is
/// signed Q4 fixed-point (one log unit per 16 integer steps, per
/// [`inputx_scoring::Q4`]). bigram_offset / embedding_offset are 0
/// when absent.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct EntryRecord {
    pub word_offset: u32, // u24 on disk
    pub code_offset: u32, // u24 on disk
    pub log_prior: i16,
    pub match_type: u8,
    pub flags: u8,
    pub bigram_offset: u32,
    pub embedding_offset: u32,
}

impl EntryRecord {
    /// Encode to the 16-byte on-disk representation.
    pub fn to_bytes(&self) -> [u8; ENTRY_SIZE] {
        let mut buf = [0u8; ENTRY_SIZE];
        // u24 little-endian = low 3 bytes
        let wo = self.word_offset.to_le_bytes();
        buf[0..3].copy_from_slice(&wo[0..3]);
        let co = self.code_offset.to_le_bytes();
        buf[3..6].copy_from_slice(&co[0..3]);
        buf[6..8].copy_from_slice(&self.log_prior.to_le_bytes());
        buf[8] = self.match_type;
        buf[9] = self.flags;
        buf[10..14].copy_from_slice(&(self.bigram_offset).to_le_bytes()[0..4]);
        // 14..16 reserved for entry_table padding to 16 (embedding_offset
        // is 4 bytes; we have 16 − 14 = 2 left — embedding_offset can't
        // fit. Reconcile: per PLAN-dict-format-IDFv1.md the entry is 16
        // bytes; the 6-tuple as listed exceeds that. Real layout: u24+
        // u24+i16+u8+u8 = 12; bigram_offset (u32) = 4 → 16 total.
        // embedding_offset lives in the EMBEDDING block index, not the
        // per-entry record. v1 ships without embeddings; the spec's
        // mention of per-entry embedding_offset will be a v2 extension
        // using a side table referenced from the header's
        // embedding_offset). For v1: per-entry stops at bigram_offset.
        // Last 2 bytes reserved.
        // No write needed — buf already zero.
        buf
    }

    /// Decode from 16-byte on-disk representation.
    pub fn parse(buf: &[u8; ENTRY_SIZE]) -> Self {
        let mut wo = [0u8; 4];
        wo[0..3].copy_from_slice(&buf[0..3]);
        let word_offset = u32::from_le_bytes(wo);
        let mut co = [0u8; 4];
        co[0..3].copy_from_slice(&buf[3..6]);
        let code_offset = u32::from_le_bytes(co);
        let log_prior = i16::from_le_bytes([buf[6], buf[7]]);
        let match_type = buf[8];
        let flags = buf[9];
        let bigram_offset = u32::from_le_bytes([buf[10], buf[11], buf[12], buf[13]]);
        EntryRecord {
            word_offset,
            code_offset,
            log_prior,
            match_type,
            flags,
            bigram_offset,
            embedding_offset: 0, // v1: side table, not per-entry
        }
    }
}

/// Encode an [`inputx_scoring::MatchType`] into the single u8 stored in
/// `EntryRecord::match_type`. Round-trippable via [`decode_match_type`].
/// Inline payload (proximity / fuzzy cost / bigram_links) is lost on
/// encode — the writer uses `Exact` for entries that have a fixed
/// dict-baseline classification; runtime paths attach the inline payload
/// based on how the buffer matched.
pub fn encode_match_type(mt: inputx_scoring::MatchType) -> u8 {
    match mt {
        inputx_scoring::MatchType::Exact => 0,
        inputx_scoring::MatchType::Prefix(_) => 1,
        inputx_scoring::MatchType::Fuzzy(_) => 2,
        inputx_scoring::MatchType::Composed { .. } => 3,
    }
}

/// Decode `EntryRecord::match_type` back to [`inputx_scoring::MatchType`].
/// Inline payload fields are zeroed; callers attach runtime values.
pub fn decode_match_type(b: u8) -> inputx_scoring::MatchType {
    match b {
        0 => inputx_scoring::MatchType::Exact,
        1 => inputx_scoring::MatchType::Prefix(0),
        2 => inputx_scoring::MatchType::Fuzzy(0),
        3 => inputx_scoring::MatchType::Composed { bigram_links: 0 },
        _ => inputx_scoring::MatchType::Exact, // forward-compat: unknown → Exact
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_size_constants_match_spec() {
        assert_eq!(HEADER_SIZE, 64);
        assert_eq!(SHA256_SIZE, 32);
        assert_eq!(FULL_HEADER_SIZE, 96);
        assert_eq!(ENTRY_SIZE, 16);
    }

    #[test]
    fn header_round_trip_preserves_all_fields() {
        let h = Header {
            magic: MAGIC,
            format_version: 1,
            engine_kind: 2,
            flags: 0x0007,
            entry_count: 237_842,
            string_pool_offset: 96,
            string_pool_size: 2_097_152,
            entry_table_offset: 2_097_248,
            fst_code_index_offset: 5_900_000,
            fst_code_index_size: 1_048_576,
            fst_word_index_offset: 6_948_576,
            fst_word_index_size: 524_288,
            bigram_offset: 0,
            bigram_size: 0,
            embedding_offset: 0,
            embedding_dim: 0,
            embedding_dtype: 0,
            reserved: 0,
            sha256_of_payload: [0xab; 32],
        };
        let bytes = h.to_bytes();
        // Pad bytes 64..96 with the sha256 trailer to make Header::parse
        // happy (parse requires at least 96 bytes).
        let mut full = [0u8; FULL_HEADER_SIZE];
        full[..HEADER_SIZE].copy_from_slice(&bytes);
        full[HEADER_SIZE..].copy_from_slice(&h.sha256_of_payload);
        let h2 = Header::parse(&full).expect("parse");
        assert_eq!(h2, h);
    }

    #[test]
    fn header_rejects_wrong_magic() {
        let mut buf = [0u8; FULL_HEADER_SIZE];
        buf[0..4].copy_from_slice(b"WHAT");
        assert!(Header::parse(&buf).is_none());
    }

    #[test]
    fn header_rejects_short_buffer() {
        let buf = [0u8; HEADER_SIZE]; // missing sha256 trailer
        assert!(Header::parse(&buf).is_none());
    }

    #[test]
    fn version_byte_round_trip() {
        assert_eq!(Version::from_byte(1), Some(Version::V1));
        assert_eq!(Version::from_byte(2), None);
        assert_eq!(Version::from_byte(0), None);
    }

    #[test]
    fn engine_kind_byte_round_trip() {
        for k in [
            EngineKind::Pinyin,
            EngineKind::Wubi,
            EngineKind::NihongoJukugo,
            EngineKind::NihongoKanji,
            EngineKind::Other,
        ] {
            assert_eq!(EngineKind::from_byte(k as u8), Some(k));
        }
        assert_eq!(EngineKind::from_byte(99), None);
    }

    #[test]
    fn entry_round_trip_preserves_fields() {
        let e = EntryRecord {
            word_offset: 0x12_3456,
            code_offset: 0xab_cdef,
            log_prior: -42,
            match_type: 1,
            flags: EntryFlags::BLACKLIST | EntryFlags::USER_ADDED,
            bigram_offset: 0xdead_beef,
            embedding_offset: 0,
        };
        let bytes = e.to_bytes();
        assert_eq!(bytes.len(), ENTRY_SIZE);
        let e2 = EntryRecord::parse(&bytes);
        assert_eq!(e2, e);
    }

    #[test]
    fn entry_u24_offsets_truncate_at_24_bits() {
        // u24 max = 0xFFFFFF (16,777,215). String pool may exceed this
        // in theory (e.g., very large embeddings ship). The writer must
        // detect overflow; on the decode side we only check round-trip
        // within the u24 range.
        let e = EntryRecord {
            word_offset: 0xFF_FFFF,
            code_offset: 0,
            log_prior: 0,
            match_type: 0,
            flags: 0,
            bigram_offset: 0,
            embedding_offset: 0,
        };
        let bytes = e.to_bytes();
        let e2 = EntryRecord::parse(&bytes);
        assert_eq!(e2.word_offset, 0xFF_FFFF);
    }

    #[test]
    fn match_type_round_trip() {
        for mt in [
            inputx_scoring::MatchType::Exact,
            inputx_scoring::MatchType::Prefix(800),
            inputx_scoring::MatchType::Fuzzy(300),
            inputx_scoring::MatchType::Composed { bigram_links: 2 },
        ] {
            let b = encode_match_type(mt);
            let back = decode_match_type(b);
            // Inline payload not preserved — only variant tag round-trips.
            assert_eq!(
                core::mem::discriminant(&back),
                core::mem::discriminant(&mt),
                "variant {mt:?} → byte {b} → {back:?} (variant must match)"
            );
        }
        // Unknown byte falls back to Exact (forward-compat).
        assert_eq!(decode_match_type(99), inputx_scoring::MatchType::Exact);
    }
}
