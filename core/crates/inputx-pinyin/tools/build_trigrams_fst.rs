//! `pinyin-build-trigrams-fst` — pack the word-trigram TSVs into two
//! mmap-loadable FSTs.
//!
//! v1.3 联想-conservative (2026-05-24): inter only for prediction.
//!
//! Inputs:
//!   tools/scoring/data/supplemental/pinyin_trigrams_inter_v1.tsv
//!   tools/scoring/data/supplemental/pinyin_trigrams_intra_v1.tsv
//! Outputs:
//!   core/crates/inputx-pinyin/data/trigrams.fst       (inter)
//!   core/crates/inputx-pinyin/data/trigrams_intra.fst (intra; reserved)
//!
//! Key format: <a>\0<b>\0<c>. Range-scan [a\0b\0, a\0b\1) enumerates
//! every trigram completion.

use fst::MapBuilder;
use std::fs;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

fn pack(tsv_path: &Path, fst_path: &Path) {
    let tsv = fs::read_to_string(tsv_path).unwrap_or_else(|_| {
        panic!(
            "{} missing — run `python3 tools/scoring/build_pinyin_trigrams.py` first",
            tsv_path.display()
        )
    });

    let mut entries: Vec<(Vec<u8>, u64)> = Vec::with_capacity(1_000_000);
    let mut malformed = 0usize;
    for raw in tsv.lines() {
        let line = raw.trim_end_matches(['\r', '\n']);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split('\t');
        let a = parts.next().unwrap_or("").trim();
        let b = parts.next().unwrap_or("").trim();
        let c = parts.next().unwrap_or("").trim();
        let count: u64 = parts
            .next()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        if a.is_empty() || b.is_empty() || c.is_empty() || count == 0 {
            malformed += 1;
            continue;
        }
        let mut key = a.as_bytes().to_vec();
        key.push(0u8);
        key.extend_from_slice(b.as_bytes());
        key.push(0u8);
        key.extend_from_slice(c.as_bytes());
        entries.push((key, count));
    }
    entries.sort();
    entries.dedup_by(|a, b| a.0 == b.0);

    let writer = BufWriter::new(
        fs::File::create(fst_path).expect("create trigram fst"),
    );
    let mut builder = MapBuilder::new(writer).expect("MapBuilder::new");
    for (key, value) in &entries {
        builder.insert(key, *value).expect("insert");
    }
    builder.finish().expect("MapBuilder::finish");

    eprintln!(
        "{}: {} entries, {} malformed, {} bytes",
        fst_path.file_name().unwrap().to_string_lossy(),
        entries.len(),
        malformed,
        fs::metadata(fst_path).map(|m| m.len()).unwrap_or(0),
    );
}

fn main() {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let supplemental = crate_dir
        .parent().expect("crates parent")
        .parent().expect("workspace root")
        .join("../tools/scoring/data/supplemental");

    pack(
        &supplemental.join("pinyin_trigrams_inter_v1.tsv"),
        &crate_dir.join("data/trigrams.fst"),
    );
    pack(
        &supplemental.join("pinyin_trigrams_intra_v1.tsv"),
        &crate_dir.join("data/trigrams_intra.fst"),
    );
}
