//! `pinyin-build-fst` — pre-build `data/pinyin.fst` from
//! `data/weights/weights.tsv` (the output of `pinyin-build-weights`).
//!
//! The committed `data/pinyin.fst` is what the published crate ships;
//! consumers don't need `weights.tsv` (or any of the intermediate
//! generation files), keeping the published crate under crates.io's size
//! cap. Maintainers re-run this any time `weights.tsv` changes:
//!
//!     cargo run --features tools --release --bin pinyin-build-fst
//!
//! Then commit `data/pinyin.fst`. CI's `weights-verify` job already
//! catches drift between `weights.tsv` and its inputs; this script's
//! output is downstream of that gate.

use std::env;
use std::fs;
use std::io::BufWriter;
use std::path::PathBuf;

fn main() {
    let crate_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let weights_path = crate_dir.join("data/weights/weights.tsv");
    let fst_path = crate_dir.join("data/pinyin.fst");

    let tsv = fs::read_to_string(&weights_path).unwrap_or_else(|_| {
        panic!(
            "{} missing — run pinyin-build-weights first (item 21)",
            weights_path.display()
        )
    });

    // weights.tsv schema: `<pinyin>\t<word>\t<freq_score>`. Multiple rows
    // per (pinyin, word) shouldn't exist by construction, but dedup
    // defensively (FST::insert requires sorted unique keys).
    let mut entries: Vec<(Vec<u8>, u64)> = Vec::with_capacity(1_000_000);
    for raw in tsv.lines() {
        let line = raw.trim_end_matches(['\r', '\n']);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split('\t');
        let pinyin = parts.next().unwrap_or("").trim();
        let word = parts.next().unwrap_or("").trim();
        let freq: u64 = parts
            .next()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        if pinyin.is_empty() || word.is_empty() {
            continue;
        }
        let mut key = pinyin.as_bytes().to_vec();
        key.push(0u8);
        key.extend_from_slice(word.as_bytes());
        entries.push((key, freq));
    }
    entries.sort();
    entries.dedup_by(|a, b| a.0 == b.0);

    let writer = BufWriter::new(fs::File::create(&fst_path).expect("create pinyin.fst"));
    let mut builder = fst::MapBuilder::new(writer).expect("MapBuilder::new");
    for (key, value) in &entries {
        builder.insert(key, *value).expect("insert");
    }
    builder.finish().expect("MapBuilder::finish");

    eprintln!(
        "wrote {} entries to {} ({} bytes)",
        entries.len(),
        fst_path.display(),
        fs::metadata(&fst_path).map(|m| m.len()).unwrap_or(0),
    );
}
