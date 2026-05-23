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

    // Frequency floor for the deployed FST. weights.tsv is kept whole
    // (source of truth for future re-scoring), but only entries above
    // this cutoff get packed into pinyin.fst — drops the long-tail
    // Wikipedia/Leipzig trash that real users never type but that
    // crowds candidate panels with noise. Empirically 47% of weights
    // sit below freq=100 with no observed pick rate. Tune via env var
    // for experimentation (`PINYIN_FST_MIN_FREQ=50 cargo run ...`).
    let min_freq: u64 = std::env::var("PINYIN_FST_MIN_FREQ")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);

    // weights.tsv schema: `<pinyin>\t<word>\t<freq_score>`. Multiple rows
    // per (pinyin, word) shouldn't exist by construction, but dedup
    // defensively (FST::insert requires sorted unique keys).
    let mut entries: Vec<(Vec<u8>, u64)> = Vec::with_capacity(1_000_000);
    let mut total_seen = 0usize;
    let mut dropped_low = 0usize;
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
        total_seen += 1;
        if freq < min_freq {
            dropped_low += 1;
            continue;
        }
        let mut key = pinyin.as_bytes().to_vec();
        key.push(0u8);
        key.extend_from_slice(word.as_bytes());
        entries.push((key, freq));
    }
    entries.sort();
    entries.dedup_by(|a, b| a.0 == b.0);
    eprintln!(
        "freq cutoff: min={} kept={} dropped={} (of {})",
        min_freq, entries.len(), dropped_low, total_seen
    );

    // Polish-log driven boost overlay. `quickfix_boost.tsv` is generated
    // by `tools/scoring/07_validate/aggregate_polish_log.py` and lists
    // (pinyin, word, freq) tuples derived from real user picks: "user
    // typed BUFFER and reached past #0 N times to pick WORD" → boost
    // freq for that (pinyin, word) pair so it climbs to #0 next time.
    // This is the offline-IME equivalent of Sogou's behavior-driven
    // cloud freq adjustment; the only difference is the loop is per-
    // user and triggered by re-running this build.
    let quickfix_path = crate_dir
        .parent().expect("crates parent")
        .parent().expect("workspace root")
        .join("../tools/scoring/data/polish_reports/quickfix_boost.tsv");
    let mut boost_count = 0usize;
    let mut overlay: std::collections::HashMap<Vec<u8>, u64> = std::collections::HashMap::new();
    if let Ok(qtxt) = fs::read_to_string(&quickfix_path) {
        for raw in qtxt.lines() {
            let line = raw.trim_end_matches(['\r', '\n']);
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.split('\t');
            let pinyin = parts.next().unwrap_or("").trim();
            let word = parts.next().unwrap_or("").trim();
            let freq_s = parts.next().unwrap_or("").trim();
            // freq may have a trailing "# n=N" comment in same field
            let freq_clean = freq_s.split('#').next().unwrap_or("").trim();
            let Ok(freq) = freq_clean.parse::<u64>() else { continue };
            if pinyin.is_empty() || word.is_empty() { continue; }
            let mut key = pinyin.as_bytes().to_vec();
            key.push(0u8);
            key.extend_from_slice(word.as_bytes());
            overlay.insert(key, freq);
        }
        // Apply overlay: upsert into entries. If key exists, overwrite
        // (boost wins by design — it reflects actual user pick rate).
        // If key is new (word wasn't in corpus at all), insert.
        let mut by_key: std::collections::HashMap<Vec<u8>, u64> =
            entries.iter().cloned().collect();
        for (k, v) in &overlay {
            by_key.insert(k.clone(), *v);
            boost_count += 1;
        }
        entries = by_key.into_iter().collect();
        entries.sort();
        entries.dedup_by(|a, b| a.0 == b.0);
        eprintln!(
            "polish-log boost overlay: {} entries from {}",
            boost_count, quickfix_path.display()
        );
    } else {
        eprintln!("polish-log boost overlay: SKIPPED (no {})", quickfix_path.display());
    }

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
