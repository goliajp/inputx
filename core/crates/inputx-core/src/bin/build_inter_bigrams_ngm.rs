//! `build-inter-bigrams-ngm` — convert `bigrams_inter.tsv` to NGMv1
//! at `core/crates/inputx-pinyin-helpers/data/bigrams_inter.ngm`.
//!
//! Sister to `idf-from-pinyin-bigrams` (which snapshots the intra
//! bigram FST into `bigrams.ngm`). The intra blob covers char-pairs
//! WITHIN a single dict word — useful for "is this string fragment a
//! valid Chinese word piece". The inter blob covers token-pair
//! transitions ACROSS dict-word boundaries — useful for "do these two
//! tokens commonly co-occur in real Chinese text".
//!
//! Why both: the v1.14 K-best 3-segment chain bigram gate (user
//! report 2026-06-06 `luyaozhi → 路要职`) needs to distinguish a real
//! adjacency like `(用, 不)` (yongbuliao → 用不了) from noise like
//! `(路, 要)`. The intra blob alone returns 0 for both pairs because
//! neither is a sub-word of a high-frequency dict entry. The inter
//! blob carries the corpus-level token-adjacency signal that
//! disambiguates them.
//!
//! Format mirrors `idf-from-pinyin-bigrams`: for each (prev, next,
//! count) row, emit `log_prob_q4 = Q4 · ln(count)`. Bigram counts
//! span ~3..50k in `bigrams_inter.tsv` (built with min_count=3,
//! top=500k); Q4·ln gives ~17..173 — fits i16.
//!
//! Determinism gate: 2 runs from the same source TSV produce
//! byte-identical .ngm (same sha256). NgramBuilder sorts + dedupes
//! internally.
//!
//! Usage:
//!   cargo run --release --bin build-inter-bigrams-ngm -- \
//!       --input core/crates/inputx-pinyin/data/bigrams_inter.tsv \
//!       --output core/crates/inputx-pinyin-helpers/data/bigrams_inter.ngm

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use inputx_ngram::{NgramBuilder, Q4};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    // Min raw-count cut for kept entries. The source TSV uses
    // min_count=3 (kept top-500k); the K-best 3-segment gate's
    // discrimination threshold sits around count ≈ 13 (user-report
    // 2026-06-06 calibration: `(路, 要)` count=12 is noise, `(用, 不)`
    // count=16 is real). Default 15 cuts the gray zone; resulting blob
    // is ~1 MB vs 5 MB at no-cut.
    let mut min_count: u64 = 15;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--input" => {
                i += 1;
                input = args.get(i).map(PathBuf::from);
            }
            "--output" => {
                i += 1;
                output = args.get(i).map(PathBuf::from);
            }
            "--min-count" => {
                i += 1;
                min_count = args
                    .get(i)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(min_count);
            }
            "-h" | "--help" => {
                println!("build-inter-bigrams-ngm --input <tsv> --output <ngm> [--min-count N]");
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("unknown arg: {other}");
                return ExitCode::from(2);
            }
        }
        i += 1;
    }
    let input =
        input.unwrap_or_else(|| PathBuf::from("core/crates/inputx-pinyin/data/bigrams_inter.tsv"));
    let output = output.unwrap_or_else(|| {
        PathBuf::from("core/crates/inputx-pinyin-helpers/data/bigrams_inter.ngm")
    });

    let tsv = match fs::read_to_string(&input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("read {}: {e}", input.display());
            return ExitCode::from(1);
        }
    };

    let mut builder = NgramBuilder::new(2);
    let mut rows: u64 = 0;
    let mut skipped: u64 = 0;
    for line in tsv.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let mut parts = line.split('\t');
        let prev = parts.next();
        let next = parts.next();
        let count = parts.next();
        let (prev, next, count) = match (prev, next, count) {
            (Some(p), Some(n), Some(c)) if !p.is_empty() && !n.is_empty() => (p, n, c),
            _ => {
                skipped += 1;
                continue;
            }
        };
        let count: u64 = match count.parse() {
            Ok(c) if c > 0 => c,
            _ => {
                skipped += 1;
                continue;
            }
        };
        if count < min_count {
            skipped += 1;
            continue;
        }
        // Q4 · ln(count). Clamp to i16 range; counts > ~6.6M would overflow.
        let log_prob_q4 = ((count as f64).ln() * Q4 as f64).round();
        let log_prob_q4 = log_prob_q4.clamp(i16::MIN as f64, i16::MAX as f64) as i16;
        builder.add(&[prev], next, log_prob_q4);
        rows += 1;
    }

    if let Some(parent) = Path::new(&output).parent() {
        if !parent.as_os_str().is_empty() {
            let _ = fs::create_dir_all(parent);
        }
    }

    let sha = match builder.build(&output) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("build {}: {e}", output.display());
            return ExitCode::from(1);
        }
    };

    let size = fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
    let sha_hex: String = sha.iter().map(|b| format!("{b:02x}")).collect();
    println!(
        "wrote {} (min_count={min_count}, {rows} rows, {skipped} skipped, {size} bytes, sha256 {sha_hex})",
        output.display()
    );
    ExitCode::SUCCESS
}
