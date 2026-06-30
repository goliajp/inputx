//! `inputx-dogfood` — batch v2 ranking validator
//!
//! Reads a segments TSV (`article_id\tseg_idx\tword\tpinyin\tnotes`),
//! calls `inputx_pinyin_v2::query` for each, and emits a failure log
//! classifying each row as PASS / SOFT (expected in top10 but not #0)
//! / HARD (expected not in top10).
//!
//! Usage:
//!     inputx-dogfood --input segments.tsv --output failures.tsv [--limit N]
//!
//! Failure log schema:
//!     article_id\tseg_idx\tword\tpinyin\tlevel\trank\ttop10
//!     level: PASS | SOFT | HARD
//!     rank: 0..9 if in top10, else 99
//!     top10: comma-separated candidates from index 0

use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::process::ExitCode;

fn print_usage() {
    eprintln!("inputx-dogfood — batch v2 ranking dogfood\n");
    eprintln!("Usage:");
    eprintln!("    inputx-dogfood --input <segments.tsv> --output <failures.tsv> [--limit N]");
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let mut input: Option<String> = None;
    let mut output: Option<String> = None;
    let mut limit: Option<usize> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--input" => {
                input = args.get(i + 1).cloned();
                i += 2;
            }
            "--output" => {
                output = args.get(i + 1).cloned();
                i += 2;
            }
            "--limit" => {
                limit = args.get(i + 1).and_then(|s| s.parse().ok());
                i += 2;
            }
            "-h" | "--help" => {
                print_usage();
                return ExitCode::SUCCESS;
            }
            _ => {
                eprintln!("unknown arg: {}", args[i]);
                print_usage();
                return ExitCode::FAILURE;
            }
        }
    }
    let input = match input {
        Some(s) => s,
        None => {
            eprintln!("--input required");
            return ExitCode::FAILURE;
        }
    };
    let output = match output {
        Some(s) => s,
        None => {
            eprintln!("--output required");
            return ExitCode::FAILURE;
        }
    };

    let in_file = match File::open(&input) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("open input: {}", e);
            return ExitCode::FAILURE;
        }
    };
    let out_file = match File::create(&output) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("create output: {}", e);
            return ExitCode::FAILURE;
        }
    };
    let reader = BufReader::new(in_file);
    let mut writer = BufWriter::new(out_file);
    writeln!(
        writer,
        "# dogfood failures — v2 ranking validation\n# article_id\tseg_idx\tword\tpinyin\tlevel\trank\ttop10"
    )
    .ok();

    let mut total: usize = 0;
    let mut pass: usize = 0;
    let mut soft: usize = 0;
    let mut hard: usize = 0;

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 4 {
            continue;
        }
        let article_id = cols[0];
        let seg_idx = cols[1];
        let word = cols[2];
        let pinyin = cols[3];
        if pinyin.is_empty() {
            continue;
        }

        let scored = inputx_pinyin_v2::query(pinyin);
        let top10: Vec<&String> = scored.iter().take(10).map(|(w, _, _)| w).collect();
        let rank = top10.iter().position(|w| w.as_str() == word);

        let level = match rank {
            Some(0) => {
                pass += 1;
                "PASS"
            }
            Some(_) => {
                soft += 1;
                "SOFT"
            }
            None => {
                hard += 1;
                "HARD"
            }
        };
        let rank_str = rank.map(|r| r.to_string()).unwrap_or_else(|| "99".into());
        let top10_str = top10
            .iter()
            .map(|w| w.as_str())
            .collect::<Vec<_>>()
            .join(",");

        // Only log non-PASS to keep failure file focused.
        if level != "PASS" {
            writeln!(
                writer,
                "{article_id}\t{seg_idx}\t{word}\t{pinyin}\t{level}\t{rank_str}\t{top10_str}"
            )
            .ok();
        }

        total += 1;
        if let Some(lim) = limit {
            if total >= lim {
                break;
            }
        }
    }

    eprintln!("dogfood done: {total} segments");
    eprintln!("  PASS: {pass} ({:.1}%)", 100.0 * pass as f64 / total as f64);
    eprintln!("  SOFT: {soft} ({:.1}%)", 100.0 * soft as f64 / total as f64);
    eprintln!("  HARD: {hard} ({:.1}%)", 100.0 * hard as f64 / total as f64);
    eprintln!("failures → {}", output);
    ExitCode::SUCCESS
}
