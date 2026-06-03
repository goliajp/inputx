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
    let mut weights_path = crate_dir.join("data/library.tsv");
    // v1.11 WU-γ: write directly to inputx-pinyin-data-core's
    // `data/pinyin.dict` — the single source of truth post-v1.11.
    // Pre-v1.11 we wrote to the facade's `data/pinyin.dict` AND
    // required a manual copy step to inputx-pinyin-data-core, a
    // sync trap that bit v1.9.0 WU-π.c. Resolving relative to the
    // facade's CARGO_MANIFEST_DIR keeps the path stable across
    // workspace setups.
    let mut dict_path = crate_dir
        .parent()
        .expect("inputx-pinyin parent dir")
        .join("inputx-pinyin-data-core/data/pinyin.dict");
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
        .unwrap_or_else(|_| panic!("{} missing — library.tsv is the post-治理 source of truth", weights_path.display()));

    let min_freq: u64 = std::env::var("PINYIN_FST_MIN_FREQ")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);

    // Load corpus_garbage_filter_v1.tsv: future-proof D1 record. Rows
    // listed here MUST NOT enter the dict even if they're somehow back
    // in library.tsv (e.g. accidentally re-ingested). The garbage filter
    // is the durable D1 contract.
    let garbage_path = crate_dir
        .parent().expect("crates parent")
        .parent().expect("workspace root")
        .join("../tools/scoring/data/polish/corpus_garbage_filter_v1.tsv");
    let mut garbage: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    if let Ok(text) = fs::read_to_string(&garbage_path) {
        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') { continue; }
            let mut parts = line.splitn(3, '\t');
            if let (Some(c), Some(w)) = (parts.next(), parts.next()) {
                garbage.insert((c.trim().to_string(), w.trim().to_string()));
            }
        }
        eprintln!("garbage filter loaded: {} entries from {}", garbage.len(), garbage_path.display());
    }

    // key = pinyin\0word → freq, applying the freq cutoff.
    let mut by_key: HashMap<Vec<u8>, u64> = HashMap::with_capacity(1_000_000);
    let mut total_seen = 0usize;
    let mut dropped_low = 0usize;
    let mut dropped_garbage = 0usize;
    let mut source_digested = 0usize;
    let mut source_polish = 0usize;
    for raw in tsv.lines() {
        let line = raw.trim_end_matches(['\r', '\n']);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split('\t');
        let pinyin = parts.next().unwrap_or("").trim();
        let word = parts.next().unwrap_or("").trim();
        let freq: u64 = parts.next().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
        let source = parts.next().unwrap_or("").trim(); // "digested" | "polish" | ""
        if pinyin.is_empty() || word.is_empty() {
            continue;
        }
        total_seen += 1;
        if garbage.contains(&(pinyin.to_string(), word.to_string())) {
            dropped_garbage += 1;
            continue;
        }
        if freq < min_freq {
            dropped_low += 1;
            continue;
        }
        match source {
            "polish" => source_polish += 1,
            _ => source_digested += 1,
        }
        let mut key = pinyin.as_bytes().to_vec();
        key.push(0u8);
        key.extend_from_slice(word.as_bytes());
        let e = by_key.entry(key).or_insert(0);
        *e = (*e).max(freq);
    }
    eprintln!(
        "freq cutoff: min={min_freq} kept={} dropped_low={dropped_low} dropped_garbage={dropped_garbage} (of {total_seen})",
        by_key.len()
    );
    eprintln!("library sources: {source_digested} digested + {source_polish} polish");

    // Overlays, MAX semantics (identical to build_fst). Skipped under
    // --no-overlay so gate1 can measure pipeline weights without the ad-hoc
    // quickfix/modern boosts (which retire at CP3d/CP5).
    if use_overlays {
        let supplemental_dir = crate_dir
            .parent().expect("crates parent")
            .parent().expect("workspace root")
            .join("../tools/scoring/data");
        let overlays: &[(&str, PathBuf)] = &[
            ("polish-log", supplemental_dir.join("polish/quickfix_boost.tsv")),
            ("modern-vocab", supplemental_dir.join("polish/modern_vocab_v1.tsv")),
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
