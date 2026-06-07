//! `inputx-l0` — per-user pin / boost store for IME engines.
//!
//! "L0" = user level, sits *on top of* the corpus-derived dict
//! `log_prior` and adds a Q4 boost. A pinned word effectively
//! dominates its dict baseline (a boost of `weight × 16` Q4 units
//! adds `weight` log-units = `e^weight ×` linear-space ranking
//! multiplier, which at weight=255 saturates anything natural).
//!
//! # Binary format L0v1
//!
//! ```text
//! +---------------------------+
//! | Header (12 B)             |
//! |   magic[4]   "L0v1"       |
//! |   reserved u32 (= 0)      |
//! |   entry_count u32         |
//! +---------------------------+
//! | Entries (varlen each)     |
//! |   code_len u8             |
//! |   code_bytes              |
//! |   word_len u8             |
//! |   word_bytes              |
//! |   boost i16 (Q4)          |
//! +---------------------------+
//! ```
//!
//! # Hard reset semantics
//!
//! `open` returns `Err(OpenError::BadMagic)` if the file's first 4
//! bytes don't match `b"L0v1"` (including any pre-v1.4 JSON files).
//! Callers that want the "ignore old data, start fresh" behavior use
//! [`L0Store::open_or_empty`] — it mounts as empty whenever `open`
//! fails. This is the intended path for Inputx's reinstall flow,
//! consistent with PLAN.md D3 (L0 hard reset, "user 接受个性化
//! lost (可重建)").

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
extern crate std;

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

pub const MAGIC: [u8; 4] = *b"L0v1";
pub const HEADER_SIZE: usize = 12;
pub const Q4: i32 = 16;

#[derive(Debug)]
pub enum OpenError {
    #[cfg(feature = "std")]
    Io(std::io::Error),
    TooShort,
    BadMagic,
    CorruptEntry,
}

#[cfg(feature = "std")]
impl From<std::io::Error> for OpenError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// One pinned (code, word) → boost association. `boost` is Q4
/// fixed-point: `boost / Q4` is the natural-log additive bonus on top
/// of the engine's dict `log_prior`. Mirrors `inputx_scoring`'s Q4
/// convention so the cement layer can add directly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pin {
    pub code: String,
    pub word: String,
    pub boost: i16,
}

/// User pin / boost store. Backed by a BTreeMap on (code, word) for
/// deterministic iteration order (snapshot serializer relies on this).
#[derive(Default, Clone, Debug)]
pub struct L0Store {
    entries: BTreeMap<(String, String), i16>,
}

impl L0Store {
    /// Empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of pinned entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True iff no pinned entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Look up the boost for `(code, word)`. `None` when not pinned.
    /// Hot-path API — callers add the boost to the dict's `log_prior`
    /// at score-compute time.
    pub fn boost(&self, code: &str, word: &str) -> Option<i16> {
        self.entries
            .get(&(code.to_string(), word.to_string()))
            .copied()
    }

    /// Pin `word` for `code` with `weight` (0..=255). The on-disk
    /// boost is `weight as i16 × Q4` — one log-unit per weight tier.
    /// If an existing pin has higher weight, this is a no-op (don't
    /// regress a stronger pin).
    pub fn pin(&mut self, code: &str, word: &str, weight: u8) {
        let new_boost = weight_to_boost(weight);
        let key = (code.to_string(), word.to_string());
        let cur = self.entries.get(&key).copied().unwrap_or(i16::MIN);
        if new_boost > cur {
            self.entries.insert(key, new_boost);
        }
    }

    /// Drop the pin for `(code, word)`. No-op if absent.
    pub fn forget(&mut self, code: &str, word: &str) {
        self.entries.remove(&(code.to_string(), word.to_string()));
    }

    /// Drop ALL pins. Mac/iOS reinstall fires this via the
    /// [reinstall L0 reset] memory note.
    pub fn reset(&mut self) {
        self.entries.clear();
    }

    /// Iterate entries in deterministic order: `(code asc, word asc)`.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str, i16)> {
        self.entries
            .iter()
            .map(|((c, w), b)| (c.as_str(), w.as_str(), *b))
    }

    /// Serialize to bytes (L0v1 format). Always deterministic — entries
    /// iterate in BTreeMap order.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(HEADER_SIZE + self.entries.len() * 8);
        buf.extend_from_slice(&MAGIC);
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&(self.entries.len() as u32).to_le_bytes());
        for (code, word, boost) in self.iter() {
            assert!(code.len() <= 255, "code length {} exceeds u8", code.len());
            assert!(word.len() <= 255, "word length {} exceeds u8", word.len());
            buf.push(code.len() as u8);
            buf.extend_from_slice(code.as_bytes());
            buf.push(word.len() as u8);
            buf.extend_from_slice(word.as_bytes());
            buf.extend_from_slice(&boost.to_le_bytes());
        }
        buf
    }

    /// Parse from L0v1 bytes. Returns `Err(BadMagic)` for unknown
    /// formats (including old JSON files). For hard-reset semantics
    /// use [`L0Store::open_or_empty_bytes`].
    pub fn from_bytes(buf: &[u8]) -> Result<Self, OpenError> {
        if buf.len() < HEADER_SIZE {
            return Err(OpenError::TooShort);
        }
        if buf[0..4] != MAGIC {
            return Err(OpenError::BadMagic);
        }
        // reserved u32 at [4..8] ignored.
        let entry_count = u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]) as usize;
        let mut entries: BTreeMap<(String, String), i16> = BTreeMap::new();
        let mut p = HEADER_SIZE;
        for _ in 0..entry_count {
            if p + 1 > buf.len() {
                return Err(OpenError::CorruptEntry);
            }
            let code_len = buf[p] as usize;
            p += 1;
            if p + code_len > buf.len() {
                return Err(OpenError::CorruptEntry);
            }
            let code = match core::str::from_utf8(&buf[p..p + code_len]) {
                Ok(s) => s.to_string(),
                Err(_) => return Err(OpenError::CorruptEntry),
            };
            p += code_len;
            if p + 1 > buf.len() {
                return Err(OpenError::CorruptEntry);
            }
            let word_len = buf[p] as usize;
            p += 1;
            if p + word_len > buf.len() {
                return Err(OpenError::CorruptEntry);
            }
            let word = match core::str::from_utf8(&buf[p..p + word_len]) {
                Ok(s) => s.to_string(),
                Err(_) => return Err(OpenError::CorruptEntry),
            };
            p += word_len;
            if p + 2 > buf.len() {
                return Err(OpenError::CorruptEntry);
            }
            let boost = i16::from_le_bytes([buf[p], buf[p + 1]]);
            p += 2;
            entries.insert((code, word), boost);
        }
        Ok(Self { entries })
    }

    /// Parse from L0v1 bytes, falling back to an empty store on any
    /// failure (wrong magic, truncated, etc.). This is the hard-reset
    /// path — old wubi_l0.json / pinyin_l0.json files silently become
    /// "no pins" (intentional, per PLAN.md D3).
    pub fn open_or_empty_bytes(buf: &[u8]) -> Self {
        Self::from_bytes(buf).unwrap_or_default()
    }
}

#[cfg(feature = "std")]
impl L0Store {
    /// Read an L0v1 file from disk.
    pub fn open<P: AsRef<std::path::Path>>(path: P) -> Result<Self, OpenError> {
        let bytes = std::fs::read(path)?;
        Self::from_bytes(&bytes)
    }

    /// Read an L0v1 file from disk; on any failure (missing file,
    /// wrong magic, old JSON, truncated) mount as an empty store.
    /// This is the hard-reset path the Inputx reinstall flow relies
    /// on.
    pub fn open_or_empty<P: AsRef<std::path::Path>>(path: P) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|b| Self::from_bytes(&b).ok())
            .unwrap_or_default()
    }

    /// Atomic write to disk: tmpfile → fsync → rename. Safe across
    /// crash / SIGKILL.
    pub fn save<P: AsRef<std::path::Path>>(&self, path: P) -> std::io::Result<()> {
        use std::io::Write;
        let bytes = self.to_bytes();
        let path = path.as_ref();
        let tmp = path.with_extension("l0.tmp");
        {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(&bytes)?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

/// Map a u8 weight (0..=255) to a Q4 boost. Linear: `weight × Q4` →
/// 1 log unit per weight tier. weight=0 → 0 (effectively no pin),
/// weight=1 → +1 log unit (e× linear bump), weight=255 →
/// saturates i16::MAX.
fn weight_to_boost(weight: u8) -> i16 {
    let scaled = weight as i32 * Q4;
    scaled.clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "std")]
    use tempfile::tempdir;

    #[test]
    fn empty_store_serializes_to_header_only() {
        let s = L0Store::new();
        let bytes = s.to_bytes();
        assert_eq!(bytes.len(), HEADER_SIZE);
        assert_eq!(&bytes[0..4], &MAGIC);
        // entry_count u32 = 0
        assert_eq!(&bytes[8..12], &[0u8; 4]);
    }

    #[test]
    fn pin_then_boost_round_trip() {
        let mut s = L0Store::new();
        s.pin("jixu", "继续", 100);
        s.pin("shinjuku", "新宿", 50);
        assert_eq!(s.boost("jixu", "继续"), Some(weight_to_boost(100)));
        assert_eq!(s.boost("shinjuku", "新宿"), Some(weight_to_boost(50)));
        assert_eq!(s.boost("missing", "x"), None);
    }

    #[test]
    fn pin_higher_weight_overrides_lower() {
        let mut s = L0Store::new();
        s.pin("ni", "你", 50);
        s.pin("ni", "你", 100); // higher
        assert_eq!(s.boost("ni", "你"), Some(weight_to_boost(100)));
    }

    #[test]
    fn pin_lower_weight_does_not_regress() {
        let mut s = L0Store::new();
        s.pin("ni", "你", 100);
        s.pin("ni", "你", 50); // lower → keep 100
        assert_eq!(s.boost("ni", "你"), Some(weight_to_boost(100)));
    }

    #[test]
    fn forget_removes_pin() {
        let mut s = L0Store::new();
        s.pin("ni", "你", 100);
        assert!(s.boost("ni", "你").is_some());
        s.forget("ni", "你");
        assert_eq!(s.boost("ni", "你"), None);
    }

    #[test]
    fn reset_clears_all() {
        let mut s = L0Store::new();
        s.pin("a", "1", 10);
        s.pin("b", "2", 20);
        s.pin("c", "3", 30);
        assert_eq!(s.len(), 3);
        s.reset();
        assert!(s.is_empty());
    }

    #[test]
    fn round_trip_to_bytes_preserves_entries() {
        let mut s = L0Store::new();
        s.pin("jixu", "继续", 100);
        s.pin("shinjuku", "新宿", 50);
        s.pin("ggg", "王", 75);
        let bytes = s.to_bytes();
        let s2 = L0Store::from_bytes(&bytes).unwrap();
        assert_eq!(s2.len(), 3);
        assert_eq!(s2.boost("jixu", "继续"), Some(weight_to_boost(100)));
        assert_eq!(s2.boost("shinjuku", "新宿"), Some(weight_to_boost(50)));
        assert_eq!(s2.boost("ggg", "王"), Some(weight_to_boost(75)));
    }

    #[test]
    fn from_bytes_rejects_wrong_magic() {
        let mut bad = MAGIC.to_vec();
        bad[0] = b'X';
        bad.extend_from_slice(&0u32.to_le_bytes());
        bad.extend_from_slice(&0u32.to_le_bytes());
        match L0Store::from_bytes(&bad) {
            Err(OpenError::BadMagic) => (),
            other => panic!("expected BadMagic, got {other:?}"),
        }
    }

    #[test]
    fn open_or_empty_bytes_hard_resets_on_wrong_magic() {
        // Simulate an old wubi_l0.json (starts with `{`, not the magic).
        let old_json = br#"{"version":1,"pins":[]}"#;
        let s = L0Store::open_or_empty_bytes(old_json);
        assert!(s.is_empty(), "hard reset should produce empty store");
    }

    #[test]
    fn open_or_empty_bytes_hard_resets_on_truncated() {
        let mut s = L0Store::new();
        s.pin("ni", "你", 100);
        let mut bytes = s.to_bytes();
        bytes.truncate(bytes.len() - 1);
        let s2 = L0Store::open_or_empty_bytes(&bytes);
        assert!(s2.is_empty(), "truncated → empty");
    }

    #[test]
    fn iter_is_deterministic_sorted_order() {
        let mut s = L0Store::new();
        s.pin("z", "Z", 1);
        s.pin("a", "A", 1);
        s.pin("m", "M", 1);
        let v: Vec<&str> = s.iter().map(|(c, _, _)| c).collect();
        assert_eq!(v, vec!["a", "m", "z"]);
    }

    #[cfg(feature = "std")]
    #[test]
    fn save_then_open_round_trip() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("pins.l0");
        let mut s = L0Store::new();
        s.pin("ggg", "王", 100);
        s.pin("ji", "继", 50);
        s.save(&p).unwrap();
        let s2 = L0Store::open(&p).unwrap();
        assert_eq!(s2.len(), 2);
        assert_eq!(s2.boost("ggg", "王"), Some(weight_to_boost(100)));
        assert_eq!(s2.boost("ji", "继"), Some(weight_to_boost(50)));
    }

    #[cfg(feature = "std")]
    #[test]
    fn open_or_empty_returns_empty_when_missing() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("missing.l0");
        let s = L0Store::open_or_empty(&p);
        assert!(s.is_empty());
    }

    #[cfg(feature = "std")]
    #[test]
    fn two_saves_produce_identical_files() {
        let dir = tempdir().unwrap();
        let p1 = dir.path().join("a.l0");
        let p2 = dir.path().join("b.l0");
        let mut s = L0Store::new();
        // Insert deliberately out-of-order; BTreeMap normalizes.
        s.pin("z", "末", 30);
        s.pin("a", "始", 100);
        s.pin("m", "中", 50);
        s.save(&p1).unwrap();
        s.save(&p2).unwrap();
        assert_eq!(std::fs::read(&p1).unwrap(), std::fs::read(&p2).unwrap());
    }
}
