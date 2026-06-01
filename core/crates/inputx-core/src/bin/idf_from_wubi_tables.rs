//! `idf-from-wubi-tables` — snapshot the embedded Wubi 86 dict into
//! IDFv1 at `core/crates/inputx-wubi-data/data/words.idf`.
//!
//! Source: `inputx_wubi::WubiDict::embedded().all_entries()` →
//! `Vec<(code, word, Layer, freq)>` covering all ~135k entries
//! (jianma1 + zigen + jianma2/3 + auto + phrases).
//!
//! `log_prior = inputx_scoring::log_prob_corpus_from_freq(raw_freq,
//! Σraw_freq)` — a real log-probability `Q4·ln((1+freq)/(1+total))`.
//! Cross-engine comparable with pinyin / nihongo .idf log_prior fields
//! (v1.7.4 megachange). Layer signal lives in `EntryFlags::engine_tag`,
//! not in the prior — the runtime synth in `composite/dispatch.rs`
//! recombines `(layer.base · pref · demotes)` into `log_likelihood_q4`,
//! and the cross-engine merge weighs simcode prominence via
//! `EngineWeights::simcode_boost_q4` rather than baking it into the
//! data primitive.
//!
//! Usage:
//!   cargo run --release --bin idf-from-wubi-tables -- \
//!       --output core/crates/inputx-wubi-data/data/words.idf

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use inputx_dict_format::{EngineKind, EntryFlags, IdfBuilder};
use inputx_scoring::{log_prob_corpus_from_freq, MatchType};
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

    // v1.7.4: corpus-total denominator = Σ raw_freq across all .idf
    // entries. Wubi raw_freq spans 0 (bootstrap 字根 entries) up to
    // ~50k for top-frequency words — most entries cluster low, so the
    // total is on the ~tens-of-millions scale. This MUST match what
    // `inputx_wubi_data::wubi_corpus_total()` computes at runtime by
    // scanning the same .idf — they iterate the same entries, no
    // filtering on either side.
    let total_corpus: u64 = entries.iter().map(|(_, _, _, f)| *f).sum();
    eprintln!("[idf-from-wubi-tables] corpus total raw_freq = {total_corpus}");
    let mut builder = IdfBuilder::new(EngineKind::Wubi);
    for (code, word, layer, freq) in &entries {
        let log_q4 = log_prob_corpus_from_freq(*freq, total_corpus);
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
