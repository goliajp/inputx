//! `IdfBuilder` — deterministic writer for IDFv1 files.
//!
//! Build flow:
//!
//! 1. Caller adds entries via `add_entry`.
//! 2. `build(path)` sorts entries by `(code, word)`, dedupes exact
//!    duplicates, builds the string pool (deduplicated UTF-8), writes
//!    section bytes, computes sha256 of the payload, and atomically
//!    renames a temp file over `path`.
//!
//! Determinism: same input set → byte-identical output. Tests assert
//! this via two-build round-trips matching sha256.

use std::collections::HashMap;
use std::io::{self, Write};
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::codec::{
    EngineKind, EntryFlags, EntryRecord, FULL_HEADER_SIZE, Header, MAGIC, encode_match_type,
};

/// Drafted entry queued in [`IdfBuilder`]; pre-encoding form.
#[derive(Clone, Debug)]
struct EntryDraft {
    code: String,
    word: String,
    log_prior: i16,
    raw_freq: u32,
    match_type: u8,
    flags: u8,
}

/// IdfBuilder. Accumulates entries, then writes them deterministically
/// to disk. One IdfBuilder per output file.
pub struct IdfBuilder {
    engine_kind: EngineKind,
    flags: u16,
    entries: Vec<EntryDraft>,
}

impl IdfBuilder {
    /// Start a new builder for a given engine kind. `flags` is the
    /// per-file header flag word (bit0=has_embeddings, bit1=has_ngram_
    /// extended, bit2=has_phonetic_edit_table); set to 0 for the v1.4
    /// baseline (no auxiliary blocks).
    pub fn new(engine_kind: EngineKind) -> Self {
        Self {
            engine_kind,
            flags: 0,
            entries: Vec::new(),
        }
    }

    /// Queue one entry. `code` is the lookup key (pinyin syllable,
    /// wubi code, romaji reading, etc.); `word` is the displayed form;
    /// `log_prior` is the Q4 fixed-point log prior (use
    /// `inputx_scoring::log_prior_from_freq` to derive from a raw
    /// frequency); `raw_freq` is the pre-quantization corpus frequency
    /// (v1.4.7 sub-phase A4: lossless tiebreaker for entries that
    /// collide in the same Q4 `log_prior` bucket — e.g. high-frequency
    /// single-char readings whose freq differences quantize out, like
    /// 乎/护 for code `hu`). Pass `0` when no meaningful freq is
    /// available (synthetic / placeholder entries).
    pub fn add_entry(
        &mut self,
        code: &str,
        word: &str,
        log_prior: i16,
        raw_freq: u32,
        match_type: inputx_scoring::MatchType,
        flags: EntryFlags,
    ) {
        self.entries.push(EntryDraft {
            code: code.to_string(),
            word: word.to_string(),
            log_prior,
            raw_freq,
            match_type: encode_match_type(match_type),
            flags: flags.0,
        });
    }

    /// Total count of queued entries (before dedupe).
    pub fn pending_count(&self) -> usize {
        self.entries.len()
    }

    /// Build the file. Atomic: writes to `<path>.tmp` then renames over
    /// `path`. Returns the final sha256 of the payload so callers can
    /// log / write a `meta.toml` entry without re-reading the file.
    pub fn build(mut self, path: &Path) -> io::Result<[u8; 32]> {
        // 1. Sort + dedupe for determinism.
        self.entries.sort_by(|a, b| {
            a.code
                .cmp(&b.code)
                .then_with(|| a.word.cmp(&b.word))
                .then_with(|| a.log_prior.cmp(&b.log_prior))
        });
        self.entries
            .dedup_by(|a, b| a.code == b.code && a.word == b.word);
        let entry_count = self.entries.len() as u32;

        // 2. String pool. Dedupe distinct UTF-8 strings; assign byte
        // offsets sorted by string for determinism.
        let mut unique: Vec<&str> = Vec::with_capacity(entry_count as usize * 2);
        for e in &self.entries {
            unique.push(e.code.as_str());
            unique.push(e.word.as_str());
        }
        unique.sort_unstable();
        unique.dedup();
        let mut pool_bytes: Vec<u8> = Vec::with_capacity(unique.len() * 8);
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
            pool_bytes.push(0); // null terminator
        }
        let string_pool_size = pool_bytes.len() as u32;
        // Pad pool to 8-byte boundary.
        while pool_bytes.len() % 8 != 0 {
            pool_bytes.push(0);
        }

        // 3. Entry table.
        let mut entry_bytes: Vec<u8> =
            Vec::with_capacity(entry_count as usize * crate::codec::ENTRY_SIZE);
        for e in &self.entries {
            let rec = EntryRecord {
                word_offset: pool_offsets[e.word.as_str()],
                code_offset: pool_offsets[e.code.as_str()],
                log_prior: e.log_prior,
                match_type: e.match_type,
                flags: e.flags,
                raw_freq: e.raw_freq,
                embedding_offset: 0,
            };
            entry_bytes.extend_from_slice(&rec.to_bytes());
        }

        // 4. FST code index (populated v1.4.6 sub-phase C1 for hot-path
        // latency). Entries are already sorted by (code, word), so all
        // readings of one code form a contiguous run; the FST stores
        // `code → first_entry_index` and the reader walks
        // `entries[first..]` until the code changes. `inputx_fsa` keys
        // must be unique — multi-reading codes get one FST entry
        // pointing at their first reading.
        //
        // Word index stays empty in v1.4.6: `find_by_word` is used only
        // by L0 join and blacklist post-merge, not the hot keystroke
        // path, so linear scan over the entry table is acceptable. A
        // future polish wave can add it.
        let fst_code_index: Vec<u8> = {
            let mut fb = inputx_fsa::Builder::new();
            let mut last_code: Option<&str> = None;
            for (i, e) in self.entries.iter().enumerate() {
                if Some(e.code.as_str()) == last_code {
                    continue;
                }
                fb.insert(e.code.as_bytes(), i as u64);
                last_code = Some(e.code.as_str());
            }
            fb.finish()
        };
        let fst_word_index: Vec<u8> = Vec::new();

        // 5. Layout offsets.
        let string_pool_offset = FULL_HEADER_SIZE as u32;
        let entry_table_offset = string_pool_offset + pool_bytes.len() as u32;
        let fst_code_index_offset = entry_table_offset + entry_bytes.len() as u32;
        let fst_word_index_offset = fst_code_index_offset + fst_code_index.len() as u32;

        let header = Header {
            magic: MAGIC,
            format_version: 1,
            engine_kind: self.engine_kind as u8,
            flags: self.flags,
            entry_count,
            string_pool_offset,
            string_pool_size,
            entry_table_offset,
            fst_code_index_offset,
            fst_code_index_size: fst_code_index.len() as u32,
            fst_word_index_offset,
            fst_word_index_size: fst_word_index.len() as u32,
            bigram_offset: 0,
            bigram_size: 0,
            embedding_offset: 0,
            embedding_dim: 0,
            embedding_dtype: 0,
            reserved: 0,
            sha256_of_payload: [0; 32],
        };
        let header_bytes = header.to_bytes();

        // 6. Compute payload sha256 (everything after the sha256 field).
        let mut hasher = Sha256::new();
        hasher.update(&pool_bytes);
        hasher.update(&entry_bytes);
        hasher.update(&fst_code_index);
        hasher.update(&fst_word_index);
        let sha: [u8; 32] = hasher.finalize().into();

        // 7. Write to tmp file, fsync, rename. Atomic across crashes.
        let tmp = path.with_extension("idf.tmp");
        {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(&header_bytes)?;
            f.write_all(&sha)?;
            f.write_all(&pool_bytes)?;
            f.write_all(&entry_bytes)?;
            f.write_all(&fst_code_index)?;
            f.write_all(&fst_word_index)?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, path)?;
        Ok(sha)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::IdfReader;
    use inputx_scoring::MatchType;
    use tempfile::tempdir;

    #[test]
    fn round_trip_50_entries_recovers_all() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("rt.idf");
        let mut b = IdfBuilder::new(EngineKind::Pinyin);
        for i in 0..50 {
            b.add_entry(
                &format!("code{i}"),
                &format!("word{i}"),
                i16::try_from(i * 10).unwrap(),
                0,
                MatchType::Exact,
                EntryFlags::default(),
            );
        }
        b.build(&path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let r = IdfReader::from_bytes(bytes).unwrap();
        assert_eq!(r.entry_count(), 50);
        for i in 0..50 {
            let hit = r.lookup(format!("code{i}").as_bytes());
            assert_eq!(hit.len(), 1, "code{i} should have exactly 1 entry");
            assert_eq!(hit[0].word, format!("word{i}"));
            assert_eq!(hit[0].log_prior, (i * 10) as i16);
        }
    }

    #[test]
    fn two_builds_same_input_produce_identical_sha256() {
        let dir = tempdir().unwrap();
        let path1 = dir.path().join("a.idf");
        let path2 = dir.path().join("b.idf");
        let mk = || {
            let mut b = IdfBuilder::new(EngineKind::Wubi);
            // Insert deliberately out-of-order to exercise the sort step.
            for i in [42, 7, 19, 3, 88, 100, 1] {
                b.add_entry(
                    &format!("c{i:03}"),
                    &format!("w{i:03}"),
                    i as i16,
                    0,
                    MatchType::Exact,
                    EntryFlags::default(),
                );
            }
            b
        };
        let sha1 = mk().build(&path1).unwrap();
        let sha2 = mk().build(&path2).unwrap();
        assert_eq!(sha1, sha2, "deterministic build requires identical sha");
        let b1 = std::fs::read(&path1).unwrap();
        let b2 = std::fs::read(&path2).unwrap();
        assert_eq!(b1, b2, "files must be byte-identical");
    }

    #[test]
    fn multi_reading_same_code_returns_all_entries() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("h.idf");
        let mut b = IdfBuilder::new(EngineKind::Pinyin);
        // 长 has at least two readings; here we abuse the field for the
        // multi-reading test — same code, different words.
        for w in ["你", "妮", "尼", "拟"] {
            b.add_entry("ni", w, 100, 0, MatchType::Exact, EntryFlags::default());
        }
        b.build(&path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let r = IdfReader::from_bytes(bytes).unwrap();
        let hits = r.lookup(b"ni");
        assert_eq!(hits.len(), 4, "all 4 readings should be returned");
        let words: Vec<&str> = hits.iter().map(|e| e.word).collect();
        assert!(words.contains(&"你"));
        assert!(words.contains(&"尼"));
    }

    #[test]
    fn duplicate_code_word_pair_deduped() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("d.idf");
        let mut b = IdfBuilder::new(EngineKind::Pinyin);
        b.add_entry("ni", "你", 100, 0, MatchType::Exact, EntryFlags::default());
        b.add_entry("ni", "你", 100, 0, MatchType::Exact, EntryFlags::default());
        b.add_entry("ni", "你", 100, 0, MatchType::Exact, EntryFlags::default());
        b.build(&path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let r = IdfReader::from_bytes(bytes).unwrap();
        assert_eq!(r.entry_count(), 1);
        assert_eq!(r.lookup(b"ni").len(), 1);
    }

    #[test]
    fn prefix_top_k_returns_top_by_log_prior_desc() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("p.idf");
        let mut b = IdfBuilder::new(EngineKind::Pinyin);
        b.add_entry("z", "之", 100, 0, MatchType::Exact, EntryFlags::default());
        b.add_entry(
            "zhong",
            "中",
            500,
            0,
            MatchType::Exact,
            EntryFlags::default(),
        );
        b.add_entry(
            "zhongguo",
            "中国",
            700,
            0,
            MatchType::Exact,
            EntryFlags::default(),
        );
        b.add_entry(
            "zhongguodian",
            "中国电",
            50,
            0,
            MatchType::Exact,
            EntryFlags::default(),
        );
        b.add_entry(
            "xinjiang",
            "新疆",
            999,
            0,
            MatchType::Exact,
            EntryFlags::default(),
        );
        b.build(&path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let r = IdfReader::from_bytes(bytes).unwrap();
        let hits = r.prefix_top_k(b"zhong", 3);
        assert_eq!(hits.len(), 3);
        assert_eq!(hits[0].word, "中国");
        assert_eq!(hits[1].word, "中");
        assert_eq!(hits[2].word, "中国电");
    }

    #[test]
    fn prefix_for_each_entry_streams_all_matches_fst_order() {
        // Streaming sibling of prefix_top_k_fst: no sort, no truncate.
        // Used by cement layers (composite/pinyin_adapter.rs prefix
        // prediction + single-letter cache) that need to apply
        // length-bias / proximity factors PER entry before ranking.
        let dir = tempdir().unwrap();
        let path = dir.path().join("pfe.idf");
        let mut b = IdfBuilder::new(EngineKind::Pinyin);
        // Mixed under prefix `zhong`: two share code zhongguo (multi-
        // reading run) so we also assert that all readings of one code
        // are visited together.
        b.add_entry("z", "之", 100, 0, MatchType::Exact, EntryFlags::default());
        b.add_entry(
            "zhong",
            "中",
            500,
            0,
            MatchType::Exact,
            EntryFlags::default(),
        );
        b.add_entry(
            "zhongguo",
            "中国",
            700,
            0,
            MatchType::Exact,
            EntryFlags::default(),
        );
        b.add_entry(
            "zhongguo",
            "种过",
            50,
            0,
            MatchType::Exact,
            EntryFlags::default(),
        );
        b.add_entry(
            "zhongguodian",
            "中国电",
            30,
            0,
            MatchType::Exact,
            EntryFlags::default(),
        );
        b.add_entry(
            "xinjiang",
            "新疆",
            999,
            0,
            MatchType::Exact,
            EntryFlags::default(),
        );
        b.build(&path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let r = IdfReader::from_bytes(bytes).unwrap();

        let mut visited: Vec<(String, String, i16)> = Vec::new();
        r.prefix_for_each_entry(b"zhong", |e| {
            visited.push((e.code.to_string(), e.word.to_string(), e.log_prior));
        });
        // All 4 zhong* entries visited, none of the `z` / `xinjiang` ones.
        assert_eq!(visited.len(), 4, "got {visited:?}");
        for (code, _, _) in &visited {
            assert!(code.starts_with("zhong"));
        }
        // FST code-asc visit order: zhong < zhongguo (both readings) < zhongguodian.
        let codes: Vec<&str> = visited.iter().map(|(c, _, _)| c.as_str()).collect();
        assert_eq!(codes, vec!["zhong", "zhongguo", "zhongguo", "zhongguodian"]);
        // Multi-reading run for `zhongguo` visits both 中国 + 种过.
        let words_for_zhongguo: Vec<&str> = visited
            .iter()
            .filter(|(c, _, _)| c == "zhongguo")
            .map(|(_, w, _)| w.as_str())
            .collect();
        assert_eq!(words_for_zhongguo.len(), 2);
        assert!(words_for_zhongguo.contains(&"中国"));
        assert!(words_for_zhongguo.contains(&"种过"));
    }

    #[test]
    fn prefix_for_each_entry_empty_prefix_visits_all() {
        // Empty prefix is a full scan via FST root walk; cement
        // initials-index build does this once at warmup.
        let dir = tempdir().unwrap();
        let path = dir.path().join("pfe_empty.idf");
        let mut b = IdfBuilder::new(EngineKind::Pinyin);
        b.add_entry("a", "啊", 10, 0, MatchType::Exact, EntryFlags::default());
        b.add_entry("ni", "你", 20, 0, MatchType::Exact, EntryFlags::default());
        b.add_entry(
            "zhong",
            "中",
            30,
            0,
            MatchType::Exact,
            EntryFlags::default(),
        );
        b.build(&path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let r = IdfReader::from_bytes(bytes).unwrap();
        let mut count = 0;
        r.prefix_for_each_entry(b"", |_| count += 1);
        assert_eq!(count, 3);
    }

    #[test]
    fn find_by_word_round_trip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("w.idf");
        let mut b = IdfBuilder::new(EngineKind::Pinyin);
        b.add_entry(
            "changchang",
            "长长",
            100,
            0,
            MatchType::Exact,
            EntryFlags::default(),
        );
        b.add_entry(
            "zhang",
            "长",
            200,
            0,
            MatchType::Exact,
            EntryFlags::default(),
        );
        b.add_entry(
            "chang",
            "长",
            300,
            0,
            MatchType::Exact,
            EntryFlags::default(),
        );
        b.build(&path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let r = IdfReader::from_bytes(bytes).unwrap();
        let hits = r.find_by_word("长");
        assert_eq!(hits.len(), 2);
        let codes: Vec<&str> = hits.iter().map(|e| e.code).collect();
        assert!(codes.contains(&"zhang"));
        assert!(codes.contains(&"chang"));
    }

    #[test]
    fn sha256_mismatch_rejected() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("tamper.idf");
        let mut b = IdfBuilder::new(EngineKind::Pinyin);
        b.add_entry("x", "x", 10, 0, MatchType::Exact, EntryFlags::default());
        b.build(&path).unwrap();
        let mut bytes = std::fs::read(&path).unwrap();
        // Corrupt the entry table region (anywhere after FULL_HEADER_SIZE).
        let target = bytes.len() - 5;
        bytes[target] ^= 0xFF;
        let r = IdfReader::from_bytes(bytes);
        assert!(matches!(r, Err(crate::reader::OpenError::Sha256Mismatch)));
    }
}
