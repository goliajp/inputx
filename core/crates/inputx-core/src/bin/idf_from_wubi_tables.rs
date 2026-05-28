//! `idf-from-wubi-tables` — snapshot the embedded Wubi 86 dict into
//! IDFv1 at `core/crates/inputx-wubi-data/data/words.idf`.
//!
//! Source: `inputx_wubi::WubiDict::embedded().all_entries()` →
//! `Vec<(code, word, Layer, freq)>` covering all ~135k entries
//! (jianma1 + zigen + jianma2/3 + auto + phrases).
//!
//! `log_prior = Q4 · ln(layer.base() + freq)` — the same multiplicative
//! shape the runtime uses (`layer.base() · pref + freq`), with pref
//! collapsed to 1.0. Layer base bands (per `inputx_wubi::LAYER_BASE`):
//! Jianma1=1M, Jianma2=800k, Jianma3=600k, Zigen=500k, Phrase=400k,
//! Auto=70k. Q4·ln gives ~221 for Jianma1, ~209 for Auto.
//!
//! Usage:
//!   cargo run --release --bin idf-from-wubi-tables -- \
//!       --output core/crates/inputx-wubi-data/data/words.idf

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use inputx_dict_format::{EngineKind, EntryFlags, IdfBuilder};
use inputx_scoring::{MatchType, Q4};
use inputx_wubi::WubiDict;

fn main() -> ExitCode {
    let mut output: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--output" | "-o" => output = args.next().map(PathBuf::from),
            "--help" | "-h" => {
                eprintln!("Usage: idf-from-wubi-tables [--output PATH]");
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("unknown argument: {other}");
                return ExitCode::FAILURE;
            }
        }
    }
    let out = output.unwrap_or_else(|| {
        PathBuf::from("crates/inputx-wubi-data/data/words.idf")
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
    eprintln!("[idf-from-wubi-tables] loading wubi dict ...");
    let dict = WubiDict::embedded();
    let entries = dict.all_entries();
    let entry_count = entries.len();
    eprintln!("[idf-from-wubi-tables] loaded {entry_count} entries");

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut builder = IdfBuilder::new(EngineKind::Wubi);
    for (code, word, layer, freq) in &entries {
        let effective = layer.base().saturating_add(*freq);
        let log_q4 = ((effective.max(1) as f64).ln() * Q4 as f64).round() as i32;
        let log_prior_i16 = log_q4.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        // raw_freq carries the per-entry frequency from the wubi dict
        // (the same `freq` field facade `lookup_with_freq_layer_into`
        // hands back). v1.4.7 A4 step 2: cement-side `lookup_with_freq
        // _layer` reads this directly from IDF so the runtime never
        // needs the facade dict for corpus lookup.
        let raw_freq_u32 = (*freq).min(u32::MAX as u64) as u32;
        // EntryFlags::engine_tag bits encode Layer enum index so the
        // cement reader can reverse the layer without scanning a side
        // table or implying it from a raw_freq band (layer + freq are
        // additive in the legacy score, so the (layer.base + freq)
        // ordering would otherwise collapse same-bucket entries from
        // different layers).
        let flags = EntryFlags::default()
            .with_engine_tag(layer.as_index() as u8);
        builder.add_entry(
            code,
            word,
            log_prior_i16,
            raw_freq_u32,
            MatchType::Exact,
            flags,
        );
    }

    eprintln!(
        "[idf-from-wubi-tables] writing {} -> {}",
        builder.pending_count(),
        out_path.display()
    );
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
