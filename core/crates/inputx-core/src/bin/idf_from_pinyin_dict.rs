//! `idf-from-pinyin-dict` — snapshot the current pinyin dict (.dict /
//! FST) into IDFv1 binary at `data/private-dict/v0.0.1/pinyin/words.idf`.
//!
//! Reads every entry via `PinyinDict::prefix_with_freq("")` (~237k
//! tuples), computes `log_prior = Q4 · ln(raw_freq / total_corpus)`,
//! writes to .idf via `IdfBuilder`. Deterministic: two runs from the
//! same source produce byte-identical output (sha256 stable).
//!
//! Usage:
//!   cargo run --release --bin idf-from-pinyin-dict -- \
//!       --output data/private-dict/v0.0.1/pinyin/words.idf
//!
//! Default output path is `data/private-dict/v0.0.1/pinyin/words.idf`
//! relative to the workspace root (cwd when invoked from `core/`).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use inputx_dict_format::{EngineKind, EntryFlags, IdfBuilder};
use inputx_pinyin::PinyinDict;
use inputx_scoring::{log_prior_from_freq, MatchType};

/// Mirror of `inputx_core::composite::prior_correction::PRIOR_CORRECTIONS`.
/// Inlined here so this binary doesn't reach into a `pub(crate)`-deep
/// composite path. The composite module retains the same table for the
/// live runtime callsite in `composite/merge.rs` until v1.4.6 sub-phase
/// D drops both (engine cutover stops going through merge.rs's
/// `correct` lambda, and prior_correction.rs deletes).
///
/// Keep in lockstep with `composite/prior_correction.rs` until that
/// file deletes. Each row carries the polish-log citation in the
/// composite module's source (not duplicated here).
const PRIOR_CORRECTIONS: &[(&str, f64)] = &[
    ("继续", 2.0),
    ("设计", 2.0),
    ("理想", 2.0),
    ("加载", 1.5),
    ("具体", 1.5),
    ("统一", 1.5),
];

fn correction_for(word: &str) -> f64 {
    PRIOR_CORRECTIONS
        .iter()
        .find(|(w, _)| *w == word)
        .map(|(_, m)| *m)
        .unwrap_or(1.0)
}

fn main() -> ExitCode {
    let mut output: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--output" | "-o" => {
                output = args.next().map(PathBuf::from);
            }
            "--help" | "-h" => {
                eprintln!(
                    "Usage: idf-from-pinyin-dict [--output PATH]\n\
                     \n\
                     Snapshot inputx-pinyin's bundled dict into IDFv1.\n\
                     \n\
                     Default output: data/private-dict/v0.0.1/pinyin/words.idf"
                );
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("unknown argument: {other}");
                return ExitCode::FAILURE;
            }
        }
    }
    let out = output.unwrap_or_else(|| {
        PathBuf::from("../data/private-dict/v0.0.1/pinyin/words.idf")
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
    eprintln!("[idf-from-pinyin-dict] loading pinyin dict ...");
    let dict = PinyinDict::embedded();
    let entries = dict.prefix_with_freq("");
    let entry_count = entries.len();
    let total_corpus: u128 = entries.iter().map(|(_, _, f)| *f as u128).sum();
    eprintln!(
        "[idf-from-pinyin-dict] loaded {entry_count} entries, total corpus freq = {total_corpus}"
    );

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut builder = IdfBuilder::new(EngineKind::Pinyin);
    let mut corrected = 0usize;
    for (code, word, raw_freq) in &entries {
        // v1.4.6 sub-phase B1: absorb prior_correction multiplier into
        // log_prior at snapshot time. The 6 corrected words (继续 / 设计
        // / 理想 / 加载 / 具体 / 统一, all ×1.5-×2.0) currently get the
        // multiplier via composite/merge.rs's `correct = |w, s| s *
        // correction_for(w)` lambda at every score step. Baking the
        // multiplier into log_prior here lets sub-phase C (engine
        // cutover only reads .idf) drop the lambda + delete
        // prior_correction.rs without changing ranking.
        //
        // Mapping: legacy `score = (base + freq · mult) · correction` →
        // .idf log_prior = Q4 · ln(1 + freq · correction). Strict
        // mathematical non-equivalence (additive in linear vs log-space
        // factor), but rank-preserving for the 6 entries (correction is
        // monotone scalar > 1.0; multiplying freq monotone-preserves
        // log_prior ordering within the engine).
        let correction = correction_for(word);
        let effective_freq = ((*raw_freq as f64) * correction).round() as u64;
        if correction != 1.0 {
            corrected += 1;
        }
        let log_prior_q4 = log_prior_from_freq(effective_freq);
        let log_prior_i16 = clamp_to_i16(log_prior_q4);
        builder.add_entry(
            code,
            word,
            log_prior_i16,
            MatchType::Exact,
            EntryFlags::default(),
        );
    }
    eprintln!(
        "[idf-from-pinyin-dict] applied prior_correction to {corrected} entries",
    );

    eprintln!(
        "[idf-from-pinyin-dict] writing {} -> {}",
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

/// Saturate an i32 Q4 log-prior into the on-disk i16. Should not
/// realistically hit in production: i16 Q4 covers ±2048 log units →
/// freq range 1e-56..1e+56, far beyond any natural corpus.
fn clamp_to_i16(v: i32) -> i16 {
    v.clamp(i16::MIN as i32, i16::MAX as i32) as i16
}
