//! `idf-from-pinyin-bigrams` — snapshot the pinyin bigram FST into
//! NGMv1 at `core/crates/inputx-pinyin-helpers/data/bigrams.ngm`.
//!
//! Source: `inputx_pinyin::PinyinDict::iter_bigrams()` →
//! `Vec<(prev, next, count)>` covering ~500k pairs from the embedded
//! `bigrams.fsa`. For each pair, compute
//! `log_prob_q4 = Q4 · ln(count)` and write the (prev, next, log_prob)
//! triplet. Bigram counts span ~1..1M; Q4·ln gives 0..~221 — fits i16.
//!
//! Cap to TOP_K_PER_PREV per prev word to bound output size. The
//! reasoning: a prev with 50k followers (e.g. 的) dominates file size
//! disproportionately, and runtime predict_next_words already truncates
//! to ~10. v1.4.4 ships with TOP_K_PER_PREV=50 (engine layer surfaces
//! ≤10 typical, 50 gives headroom for fuzzy / rank-tie cases).
//!
//! Determinism gate: 2 runs from the same source produce byte-identical
//! .ngm (same sha256). NgramBuilder sorts + dedupes internally.
//!
//! Usage:
//!   cargo run --release --bin idf-from-pinyin-bigrams -- \
//!       --output core/crates/inputx-pinyin-helpers/data/bigrams.ngm

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use inputx_ngram::{NgramBuilder, Q4};
use inputx_pinyin::PinyinDict;

const TOP_K_PER_PREV: usize = 50;
/// Filter weak bigrams. v1.3 runtime uses MIN_PREDICTION_COUNT=30 for
/// the predict_next_words API; we keep the same floor on the snapshot
/// (counts < 30 add file-size noise and aren't surfaced anyway).
const MIN_COUNT: u64 = 30;

fn main() -> ExitCode {
    let mut output: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--output" | "-o" => output = args.next().map(PathBuf::from),
            "--help" | "-h" => {
                eprintln!("Usage: idf-from-pinyin-bigrams [--output PATH]");
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("unknown argument: {other}");
                return ExitCode::FAILURE;
            }
        }
    }
    let out = output.unwrap_or_else(|| {
        PathBuf::from("crates/inputx-pinyin-helpers/data/bigrams.ngm")
    });
    match run(&out) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(out_path: &Path) -> std::io::Result<()> {
    eprintln!("[idf-from-pinyin-bigrams] loading pinyin dict ...");
    let dict = PinyinDict::embedded();
    let all_bigrams = dict.iter_bigrams();
    eprintln!(
        "[idf-from-pinyin-bigrams] raw bigram count = {}",
        all_bigrams.len()
    );

    // Group by prev for the top-k cap.
    let mut by_prev: HashMap<String, Vec<(String, u64)>> = HashMap::new();
    for (prev, next, count) in all_bigrams {
        if count < MIN_COUNT {
            continue;
        }
        by_prev.entry(prev).or_default().push((next, count));
    }
    eprintln!(
        "[idf-from-pinyin-bigrams] {} unique prev words after MIN_COUNT={MIN_COUNT} filter",
        by_prev.len()
    );

    // Truncate per-prev to top-K. Collect owned prev keys upfront so we
    // can mutate the HashMap values without re-borrowing the keys.
    let mut keep_total = 0usize;
    let mut builder = NgramBuilder::new(2);
    let mut prev_keys: Vec<String> = by_prev.keys().cloned().collect();
    prev_keys.sort();
    for prev in &prev_keys {
        let followers = by_prev.get_mut(prev).expect("just inserted");
        followers.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        followers.truncate(TOP_K_PER_PREV);
        for (next, count) in followers.iter() {
            // log_prob_q4 = Q4 · ln(count). count is at least MIN_COUNT
            // so ln is non-negative; max count ~1M → log_q4 ~ 221.
            let lp_q4 = ((*count as f64).ln() * Q4 as f64).round() as i32;
            let lp_i16 = lp_q4.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            builder.add(&[prev.as_str()], next.as_str(), lp_i16);
            keep_total += 1;
        }
    }
    eprintln!(
        "[idf-from-pinyin-bigrams] writing {} triplets (top-{} per prev) -> {}",
        keep_total,
        TOP_K_PER_PREV,
        out_path.display()
    );

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let sha = builder.build(out_path)?;
    let sha_hex: String = sha.iter().map(|b| format!("{b:02x}")).collect();
    let size = std::fs::metadata(out_path)?.len();
    println!(
        "wrote {} ({} triplets, {} bytes, payload sha256 {})",
        out_path.display(),
        keep_total,
        size,
        sha_hex
    );
    Ok(())
}
