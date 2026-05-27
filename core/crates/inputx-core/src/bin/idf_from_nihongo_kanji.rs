//! `idf-from-nihongo-kanji` — snapshot the embedded kanji table into
//! IDFv1 at `data/private-dict/v0.0.1/nihongo/kanji.idf`.
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
//!       --output data/private-dict/v0.0.1/nihongo/kanji.idf

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use inputx_dict_format::{EngineKind, EntryFlags, IdfBuilder};
use inputx_nihongo::kanji::KANJI_TABLE;
use inputx_scoring::{log_prior_from_freq, MatchType};

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
        PathBuf::from("../data/private-dict/v0.0.1/nihongo/kanji.idf")
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
    eprintln!("[idf-from-nihongo-kanji] loading KANJI_TABLE ...");
    let mut total_pairs = 0usize;
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut builder = IdfBuilder::new(EngineKind::NihongoKanji);
    // Pre-allocate a single owned buffer for the kanji char-as-string.
    // `char::encode_utf8` writes into a 4-byte stack buffer; we then
    // copy into a `String` to hand to `add_entry`. Alternative: keep
    // the per-reading `String` clones inside the loop body.
    for e in KANJI_TABLE {
        let mut buf = [0u8; 4];
        let kanji_str = e.kanji.encode_utf8(&mut buf).to_string();
        let log_q4 = log_prior_from_freq(e.freq as u64);
        let log_prior_i16 = log_q4.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        for reading in e.readings {
            builder.add_entry(
                reading,
                &kanji_str,
                log_prior_i16,
                MatchType::Exact,
                EntryFlags::default(),
            );
            total_pairs += 1;
        }
    }
    eprintln!(
        "[idf-from-nihongo-kanji] {} kanji × multi-reading = {} pairs",
        KANJI_TABLE.len(),
        total_pairs
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
