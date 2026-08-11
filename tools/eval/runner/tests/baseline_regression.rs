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

use inputx_core::{PINYIN_DISABLE_ASSOCIATION, PINYIN_DISABLE_COMPOSE, PINYIN_DISABLE_FUZZY};
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
            let b = base[*m]
                .as_f64()
                .unwrap_or_else(|| panic!("baseline missing {m}"));
            let c = results
                .metric(m)
                .unwrap_or_else(|| panic!("results missing {m}"));
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

/// Climb-final Stage A2 (2026-06-16) — three fixture-specific MIU
/// regression gates. Floors are absolute (not ±band like the gold
/// gate above) because fixtures are small (17-30 rows) and a single
/// case flip = 3-6pp move; a band would fail on every legitimate
/// single-case polish. Floors sit a few pp below current head value
/// so single-case flips stay above floor; whole-sprint regression
/// (e.g. wire deleted) drops well below and trips the gate. Re-
/// baseline (raise floor + bump comment) when implementation lifts
/// deliberately.

/// fuzzy_miu floor: 30 cases of z↔zh / c↔ch / s↔sh / n↔l / f↔h /
/// r↔l / in↔ing / en↔eng / an↔ang fuzzy variants. Wired via
/// `compose_via_lattice_paths`'s fuzzy edges branch (CP-3.6 step-2
/// fuzzy follow-up · commit `0d10c29`).
///
/// Current head (2026-06-16): top1 6.67% / top5 20.00% / top10 20.00%.
/// Floor at 5.0% / 15.0% / 15.0%.
#[test]
fn fuzzy_miu_above_floor() {
    // Gate-guarded per pinyin_adapter.rs pattern: fuzzy wire disabled
    // (PINYIN_DISABLE_FUZZY = true, since 2026-06-28). Test revives
    // automatically when the const flips back off.
    if PINYIN_DISABLE_FUZZY {
        return;
    }
    let dir = eval_dir();
    let fuzzy = load_tsv(&dir.join("fuzzy_miu.tsv")).expect("read fuzzy_miu.tsv");
    assert!(!fuzzy.is_empty(), "fuzzy_miu.tsv should not be empty");
    let results = run_eval(&fuzzy, &[], "regression-test");
    let pairs = [
        ("gold_top1", 0.05_f64),
        ("gold_top5", 0.15_f64),
        ("gold_top10", 0.15_f64),
    ];
    for (m, floor) in pairs {
        let cur = results
            .metric(m)
            .unwrap_or_else(|| panic!("fuzzy_miu results missing {m}"));
        assert!(
            cur >= floor,
            "fuzzy_miu {m} = {:.2}% dropped below floor {:.2}% — wire regressed; re-baseline only if deliberate",
            cur * 100.0,
            floor * 100.0,
        );
    }
}

/// typo_miu floor: 30 cases of single-edit QWERTY-adjacent
/// substitution (per `keyboard_adjacency` distance ≤ 1.5). Wired via
/// `compose_via_lattice_paths`'s typo edges branch (CP-5.3 step-2 ·
/// commit `36f281d`).
///
/// Current head (2026-06-16): top1 46.67% / top5 56.67% / top10 60.00%.
/// Floor at 35.0% / 45.0% / 50.0%.
#[test]
fn typo_miu_above_floor() {
    // Gate-guarded: typo edges live behind PINYIN_DISABLE_FUZZY
    // (2-consonant-prefix + keyboard-adjacency rescue, per
    // pinyin_adapter.rs gate doc). Auto-revives when flipped off.
    if PINYIN_DISABLE_FUZZY {
        return;
    }
    let dir = eval_dir();
    let typo = load_tsv(&dir.join("typo_miu.tsv")).expect("read typo_miu.tsv");
    assert!(!typo.is_empty(), "typo_miu.tsv should not be empty");
    let results = run_eval(&typo, &[], "regression-test");
    let pairs = [
        ("gold_top1", 0.35_f64),
        ("gold_top5", 0.45_f64),
        ("gold_top10", 0.50_f64),
    ];
    for (m, floor) in pairs {
        let cur = results
            .metric(m)
            .unwrap_or_else(|| panic!("typo_miu results missing {m}"));
        assert!(
            cur >= floor,
            "typo_miu {m} = {:.2}% dropped below floor {:.2}% — wire regressed; re-baseline only if deliberate",
            cur * 100.0,
            floor * 100.0,
        );
    }
}

/// abbrev_miu floor: 17 cases of 2-letter and 3-7 letter 简拼
/// abbreviations, including climb-plan flagship `zhrmghg`. Wired via
/// `compose_via_lattice_paths`'s abbrev_resolver branch +
/// pinyin_adapter INITIALS_INDEX wrapper (CP-5.4 step-2 · commit
/// `61fdb36` + follow-up `4dc7673`).
///
/// Current head (2026-06-16): top1 23.53% / top5 64.71% / top10 88.24%.
/// Floor at 17.0% / 55.0% / 80.0%.
#[test]
fn abbrev_miu_above_floor() {
    // Gate-guarded: 简拼 abbrev lives behind PINYIN_DISABLE_ASSOCIATION
    // (short) + PINYIN_DISABLE_COMPOSE (long-abbrev resolver, e.g.
    // zhrmghg → 中华人民共和国). Auto-revives when either gate flips.
    if PINYIN_DISABLE_ASSOCIATION || PINYIN_DISABLE_COMPOSE {
        return;
    }
    let dir = eval_dir();
    let abbrev = load_tsv(&dir.join("abbrev_miu.tsv")).expect("read abbrev_miu.tsv");
    assert!(!abbrev.is_empty(), "abbrev_miu.tsv should not be empty");
    let results = run_eval(&abbrev, &[], "regression-test");
    let pairs = [
        ("gold_top1", 0.17_f64),
        ("gold_top5", 0.55_f64),
        ("gold_top10", 0.80_f64),
    ];
    for (m, floor) in pairs {
        let cur = results
            .metric(m)
            .unwrap_or_else(|| panic!("abbrev_miu results missing {m}"));
        assert!(
            cur >= floor,
            "abbrev_miu {m} = {:.2}% dropped below floor {:.2}% — wire regressed; re-baseline only if deliberate",
            cur * 100.0,
            floor * 100.0,
        );
    }
}
