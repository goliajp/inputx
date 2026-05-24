//! `pinyin-build-ngrams-fsa` — pack the bigram/trigram TSVs into flat
//! zero-dep `inputx-fsa` indexes (`*.fsa`), the replacement for the
//! `fst`-backed `*.fst` n-gram maps (see .claude/PLAN-self-built-fsa.md B.4).
//!
//! Generic key shape: every column except the last is joined by `\0` to form
//! the key, and the last column is the u64 count. This handles both bigrams
//! (`prev\0next`) and trigrams (`a\0b\0c`) identically, matching the byte
//! layout the runtime already uses for point-get + prefix scan.
//!
//!     cargo run --features tools --release --bin pinyin-build-ngrams-fsa

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use inputx_fsa::Builder;

fn pack(tsv_path: &Path, fsa_path: &Path) {
    let Ok(tsv) = fs::read_to_string(tsv_path) else {
        eprintln!("{}: SKIPPED (missing)", tsv_path.display());
        return;
    };
    let mut builder = Builder::new();
    let mut n = 0usize;
    let mut malformed = 0usize;
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
        // join key parts with \0
        let mut key: Vec<u8> = Vec::new();
        for (i, p) in key_parts.iter().enumerate() {
            if i > 0 {
                key.push(0u8);
            }
            key.extend_from_slice(p.as_bytes());
        }
        builder.insert(&key, count);
        n += 1;
    }
    let bytes = builder.finish();
    fs::write(fsa_path, &bytes).expect("write fsa");
    eprintln!(
        "{}: {n} entries, {malformed} malformed, {:.2} MB",
        fsa_path.file_name().unwrap().to_string_lossy(),
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
    let jobs: &[(&str, &str)] = &[
        ("pinyin_bigrams_inter_v1.tsv", "bigrams.fsa"),
        ("pinyin_bigrams_intra_v1.tsv", "bigrams_intra.fsa"),
        ("pinyin_trigrams_inter_v1.tsv", "trigrams.fsa"),
        ("pinyin_trigrams_intra_v1.tsv", "trigrams_intra.fsa"),
    ];
    for (src, out) in jobs {
        pack(&sup.join(src), &data.join(out));
    }
}
