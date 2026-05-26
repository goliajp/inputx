//! `pinyin-build-dict` — build `data/pinyin.dict` from
//! `data/weights/weights.tsv`, the zero-dependency `inputx-fsa::Dict`
//! replacement for `pinyin.fst` (see .claude/PLAN-self-built-fsa.md B.4).
//!
//! Identical entry selection to `pinyin-build-fst` (MIN_FREQ cutoff + MAX
//! overlay semantics), but emits a two-level Dict (pinyin code → word list)
//! instead of a flat `pinyin\0word` FST — smaller and zero-dep.
//!
//!     cargo run --features tools --release --bin pinyin-build-dict

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;

use inputx_fsa::DictBuilder;

fn main() {
    let crate_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let mut weights_path = crate_dir.join("data/weights/weights.tsv");
    let mut dict_path = crate_dir.join("data/pinyin.dict");
    let mut use_overlays = true;

    // --weights/--out override the defaults so the CP3 pipeline can build a
    // dict from tools/scoring/data/merged/weights.tsv into a scratch path
    // WITHOUT touching the shipped data/pinyin.dict; --no-overlay skips the
    // ad-hoc quickfix/modern overlays to measure the pipeline weights alone.
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--weights" => {
                weights_path = PathBuf::from(args.next().expect("--weights needs a path"))
            }
            "--out" => dict_path = PathBuf::from(args.next().expect("--out needs a path")),
            "--no-overlay" | "--no-overlays" => use_overlays = false,
            other => {
                panic!("unknown arg `{other}` — expected --weights <p> / --out <p> / --no-overlay")
            }
        }
    }

    let tsv = fs::read_to_string(&weights_path)
        .unwrap_or_else(|_| panic!("{} missing — run pinyin-build-weights first", weights_path.display()));

    let min_freq: u64 = std::env::var("PINYIN_FST_MIN_FREQ")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);

    // key = pinyin\0word → freq, applying the freq cutoff. (Same shape as
    // build_fst so overlay merging is byte-for-byte identical.)
    let mut by_key: HashMap<Vec<u8>, u64> = HashMap::with_capacity(1_000_000);
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
        let freq: u64 = parts.next().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
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
        // dedup defensively: keep the max if a (pinyin,word) repeats.
        let e = by_key.entry(key).or_insert(0);
        *e = (*e).max(freq);
    }
    eprintln!(
        "freq cutoff: min={min_freq} kept={} dropped={dropped_low} (of {total_seen})",
        by_key.len()
    );

    // Overlays, MAX semantics (identical to build_fst). Skipped under
    // --no-overlay so gate1 can measure pipeline weights without the ad-hoc
    // quickfix/modern boosts (which retire at CP3d/CP5).
    if use_overlays {
        let supplemental_dir = crate_dir
            .parent().expect("crates parent")
            .parent().expect("workspace root")
            .join("../tools/scoring/data");
        let overlays: &[(&str, PathBuf)] = &[
            ("polish-log", supplemental_dir.join("polish_reports/quickfix_boost.tsv")),
            ("modern-vocab", supplemental_dir.join("supplemental/pinyin_modern_v1.tsv")),
        ];
        for (label, path) in overlays {
            let Ok(text) = fs::read_to_string(path) else {
                eprintln!("overlay {label}: SKIPPED (no {})", path.display());
                continue;
            };
            let mut applied = 0usize;
            for raw in text.lines() {
                let line = raw.trim_end_matches(['\r', '\n']);
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let mut parts = line.split('\t');
                let pinyin = parts.next().unwrap_or("").trim();
                let word = parts.next().unwrap_or("").trim();
                let freq_clean = parts.next().unwrap_or("").split('#').next().unwrap_or("").trim();
                let Ok(freq) = freq_clean.parse::<u64>() else { continue };
                if pinyin.is_empty() || word.is_empty() {
                    continue;
                }
                let mut key = pinyin.as_bytes().to_vec();
                key.push(0u8);
                key.extend_from_slice(word.as_bytes());
                let merged = match by_key.get(&key) {
                    Some(base) => (*base).max(freq),
                    None => freq,
                };
                by_key.insert(key, merged);
                applied += 1;
            }
            eprintln!("overlay {label}: applied {applied} from {}", path.display());
        }
    }

    // Split keys back into (pinyin, word) and feed the two-level builder.
    let mut builder = DictBuilder::new();
    let mut n = 0usize;
    for (key, freq) in &by_key {
        let sep = key.iter().position(|b| *b == 0u8).expect("key has \\0 separator");
        let (pinyin, rest) = key.split_at(sep);
        let word = &rest[1..];
        builder.insert(pinyin, word, *freq);
        n += 1;
    }
    let bytes = builder.finish();
    fs::write(&dict_path, &bytes).expect("write pinyin.dict");

    eprintln!(
        "wrote {n} entries to {} ({} bytes = {:.2} MB)",
        dict_path.display(),
        bytes.len(),
        bytes.len() as f64 / 1_048_576.0
    );
}
