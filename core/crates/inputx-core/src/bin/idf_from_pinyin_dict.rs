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
use inputx_scoring::{log_prior_from_freq, MatchType};

/// User-curated polish-log Q4 log-prior boosts baked into the snapshot
/// at build time (v1.4.7 sub-phase A5). The composite-runtime
/// `prior_correction.rs` + `merge.rs::correct` lambda retired at the
/// same step; corrections now live exclusively in `log_prior_q4` here,
/// applied uniformly at IDF read time by the cement IdfReader path.
///
/// Boost rationale + per-entry polish-log citations: see the v1.4.7
/// A3 commit (787b666) message and the now-deleted
/// `composite/prior_correction.rs`. Calibration: each boost is the
/// Q4 amount needed to clear the canon competitor under the Q4-log
/// additive sort key (`score_q4 = log_prior + log_likelihood`) plus a
/// small safety margin. Q4=16, so +11 ≈ ×2.0 linear, +17 ≈ ×2.9.
///
/// Keep entries sorted alphabetically by Chinese (for human review).
/// New entries MUST add a regression test in `dispatch.rs` /
/// `session.rs` pinning the expected ranking and cite the user
/// polish-log case in the same commit. Prefer small boosts (≤17);
/// anything larger suggests the corpus is fundamentally wrong about
/// the word and the dict-pipeline T0 work should address it instead.
const PRIOR_CORRECTIONS: &[(&str, i32)] = &[
    ("继续", 17),
    ("设计", 11),
    ("理想", 11),
    ("加载", 7),
    ("具体", 7),
    ("统一", 13),
];

fn correction_for(word: &str) -> i32 {
    PRIOR_CORRECTIONS
        .iter()
        .find(|(w, _)| *w == word)
        .map(|(_, b)| *b)
        .unwrap_or(0)
}

/// Build-time dict additions — `(code, word, raw_freq)` triples
/// injected into the IDF snapshot for entries the upstream
/// `inputx-pinyin` dict pipeline does not (yet) capture as native
/// phrase entries, but where polish-log evidence shows the absence
/// regresses real IME behavior.
///
/// Pattern (v1.6+ "improve, not hack" per user 2026-05-28): when a
/// composition / K-best path generates pollution that a runtime
/// blacklist would otherwise need to drop, prefer adding the
/// LEGITIMATE alternatives to the dict here. Once Path 1 exact-
/// match returns results, Path 5 K-best is gated off and the
/// pollution never generates.
///
/// Calibration: `raw_freq` chosen at the LOW end of natural
/// corpus frequency for these phrases (real corpus rarely has the
/// data; we synthesize at plausible values). The `(PINYIN_PHRASE_
/// BASE + raw_freq) * pin_mult` Path-1 score floor is enough to
/// beat any synthesized K-best fallback score (~250k).
///
/// MUST add a regression test in `dispatch.rs` pinning the
/// expected behavior + cite the polish-log case in the same
/// commit. Keep entries grouped by polish-log buffer + sorted.
const BAKED_ADDITIONS: &[(&str, &str, u64)] = &[
    // 2026-05-28 user polish-log (pianni → semantically valid variants):
    // pre-bake, `pianni` had no Path-1 exact hits — Path 5 K-best
    // composed (片) + (你) as the highest single-char freq pair, and
    // the historical blacklist had to drop 片你 explicitly. With
    // these phrase entries baked, Path-1 surfaces 骗你 / 偏你 in
    // freq order, Path 5 gates off, 片你 never generates.
    //
    // 便你 / 篇你 dropped per user 2026-05-28 follow-up: 便你 is an
    // awkward non-collocation, 篇你 isn't a Chinese phrase at all.
    // Excluded entries get suppressed by Path-5 K-best's (pian, ni)
    // zero-bigram gating, so removing them from this table is enough.
    ("pianni", "骗你", 500),
    ("pianni", "偏你", 100),
];

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
    let out = output.unwrap_or_else(|| {
        PathBuf::from("crates/inputx-pinyin-helpers/data/words.idf")
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
    let mut baked_count = 0usize;
    for (code, word, raw_freq) in &entries {
        // v1.4.7 sub-phase A5: prior_correction's Q4 boost is baked
        // into `log_prior_q4` at snapshot build time. The merge.rs
        // `correct` lambda + composite/prior_correction.rs retire in
        // the same step — corrections live in the .idf, applied
        // uniformly via the cement IdfReader path.
        //
        // Math safety under the Q4-log additive sort key: the
        // correction is now additive in the same log space as the
        // base prior (no base-vs-freq asymmetry that the v1.4.6 B1
        // attempt tripped over). raw_freq is left UNBOOSTED — it's
        // the lossless tiebreaker for same-bucket entries, not part
        // of the prior signal; boosting it would corrupt the
        // tiebreaker semantics.
        let boost = correction_for(word);
        if boost != 0 {
            baked_count += 1;
        }
        let log_prior_q4 = log_prior_from_freq(*raw_freq) + boost;
        let log_prior_i16 = clamp_to_i16(log_prior_q4);
        // raw_freq saturates into u32 — corpus frequencies don't
        // reasonably exceed 2^32-1; clamp defensively in case a future
        // pipeline emits unscaled bigram-style counts.
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
        PRIOR_CORRECTIONS.len()
    );

    // Build-time dict additions: inject synthetic phrase entries
    // for polish-log cases the upstream dict pipeline does not
    // capture natively. Path-1 exact-match will surface these and
    // gate off Path-5 K-best composition pollution.
    for (code, word, raw_freq) in BAKED_ADDITIONS {
        let log_prior_q4 = log_prior_from_freq(*raw_freq);
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
        "[idf-from-pinyin-dict] baked dict additions: {} entries",
        BAKED_ADDITIONS.len()
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
