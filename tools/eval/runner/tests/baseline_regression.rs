//! CP-0.7 baseline-regression gate.
//!
//! Runs the MIU eval over the committed gold set (`gold_1000.tsv`) and
//! asserts every gold metric is within ±2 percentage points of the
//! committed `results/baseline.json`. Any engine change that moves gold
//! Top-K beyond that fails CI, forcing an intentional baseline refresh.
//!
//! Gold-only by design: `silver_full.tsv` is gitignored (regenerable) and
//! absent in a fresh checkout / CI runner, so the gate keys off the 1000
//! version-controlled, LLM-audited gold rows. The gold buckets are
//! independent of silver, so the numbers match a full run exactly.
//!
//! Feature-gated (`eval`) so a plain `cargo test` stays fast; CI runs
//! `cargo test -p inputx-eval-runner --features eval`.
#![cfg(feature = "eval")]

use std::path::PathBuf;

use inputx_eval::{Results, check_regression, load_tsv, run_eval};

/// Alert threshold in fractional units (0.02 = 2 percentage points).
const THRESHOLD: f64 = 0.02;

fn eval_dir() -> PathBuf {
    // CARGO_MANIFEST_DIR = tools/eval/runner; parent = tools/eval.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("runner crate has a parent dir")
        .to_path_buf()
}

#[test]
fn gold_miu_within_threshold_of_baseline() {
    let dir = eval_dir();
    let gold = load_tsv(&dir.join("gold_1000.tsv")).expect("read gold_1000.tsv");
    assert_eq!(gold.len(), 1000, "gold set should hold exactly 1000 rows");

    let results: Results = run_eval(&gold, &[], "regression-test");

    let raw = std::fs::read_to_string(dir.join("results").join("baseline.json"))
        .expect("read results/baseline.json");
    let base: serde_json::Value = serde_json::from_str(&raw).expect("parse baseline.json");

    let pairs: Vec<(&str, f64, f64)> = ["gold_top1", "gold_top5", "gold_top10"]
        .iter()
        .map(|m| {
            let b = base[*m].as_f64().unwrap_or_else(|| panic!("baseline missing {m}"));
            let c = results.metric(m).unwrap_or_else(|| panic!("results missing {m}"));
            (*m, b, c)
        })
        .collect();

    let breaches = check_regression(&pairs, THRESHOLD);
    assert!(
        breaches.is_empty(),
        "gold MIU drifted beyond ±{:.0}pp from baseline.json — refresh the baseline if intentional: {:?}",
        THRESHOLD * 100.0,
        breaches
            .iter()
            .map(|b| format!("{} {:+.2}pp", b.metric, b.delta_pp))
            .collect::<Vec<_>>()
    );
}
