//! `idf-from-nihongo-jukugo` — snapshot the embedded jukugo (熟語) const
//! table into IDFv1 at `core/crates/inputx-nihongo-data-jukugo/data/jukugo.idf`.
//!
//! Source: `inputx_nihongo::jukugo::JUKUGO_TABLE` (~27k entries,
//! `reading` (romaji), `kanji` (UTF-8 multi-char), `freq` (u32)).
//!
//! `log_prior = inputx_scoring::log_prior_from_freq(freq)`. Match type
//! defaults to `Exact` for the dict-baseline entry; runtime paths attach
//! `Prefix(prox_milli)` for the shinjuk → 新宿 prediction case.
//!
//! Usage:
//!   cargo run --release --bin idf-from-nihongo-jukugo -- \
//!       --output core/crates/inputx-nihongo-data-jukugo/data/jukugo.idf

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use inputx_dict_format::{EngineKind, EntryFlags, IdfBuilder};
use inputx_nihongo::jukugo::JUKUGO_TABLE;
use inputx_scoring::{log_prior_from_freq, MatchType};

fn main() -> ExitCode {
    let mut output: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--output" | "-o" => output = args.next().map(PathBuf::from),
            "--help" | "-h" => {
                eprintln!("Usage: idf-from-nihongo-jukugo [--output PATH]");
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("unknown argument: {other}");
                return ExitCode::FAILURE;
            }
        }
    }
    let out = output.unwrap_or_else(|| {
        PathBuf::from("crates/inputx-nihongo-data-jukugo/data/jukugo.idf")
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
    eprintln!("[idf-from-nihongo-jukugo] loading JUKUGO_TABLE ...");
    let entry_count = JUKUGO_TABLE.len();
    eprintln!("[idf-from-nihongo-jukugo] {entry_count} entries");
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut builder = IdfBuilder::new(EngineKind::NihongoJukugo);
    for e in JUKUGO_TABLE {
        let log_q4 = log_prior_from_freq(e.freq as u64);
        let log_prior_i16 = log_q4.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        builder.add_entry(
            e.reading,
            e.kanji,
            log_prior_i16,
            e.freq,
            MatchType::Exact,
            EntryFlags::default(),
        );
    }
    let sha = builder.build(out_path)?;
    let sha_hex: String = sha.iter().map(|b| format!("{b:02x}")).collect();
    let size = std::fs::metadata(out_path)?.len();
    println!(
        "wrote {} ({} entries, {} bytes, sha256 {})",
        out_path.display(),
        entry_count,
        size,
        sha_hex
    );
    Ok(())
}
