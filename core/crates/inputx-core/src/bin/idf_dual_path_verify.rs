//! `idf-dual-path-verify` — data-integrity check for v1.4.3 .idf
//! snapshots vs live source dicts.
//!
//! For each engine (pinyin / wubi / nihongo-jukugo / nihongo-kanji):
//!   1. Open the .idf via [`IdfReader`].
//!   2. Sample 200 entries (deterministic stride: every N-th row).
//!   3. For each sample, look up the same (code, word) pair in the
//!      live source dict (PinyinDict / WubiDict / JUKUGO_TABLE /
//!      KANJI_TABLE).
//!   4. Assert (a) the (code, word) round-trips, (b) the log_prior
//!      in the .idf matches the source freq via
//!      `log_prior_from_freq(source_freq)` within ±1 Q4 (= ±1/16 log
//!      unit, the floor-rounding tolerance).
//!
//! Mismatch → exit 1 with a per-engine diff report; full match → exit
//! 0. This is the v1.4.3 → v1.4.4 trigger (c) gate.
//!
//! Note: this is NOT the v1.4.6 engine-cutover byte-rank-identical
//! check. Engine output equivalence requires the entire composite
//! scoring chain (TC demote / bigram bonus / fuzzy / 5 paths / prior_
//! correction / blacklist) to run against .idf source — that's cement-
//! layer work in v1.4.6.

use std::path::PathBuf;
use std::process::ExitCode;

use inputx_dict_format::IdfReader;
use inputx_nihongo::jukugo::JUKUGO_TABLE;
use inputx_nihongo::kanji::KANJI_TABLE;
use inputx_pinyin::PinyinDict;
use inputx_scoring::log_prior_from_freq;
use inputx_wubi::WubiDict;

const SAMPLE_COUNT: usize = 200;
/// Allow the rounded i32-Q4 value to drift by this many Q4 steps from
/// the source-derived value. ±1 covers `f64::ln`-then-round symmetry.
const Q4_TOLERANCE: i32 = 1;

// v1.4.6 sub-phase B1 was reverted at C3 step 2 (see header note in
// idf_from_pinyin_dict.rs): .idf no longer bakes the prior_correction
// multipliers into log_prior; the runtime merge.rs lambda re-applies
// them at composite merge time instead. Dual-path verify therefore
// compares against the raw source freq directly — no correction
// mirror needed here.

#[derive(Default)]
struct Mismatch {
    code: String,
    word: String,
    idf_log_prior: i16,
    src_log_prior: i32,
    reason: String,
}

fn main() -> ExitCode {
    // v1.4.7 sub-phase B: .idf files moved out of the (now-deleted)
    // monorepo-root `data/private-dict/v0.0.1/` snapshot dir into each
    // cement crate's own `data/` so cargo publish can include them
    // in the tarball. Run this binary from the `core/` workspace root.
    let pinyin_idf = PathBuf::from("crates/inputx-pinyin-helpers/data/words.idf");
    let wubi_idf = PathBuf::from("crates/inputx-wubi-data/data/words.idf");
    let nihongo_jukugo_idf = PathBuf::from("crates/inputx-nihongo-data-jukugo/data/jukugo.idf");
    let nihongo_kanji_idf = PathBuf::from("crates/inputx-nihongo-data-kanji/data/kanji.idf");
    let mut total_fail = 0usize;
    let mut total_check = 0usize;

    // ---- Pinyin ----
    eprintln!("\n[verify pinyin] {}", pinyin_idf.display());
    let reader = match IdfReader::open(&pinyin_idf) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("  open failed: {e:?}");
            return ExitCode::FAILURE;
        }
    };
    let pinyin_dict = PinyinDict::embedded();
    let all_pinyin = pinyin_dict.prefix_with_freq("");
    // Build a fast lookup table (code, word) → freq for sample matching.
    let pinyin_lookup: std::collections::HashMap<(String, String), u64> = all_pinyin
        .iter()
        .map(|(c, w, f)| ((c.clone(), w.clone()), *f))
        .collect();
    let mismatches = verify_engine(&reader, SAMPLE_COUNT, |code, word| {
        pinyin_lookup
            .get(&(code.to_string(), word.to_string()))
            .copied()
    });
    let n_check = SAMPLE_COUNT.min(reader.entry_count() as usize);
    total_check += n_check;
    total_fail += mismatches.len();
    eprintln!(
        "  checked {n_check} samples · mismatches {} · entry_count {}",
        mismatches.len(),
        reader.entry_count()
    );
    report_mismatches(&mismatches);

    // ---- Wubi ----
    eprintln!("\n[verify wubi] {}", wubi_idf.display());
    let reader = match IdfReader::open(&wubi_idf) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("  open failed: {e:?}");
            return ExitCode::FAILURE;
        }
    };
    let wubi_dict = WubiDict::embedded();
    let all_wubi = wubi_dict.all_entries();
    let wubi_lookup: std::collections::HashMap<(String, String), u64> = all_wubi
        .iter()
        .map(|(c, w, layer, freq)| {
            ((c.clone(), w.clone()), layer.base().saturating_add(*freq))
        })
        .collect();
    let mismatches = verify_engine(&reader, SAMPLE_COUNT, |code, word| {
        wubi_lookup
            .get(&(code.to_string(), word.to_string()))
            .copied()
    });
    let n_check = SAMPLE_COUNT.min(reader.entry_count() as usize);
    total_check += n_check;
    total_fail += mismatches.len();
    eprintln!(
        "  checked {n_check} samples · mismatches {} · entry_count {}",
        mismatches.len(),
        reader.entry_count()
    );
    report_mismatches(&mismatches);

    // ---- Nihongo Jukugo ----
    eprintln!("\n[verify nihongo-jukugo] {}", nihongo_jukugo_idf.display());
    let reader = match IdfReader::open(&nihongo_jukugo_idf) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("  open failed: {e:?}");
            return ExitCode::FAILURE;
        }
    };
    let jukugo_lookup: std::collections::HashMap<(String, String), u64> = JUKUGO_TABLE
        .iter()
        .map(|e| ((e.reading.to_string(), e.kanji.to_string()), e.freq as u64))
        .collect();
    let mismatches = verify_engine(&reader, SAMPLE_COUNT, |code, word| {
        jukugo_lookup
            .get(&(code.to_string(), word.to_string()))
            .copied()
    });
    let n_check = SAMPLE_COUNT.min(reader.entry_count() as usize);
    total_check += n_check;
    total_fail += mismatches.len();
    eprintln!(
        "  checked {n_check} samples · mismatches {} · entry_count {}",
        mismatches.len(),
        reader.entry_count()
    );
    report_mismatches(&mismatches);

    // ---- Nihongo Kanji ----
    eprintln!("\n[verify nihongo-kanji] {}", nihongo_kanji_idf.display());
    let reader = match IdfReader::open(&nihongo_kanji_idf) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("  open failed: {e:?}");
            return ExitCode::FAILURE;
        }
    };
    // Build (reading, kanji-as-string) → freq lookup. Each KANJI_TABLE
    // entry expands into N pairs (one per reading).
    let mut kanji_lookup: std::collections::HashMap<(String, String), u64> =
        std::collections::HashMap::new();
    for e in KANJI_TABLE {
        let mut buf = [0u8; 4];
        let k = e.kanji.encode_utf8(&mut buf).to_string();
        for r in e.readings {
            kanji_lookup.insert((r.to_string(), k.clone()), e.freq as u64);
        }
    }
    let mismatches = verify_engine(&reader, SAMPLE_COUNT, |code, word| {
        kanji_lookup
            .get(&(code.to_string(), word.to_string()))
            .copied()
    });
    let n_check = SAMPLE_COUNT.min(reader.entry_count() as usize);
    total_check += n_check;
    total_fail += mismatches.len();
    eprintln!(
        "  checked {n_check} samples · mismatches {} · entry_count {}",
        mismatches.len(),
        reader.entry_count()
    );
    report_mismatches(&mismatches);

    // ---- Summary ----
    eprintln!();
    eprintln!("=== SUMMARY ===");
    eprintln!("  total samples checked: {total_check}");
    eprintln!("  total mismatches: {total_fail}");
    if total_fail == 0 {
        eprintln!("  PASS (all 4 engines, all sampled entries within ±{Q4_TOLERANCE} Q4)");
        ExitCode::SUCCESS
    } else {
        eprintln!("  FAIL");
        ExitCode::FAILURE
    }
}

fn verify_engine<F, M>(
    reader: &IdfReader<M>,
    sample_count: usize,
    mut source_freq_for: F,
) -> Vec<Mismatch>
where
    F: FnMut(&str, &str) -> Option<u64>,
    M: AsRef<[u8]>,
{
    let total = reader.entry_count() as usize;
    if total == 0 {
        return Vec::new();
    }
    // Deterministic stride: every `stride`-th entry. Falls back to all
    // entries when the dict has fewer than `sample_count`.
    let stride = (total / sample_count.max(1)).max(1);
    let mut mismatches: Vec<Mismatch> = Vec::new();
    let mut entries = reader.entries();
    let mut idx = 0usize;
    while let Some(entry) = entries.next() {
        if idx % stride == 0 {
            let src = source_freq_for(entry.code, entry.word);
            match src {
                None => mismatches.push(Mismatch {
                    code: entry.code.to_string(),
                    word: entry.word.to_string(),
                    idf_log_prior: entry.log_prior,
                    src_log_prior: 0,
                    reason: "source dict missing (code, word) pair".to_string(),
                }),
                Some(freq) => {
                    let expected_q4 = log_prior_from_freq(freq);
                    let expected_i16 = expected_q4
                        .clamp(i16::MIN as i32, i16::MAX as i32) as i16;
                    let drift = (entry.log_prior as i32) - (expected_i16 as i32);
                    if drift.abs() > Q4_TOLERANCE {
                        mismatches.push(Mismatch {
                            code: entry.code.to_string(),
                            word: entry.word.to_string(),
                            idf_log_prior: entry.log_prior,
                            src_log_prior: expected_q4,
                            reason: format!(
                                "log_prior drift {drift} Q4 (expected ±{Q4_TOLERANCE})"
                            ),
                        });
                    }
                }
            }
        }
        idx += 1;
        if mismatches.len() >= 20 {
            // Cap output volume; first 20 mismatches per engine is enough
            // to localize the issue.
            break;
        }
    }
    mismatches
}

fn report_mismatches(ms: &[Mismatch]) {
    for m in ms.iter().take(5) {
        eprintln!(
            "    mismatch: code={:?} word={:?} idf_log_prior={} src_log_prior={} ({})",
            m.code, m.word, m.idf_log_prior, m.src_log_prior, m.reason
        );
    }
    if ms.len() > 5 {
        eprintln!("    ... and {} more", ms.len() - 5);
    }
}
