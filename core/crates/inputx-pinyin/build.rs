//! Build-time codegen for golia-pinyin:
//! - `bootstrap.fst` — tiny FST built from `data/bootstrap.tsv` (~125
//!   hand-curated entries, MIT-clean). Available behind the
//!   `bootstrap_only` feature for fast tests / minimal builds.
//!
//! The full `pinyin.fst` (15 MB, ~919k entries) is NOT regenerated at
//! consumer build time — it lives committed at `data/pinyin.fst` and is
//! `include_bytes!()`'d directly by `src/dict.rs`. Maintainers regenerate
//! it via `cargo run --features tools --release --bin pinyin-build-fst`
//! after `data/weights/weights.tsv` changes; CI's `weights-verify` job
//! catches drift in the upstream weights pipeline.

use std::env;
use std::fs;
use std::io::BufWriter;
use std::path::PathBuf;

fn main() {
    let crate_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    build_bootstrap(&crate_dir, &out_dir);

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=data/bootstrap.tsv");
}

fn build_bootstrap(crate_dir: &std::path::Path, out_dir: &std::path::Path) {
    let tsv_path = crate_dir.join("data/bootstrap.tsv");
    let tsv =
        fs::read_to_string(&tsv_path).unwrap_or_else(|_| panic!("{} missing", tsv_path.display()));

    let mut entries: Vec<(Vec<u8>, u64)> = Vec::new();
    for (lineno, raw) in tsv.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(2, '\t');
        let pinyin = parts.next().unwrap_or("").trim();
        let word = parts.next().unwrap_or("").trim();
        if pinyin.is_empty() || word.is_empty() {
            panic!(
                "bootstrap.tsv:{}: bad row (need pinyin\\tword): {raw:?}",
                lineno + 1
            );
        }
        let mut key = pinyin.to_ascii_lowercase().into_bytes();
        key.push(0u8);
        key.extend_from_slice(word.as_bytes());
        entries.push((key, 1));
    }
    entries.sort();
    entries.dedup_by(|a, b| a.0 == b.0);

    let fst_path = out_dir.join("bootstrap.fst");
    let writer = BufWriter::new(fs::File::create(&fst_path).expect("create bootstrap.fst"));
    let mut builder = fst::MapBuilder::new(writer).expect("MapBuilder::new");
    for (key, value) in &entries {
        builder.insert(key, *value).expect("insert");
    }
    builder.finish().expect("MapBuilder::finish");
}
