//! `idf-from-pinyin-dict` — snapshot the current pinyin dict (.dict /
//! FST) into IDFv1 binary at `core/crates/inputx-pinyin-helpers/data/words.idf`.
//!
//! Reads every entry via `PinyinDict::prefix_with_freq("")` (~237k
//! tuples), computes `log_prior = Q4 · ln(raw_freq / total_corpus)`,
//! writes to .idf via `IdfBuilder`. Deterministic: two runs from the
//! same source produce byte-identical output (sha256 stable).
//!
//! Usage:
//!   cargo run --release --bin idf-from-pinyin-dict -- \
//!       --output core/crates/inputx-pinyin-helpers/data/words.idf
//!
//! Default output path is `core/crates/inputx-pinyin-helpers/data/words.idf`
//! relative to the workspace root (cwd when invoked from `core/`).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use inputx_dict_format::{EngineKind, EntryFlags, IdfBuilder};
use inputx_pinyin::PinyinDict;
use inputx_scoring::{MatchType, log_prob_corpus_from_freq};

// User-curated polish-log Q4 log-prior boosts baked into the snapshot
// at build time (v1.4.7 sub-phase A5). The composite-runtime
// `prior_correction.rs` + `merge.rs::correct` lambda retired at the
// same step; corrections now live exclusively in `log_prior_q4` here,
// applied uniformly at IDF read time by the cement IdfReader path.
//
// 2026-06-03 cleanup: data externalized to TSV per user directive
// "no special list, never". Per-entry boosts live in
// `tools/scoring/data/polish/prior_corrections_v1.tsv`.
const PRIOR_CORRECTIONS_TSV: &str =
    include_str!("../../../../../tools/scoring/data/polish/prior_corrections_v1.tsv");

/// Parse `<word>\t<boost_q4>[\t# comment]` rows.
fn parse_prior_corrections(src: &str) -> Vec<(String, i32)> {
    let mut out = Vec::new();
    for raw in src.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(3, '\t');
        let (Some(word), Some(boost_s)) = (parts.next(), parts.next()) else {
            continue;
        };
        let word = word.trim();
        let boost_s = boost_s.split('\t').next().unwrap_or(boost_s).trim();
        if word.is_empty() {
            continue;
        }
        let Ok(boost) = boost_s.parse::<i32>() else {
            continue;
        };
        out.push((word.to_string(), boost));
    }
    out
}

fn correction_for(word: &str, table: &[(String, i32)]) -> i32 {
    table
        .iter()
        .find(|(w, _)| w == word)
        .map(|(_, b)| *b)
        .unwrap_or(0)
}

// Path-1 display filter — entries skipped when building the IDF
// snapshot from `pinyin.dict`. They REMAIN in `pinyin.dict` (so K-best
// composition, initials reverse-lookup, etc. still see them); only
// the user-visible Path-1 ranking layer drops them.
//
// 2026-06-03 design call: per-(code, word) ADDITIONS live directly in
// `weights.tsv` (the dict source of truth) — they need to be in
// pinyin.dict anyway for reverse-lookup. EXCLUSIONS stay as a separate
// overlay because deleting from `weights.tsv` also kills reverse-lookup
// signals (e.g. removing (shen, 什) from weights.tsv breaks `wsm`
// initials lookup of `为什么` via 什's "shen" reading), which is the
// opposite of intent. See `tools/scoring/data/polish/exclusions_v1.tsv`
// header for the full rationale.
const EXCLUSIONS_TSV: &str =
    include_str!("../../../../../tools/scoring/data/polish/exclusions_v1.tsv");

/// Parse `<code>\t<word>[\t# comment]` rows, skipping blanks + comments.
fn parse_exclusions(src: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for raw in src.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(3, '\t');
        let (Some(code), Some(word)) = (parts.next(), parts.next()) else {
            continue;
        };
        let code = code.trim();
        // Allow inline `<word>\t# comment` — strip trailing `\t#`.
        let word = word.split('\t').next().unwrap_or(word).trim();
        if code.is_empty() || word.is_empty() {
            continue;
        }
        out.push((code.to_string(), word.to_string()));
    }
    out
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
                     Default output: core/crates/inputx-pinyin-helpers/data/words.idf"
                );
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("unknown argument: {other}");
                return ExitCode::FAILURE;
            }
        }
    }
    let out =
        output.unwrap_or_else(|| PathBuf::from("crates/inputx-pinyin-helpers/data/words.idf"));
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
    let exclusions = parse_exclusions(EXCLUSIONS_TSV);
    let prior_corrections = parse_prior_corrections(PRIOR_CORRECTIONS_TSV);
    // corpus_total = Σ raw_freq across the entries actually written
    // (source minus the Path-1 display filter). Runtime
    // `pinyin_corpus_total()` scans the .idf and sees the same rows.
    let excluded_freq: u64 = entries
        .iter()
        .filter(|(code, word, _)| exclusions.iter().any(|(ec, ew)| ec == code && ew == word))
        .map(|(_, _, f)| *f)
        .sum();
    let source_total: u64 = entries.iter().map(|(_, _, f)| *f).sum();
    let total_corpus: u64 = source_total - excluded_freq;
    eprintln!(
        "[idf-from-pinyin-dict] loaded {entry_count} entries, source raw_freq sum = {source_total}, post-filter corpus total = {total_corpus}"
    );

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut builder = IdfBuilder::new(EngineKind::Pinyin);
    let mut baked_count = 0usize;
    let mut filtered_count = 0usize;
    for (code, word, raw_freq) in &entries {
        if exclusions.iter().any(|(ec, ew)| ec == code && ew == word) {
            filtered_count += 1;
            continue;
        }
        // v1.4.7 sub-phase A5: prior_correction's Q4 boost is baked
        // into `log_prior_q4` at snapshot build time. raw_freq is
        // left UNBOOSTED — it's the lossless tiebreaker for same-
        // bucket entries, not part of the prior signal.
        let boost = correction_for(word, &prior_corrections);
        if boost != 0 {
            baked_count += 1;
        }
        let log_prior_q4 = log_prob_corpus_from_freq(*raw_freq, total_corpus) + boost;
        let log_prior_i16 = clamp_to_i16(log_prior_q4);
        let raw_freq_u32 = (*raw_freq).min(u32::MAX as u64) as u32;
        builder.add_entry(
            code,
            word,
            log_prior_i16,
            raw_freq_u32,
            MatchType::Exact,
            EntryFlags::default(),
        );
    }
    eprintln!(
        "[idf-from-pinyin-dict] baked prior_correction Q4 boosts into {baked_count} entries (table size: {})",
        prior_corrections.len()
    );
    eprintln!(
        "[idf-from-pinyin-dict] Path-1 display filter: {filtered_count} entries skipped (table size: {})",
        exclusions.len()
    );

    eprintln!(
        "[idf-from-pinyin-dict] writing {} -> {}",
        builder.pending_count(),
        out_path.display()
    );
    let final_count = entry_count - filtered_count;
    let sha = builder.build(out_path)?;
    let sha_hex: String = sha.iter().map(|b| format!("{b:02x}")).collect();
    let size = std::fs::metadata(out_path)?.len();
    println!(
        "wrote {} ({} entries, {} bytes, sha256 {})",
        out_path.display(),
        final_count,
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
