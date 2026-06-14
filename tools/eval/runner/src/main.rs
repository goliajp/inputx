//! Phase-0 MIU-accuracy harness — CLI (climb plan CP-0.5 / CP-0.7).
//!
//! Thin command-line front end over the eval core in `lib.rs`. Drives the
//! production `PinyinAdapter` over the gold/silver eval sets and reports
//! Top-K MIU accuracy. See the library crate docs for the metric.
//!
//! Usage:
//!   cargo run -p inputx-eval-runner --release
//!   cargo run -p inputx-eval-runner --release -- --diff tools/eval/results/baseline.json
//!   cargo run -p inputx-eval-runner --release -- --silver-limit 2000      # fast dev
//!   cargo run -p inputx-eval-runner --release -- \
//!       --diff tools/eval/results/baseline.json --fail-on-regression 2   # CI gate
//!
//! Flags:
//!   --gold PATH               gold TSV    (default: tools/eval/gold_1000.tsv)
//!   --silver PATH             silver TSV  (default: tools/eval/silver_full.tsv)
//!   --out PATH                results JSON (default: tools/eval/results/<date>.json)
//!   --date YYYY-MM-DD         override the date used for the default --out name
//!   --silver-limit N          cap silver rows processed (default: all)
//!   --probe "<pinyin>"        print the candidate list for one input and exit
//!   --diff PATH               print a per-metric delta vs a prior results JSON
//!   --fail-on-regression PP   with --diff, exit 1 if any metric moved > PP

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use inputx_core::PinyinAdapter;
use inputx_eval::{METRIC_FIELDS, Results, check_regression, load_tsv, run_eval, today_utc};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let eval_dir = default_eval_dir();

    let mut gold_path = eval_dir.join("gold_1000.tsv");
    let mut silver_path = eval_dir.join("silver_full.tsv");
    let mut out_path: Option<PathBuf> = None;
    let mut diff_path: Option<PathBuf> = None;
    let mut silver_limit = usize::MAX;
    let mut date_override: Option<String> = None;
    let mut probe: Option<String> = None;
    let mut fail_on_regression: Option<f64> = None;

    let mut i = 0;
    while i < args.len() {
        let need = |i: usize| -> String {
            args.get(i + 1)
                .unwrap_or_else(|| {
                    eprintln!("error: {} expects a value", args[i]);
                    std::process::exit(2);
                })
                .clone()
        };
        match args[i].as_str() {
            "--gold" => {
                gold_path = PathBuf::from(need(i));
                i += 2;
            }
            "--silver" => {
                silver_path = PathBuf::from(need(i));
                i += 2;
            }
            "--out" => {
                out_path = Some(PathBuf::from(need(i)));
                i += 2;
            }
            "--diff" => {
                diff_path = Some(PathBuf::from(need(i)));
                i += 2;
            }
            "--date" => {
                date_override = Some(need(i));
                i += 2;
            }
            "--probe" => {
                probe = Some(need(i));
                i += 2;
            }
            "--silver-limit" => {
                silver_limit = need(i).parse().unwrap_or_else(|_| {
                    eprintln!("error: --silver-limit expects an integer");
                    std::process::exit(2);
                });
                i += 2;
            }
            "--fail-on-regression" => {
                fail_on_regression = Some(need(i).parse::<f64>().unwrap_or_else(|_| {
                    eprintln!("error: --fail-on-regression expects a number (percentage points)");
                    std::process::exit(2);
                }));
                i += 2;
            }
            "-h" | "--help" => {
                print_help();
                return;
            }
            other => {
                eprintln!("error: unknown argument {other:?}");
                print_help();
                std::process::exit(2);
            }
        }
    }

    // Diagnostic: print the candidate list for one pinyin string and exit.
    if let Some(p) = probe {
        let mut adapter = PinyinAdapter::new();
        adapter.warmup();
        adapter.clear_all();
        for b in p.bytes() {
            adapter.handle_letter(b);
        }
        println!("probe {p:?} → buffer {:?}", adapter.buffer_str());
        for (idx, c) in adapter.candidates().iter().take(20).enumerate() {
            println!("  [{idx}] {c}");
        }
        return;
    }

    let date = date_override.unwrap_or_else(today_utc);
    let out_path = out_path.unwrap_or_else(|| eval_dir.join("results").join(format!("{date}.json")));

    // Load both sets up front so a bad path fails before the slow warmup.
    let gold = load_tsv(&gold_path).unwrap_or_else(|e| die(&format!("gold {gold_path:?}: {e}")));
    let mut silver =
        load_tsv(&silver_path).unwrap_or_else(|e| die(&format!("silver {silver_path:?}: {e}")));
    if silver.len() > silver_limit {
        silver.truncate(silver_limit);
    }
    eprintln!(
        "loaded {} gold + {} silver rows; warming up engine…",
        gold.len(),
        silver.len()
    );

    let results = run_eval(&gold, &silver, &date);

    write_results(&out_path, &results).unwrap_or_else(|e| die(&format!("write {out_path:?}: {e}")));
    print_summary(&results, &out_path);

    if let Some(base) = diff_path {
        let exit_code = print_diff(&base, &results, fail_on_regression);
        if exit_code != 0 {
            std::process::exit(exit_code);
        }
    }
}

fn write_results(path: &Path, results: &Results) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(results).expect("results serialize");
    let mut f = fs::File::create(path)?;
    f.write_all(json.as_bytes())?;
    f.write_all(b"\n")?;
    Ok(())
}

fn print_summary(r: &Results, out_path: &Path) {
    println!("\nMIU accuracy — {} ({} rows, {:.1}s)", r.date, r.counts.total, r.elapsed_secs);
    println!("  bucket      top1     top5     top10    n");
    let line = |name: &str, t1: f64, t5: f64, t10: f64, n: u64| {
        println!("  {name:<10}  {:>6.2}%  {:>6.2}%  {:>6.2}%  {n}", t1 * 100.0, t5 * 100.0, t10 * 100.0);
    };
    line("overall", r.top1, r.top5, r.top10, r.counts.total);
    line("gold", r.gold_top1, r.gold_top5, r.gold_top10, r.counts.gold);
    line("silver", r.silver_top1, r.silver_top5, r.silver_top10, r.counts.silver);
    line("polyphone", r.per_polyphone_top1, r.per_polyphone_top5, r.per_polyphone_top10, r.counts.polyphone);
    println!("\nwrote {}", out_path.display());
}

/// Print a per-metric delta (percentage points) of the current run vs a
/// prior results JSON. When `fail_threshold` is set, return a non-zero exit
/// code if any metric moved beyond it (the CI alert gate). Returns 0 on
/// success or when only printing.
fn print_diff(base_path: &Path, current: &Results, fail_threshold: Option<f64>) -> i32 {
    let raw = match fs::read_to_string(base_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("--diff: cannot read {base_path:?}: {e}");
            return 0;
        }
    };
    let base: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("--diff: cannot parse {base_path:?}: {e}");
            return 0;
        }
    };

    println!("\ndiff vs {} (Δ in percentage points)", base_path.display());
    if let Some(d) = base.get("date").and_then(serde_json::Value::as_str) {
        println!("  baseline date: {d}");
    }
    // Stable order via BTreeMap over the shared metric field list.
    let mut table: BTreeMap<&str, (f64, f64)> = BTreeMap::new();
    for m in METRIC_FIELDS {
        let b = base.get(m).and_then(serde_json::Value::as_f64);
        let c = current.metric(m);
        if let (Some(b), Some(c)) = (b, c) {
            table.insert(m, (b, c));
        }
    }
    for (m, (b, c)) in &table {
        let delta = (c - b) * 100.0;
        let sign = if delta >= 0.0 { "+" } else { "" };
        println!("  {m:<20}  {:>6.2}% → {:>6.2}%   {sign}{delta:.2} pp", b * 100.0, c * 100.0);
    }

    let Some(threshold_pp) = fail_threshold else {
        return 0;
    };
    let pairs: Vec<(&str, f64, f64)> = table.iter().map(|(m, (b, c))| (*m, *b, *c)).collect();
    let breaches = check_regression(&pairs, threshold_pp / 100.0);
    if breaches.is_empty() {
        println!("\nregression gate OK — no metric moved more than {threshold_pp:.1} pp");
        0
    } else {
        eprintln!("\nregression gate FAILED ({threshold_pp:.1} pp threshold):");
        for b in &breaches {
            eprintln!(
                "  {:<20} {:.2}% → {:.2}%  ({:+.2} pp)",
                b.metric,
                b.baseline * 100.0,
                b.current * 100.0,
                b.delta_pp
            );
        }
        eprintln!("if intentional, re-run without --fail-on-regression and refresh baseline.json");
        1
    }
}

/// `tools/eval/` — the runner crate's parent-of-parent. Resolved from the
/// compile-time manifest dir so defaults work regardless of CWD.
fn default_eval_dir() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")); // tools/eval/runner
    manifest.parent().map(Path::to_path_buf).unwrap_or(manifest)
}

fn print_help() {
    eprintln!(
        "inputx-eval-runner — Phase-0 MIU accuracy harness\n\
         \n\
         flags:\n\
         \x20 --gold PATH               gold TSV (default tools/eval/gold_1000.tsv)\n\
         \x20 --silver PATH             silver TSV (default tools/eval/silver_full.tsv)\n\
         \x20 --out PATH                results JSON (default tools/eval/results/<date>.json)\n\
         \x20 --date YYYY-MM-DD         override date for the default --out name\n\
         \x20 --silver-limit N          cap silver rows (default: all)\n\
         \x20 --probe \"<pinyin>\"        print the candidate list for one input and exit\n\
         \x20 --diff PATH               print per-metric delta vs a prior results JSON\n\
         \x20 --fail-on-regression PP   with --diff, exit 1 if any metric moved > PP"
    );
}

fn die(msg: &str) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(1);
}
