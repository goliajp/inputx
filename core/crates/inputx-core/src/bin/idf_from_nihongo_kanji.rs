//! `idf-from-nihongo-kanji` — snapshot the embedded kanji table into
//! IDFv1 at `core/crates/inputx-nihongo-data-kanji/data/kanji.idf`.
//!
//! Source: `inputx_nihongo::kanji::KANJI_TABLE`. Each entry has 1+
//! Hepburn romaji readings + a single `char` kanji + `freq` (0-100).
//! We FLATTEN per-reading: one IDFv1 entry per (reading, kanji) pair —
//! the IDFv1 model is (code, word, log_prior), and a multi-reading
//! kanji is exactly equivalent to multiple entries with the same word
//! and different codes.
//!
//! Usage:
//!   cargo run --release --bin idf-from-nihongo-kanji -- \
//!       --output core/crates/inputx-nihongo-data-kanji/data/kanji.idf

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use inputx_dict_format::{EngineKind, EntryFlags, IdfBuilder};
use inputx_nihongo::kanji::all_entries as kanji_entries;
use inputx_scoring::{log_prob_corpus_from_freq, MatchType};

fn main() -> ExitCode {
    let mut output: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--output" | "-o" => output = args.next().map(PathBuf::from),
            "--help" | "-h" => {
                eprintln!("Usage: idf-from-nihongo-kanji [--output PATH]");
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("unknown argument: {other}");
                return ExitCode::FAILURE;
            }
        }
    }
    let out = output.unwrap_or_else(|| {
        PathBuf::from("crates/inputx-nihongo-data-kanji/data/kanji.idf")
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
    eprintln!("[idf-from-nihongo-kanji] loading library kanji entries ...");
    let entries = kanji_entries();
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Post-治理: each library row is already one (reading, kanji, freq)
    // tuple — the legacy "1 kanji with N readings" struct has been
    // flattened upstream. So the corpus total is just Σ freq over rows,
    // matching the .idf rows we're about to write.
    let total_corpus: u64 = entries.iter().map(|e| e.freq as u64).sum();
    eprintln!(
        "[idf-from-nihongo-kanji] corpus total raw_freq = {total_corpus}"
    );
    let mut builder = IdfBuilder::new(EngineKind::NihongoKanji);
    let mut total_pairs = 0usize;
    for e in entries {
        let mut buf = [0u8; 4];
        let kanji_str = e.kanji.encode_utf8(&mut buf).to_string();
        let log_q4 = log_prob_corpus_from_freq(e.freq as u64, total_corpus);
        let log_prior_i16 = log_q4.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        builder.add_entry(
            e.reading,
            &kanji_str,
            log_prior_i16,
            e.freq,
            MatchType::Exact,
            EntryFlags::default(),
        );
        total_pairs += 1;
    }
    eprintln!(
        "[idf-from-nihongo-kanji] {total_pairs} (reading, kanji) rows written"
    );
    let sha = builder.build(out_path)?;
    let sha_hex: String = sha.iter().map(|b| format!("{b:02x}")).collect();
    let size = std::fs::metadata(out_path)?.len();
    println!(
        "wrote {} ({} pairs, {} bytes, sha256 {})",
        out_path.display(),
        total_pairs,
        size,
        sha_hex
    );
    Ok(())
}
