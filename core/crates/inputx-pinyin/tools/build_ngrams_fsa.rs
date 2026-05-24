//! `pinyin-build-ngrams-fsa` — pack the bigram/trigram TSVs into zero-dep
//! inputx-fsa indexes (see .claude/PLAN-self-built-fsa.md B.4 / C1).
//!
//! - bigrams + bigrams_intra → flat `Fsa` (`*.fsa`, key→count). bigram_boost
//!   does a point `get(prev\0next)`, so flat keeps that O(keylen) and fast.
//! - trigrams (inter) → two-level `Dict` (`trigrams.dict`, (a\0b)→[(c,count)]).
//!   predict only ever scans `(a\0b, *)`, never point-gets, so the two-level
//!   layout is the natural fit AND ~2 MB smaller than flat (15.55→13.47 MB).
//! - trigrams_intra → flat `Fsa` (reserved/unused at runtime; tiny).
//!
//!     cargo run --features tools --release --bin pinyin-build-ngrams-fsa

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use inputx_fsa::{Builder, DictBuilder};

/// Flat Fsa: every column but the last joined by \0 = key, last = count.
fn pack_fsa(tsv_path: &Path, fsa_path: &Path) {
    let Ok(tsv) = fs::read_to_string(tsv_path) else {
        eprintln!("{}: SKIPPED (missing)", tsv_path.display());
        return;
    };
    let mut b = Builder::new();
    let (mut n, mut malformed) = (0usize, 0usize);
    for raw in tsv.lines() {
        let line = raw.trim_end_matches(['\r', '\n']);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').map(str::trim).collect();
        if cols.len() < 2 {
            malformed += 1;
            continue;
        }
        let count: u64 = cols[cols.len() - 1].parse().unwrap_or(0);
        let key_parts = &cols[..cols.len() - 1];
        if count == 0 || key_parts.iter().any(|p| p.is_empty()) {
            malformed += 1;
            continue;
        }
        let mut key = Vec::new();
        for (i, p) in key_parts.iter().enumerate() {
            if i > 0 {
                key.push(0u8);
            }
            key.extend_from_slice(p.as_bytes());
        }
        b.insert(&key, count);
        n += 1;
    }
    let bytes = b.finish();
    fs::write(fsa_path, &bytes).expect("write fsa");
    eprintln!(
        "{}: {n} entries, {malformed} malformed, {:.2} MB (flat Fsa)",
        fsa_path.file_name().unwrap().to_string_lossy(),
        bytes.len() as f64 / 1_048_576.0
    );
}

/// Two-level Dict: first (ncols-2) columns joined by \0 = code, the
/// second-to-last column = item, last = count.
fn pack_dict(tsv_path: &Path, dict_path: &Path) {
    let Ok(tsv) = fs::read_to_string(tsv_path) else {
        eprintln!("{}: SKIPPED (missing)", tsv_path.display());
        return;
    };
    let mut b = DictBuilder::new();
    let (mut n, mut malformed) = (0usize, 0usize);
    for raw in tsv.lines() {
        let line = raw.trim_end_matches(['\r', '\n']);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').map(str::trim).collect();
        if cols.len() < 3 {
            malformed += 1;
            continue;
        }
        let count: u64 = cols[cols.len() - 1].parse().unwrap_or(0);
        let item = cols[cols.len() - 2];
        let code_parts = &cols[..cols.len() - 2];
        if count == 0 || item.is_empty() || code_parts.iter().any(|p| p.is_empty()) {
            malformed += 1;
            continue;
        }
        let mut code = Vec::new();
        for (i, p) in code_parts.iter().enumerate() {
            if i > 0 {
                code.push(0u8);
            }
            code.extend_from_slice(p.as_bytes());
        }
        b.insert(&code, item.as_bytes(), count);
        n += 1;
    }
    let bytes = b.finish();
    fs::write(dict_path, &bytes).expect("write dict");
    eprintln!(
        "{}: {n} entries, {malformed} malformed, {:.2} MB (two-level Dict)",
        dict_path.file_name().unwrap().to_string_lossy(),
        bytes.len() as f64 / 1_048_576.0
    );
}

fn main() {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let sup = crate_dir
        .parent().expect("crates parent")
        .parent().expect("workspace root")
        .join("../tools/scoring/data/supplemental");
    let data = crate_dir.join("data");
    pack_fsa(&sup.join("pinyin_bigrams_inter_v1.tsv"), &data.join("bigrams.fsa"));
    pack_fsa(&sup.join("pinyin_bigrams_intra_v1.tsv"), &data.join("bigrams_intra.fsa"));
    pack_dict(&sup.join("pinyin_trigrams_inter_v1.tsv"), &data.join("trigrams.dict"));
    pack_fsa(&sup.join("pinyin_trigrams_intra_v1.tsv"), &data.join("trigrams_intra.fsa"));
}
