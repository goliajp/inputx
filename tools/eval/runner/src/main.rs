//! Phase-0 MIU-accuracy harness (climb plan CP-0.5).
//!
//! Drives the production [`PinyinAdapter`] over the gold/silver eval sets
//! the same way the macOS / iOS hosts feed keystrokes — one ASCII letter
//! at a time, non-letters (the syllable-separating spaces in the TSV)
//! ignored by `handle_letter` — then reports **Top-K MIU accuracy**: the
//! fraction of rows whose gold hanzi string appears within the engine's
//! first K candidates.
//!
//! MIU = *Maximum Input Unit* (Chen & Lee 2000; the field-standard pinyin
//! IME metric). Each eval row is one clean, punctuation-free MIU, so a
//! row's accuracy is whether the whole-sentence conversion is recovered.
//!
//! Read-only by construction: it never calls `commit_index` (which would
//! record an L0 pick), never pins, never writes engine state. The only
//! side effect is the results JSON.
//!
//! Usage:
//!   cargo run -p inputx-eval-runner --release
//!   cargo run -p inputx-eval-runner --release -- --diff tools/eval/results/baseline.json
//!   cargo run -p inputx-eval-runner --release -- --silver-limit 2000   # fast dev
//!
//! Flags:
//!   --gold PATH          gold TSV       (default: tools/eval/gold_1000.tsv)
//!   --silver PATH        silver TSV     (default: tools/eval/silver_full.tsv)
//!   --out PATH           results JSON   (default: tools/eval/results/<date>.json)
//!   --date YYYY-MM-DD    override the date used for the default --out name
//!   --silver-limit N     cap silver rows processed (default: all)
//!   --diff PATH          print a per-metric delta vs a prior results JSON

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use inputx_core::PinyinAdapter;
use serde::Serialize;

/// How deep we read the candidate list. Top-10 is the deepest K reported.
const MAX_K: usize = 10;

/// One eval row: continuous pinyin, the gold hanzi string, and the source
/// flags column (e.g. `polyphone_default`).
struct Row {
    pinyin: String,
    hanzi: String,
    is_polyphone: bool,
}

/// Running tally for one bucket of rows (overall / gold / silver / poly).
#[derive(Default, Clone, Copy)]
struct Tally {
    n: u64,
    hit1: u64,
    hit5: u64,
    hit10: u64,
}

impl Tally {
    /// Fold one row's result. `rank` is the 0-based position of the gold
    /// string in the candidate list, or `None` if absent within MAX_K.
    fn add(&mut self, rank: Option<usize>) {
        self.n += 1;
        if let Some(r) = rank {
            if r < 1 {
                self.hit1 += 1;
            }
            if r < 5 {
                self.hit5 += 1;
            }
            if r < 10 {
                self.hit10 += 1;
            }
        }
    }

    fn top1(&self) -> f64 {
        ratio(self.hit1, self.n)
    }
    fn top5(&self) -> f64 {
        ratio(self.hit5, self.n)
    }
    fn top10(&self) -> f64 {
        ratio(self.hit10, self.n)
    }
}

fn ratio(num: u64, den: u64) -> f64 {
    if den == 0 { 0.0 } else { num as f64 / den as f64 }
}

/// The serialized results document. The six board-mandated fields
/// (`top1`/`top5`/`top10`/`gold_top1`/`silver_top1`/`per_polyphone_top1`)
/// are flat `[0,1]` floats; the rest is provenance + the remaining K cuts.
/// Methodology stamp baked into every results file so a baseline stays
/// self-explanatory when a future session diffs against it.
const NOTE: &str = "MIU accuracy = exact full-string match of the gold hanzi \
within the engine's top-K candidate list, driven through the production \
PinyinAdapter one keystroke at a time. Top-5/10 collapse toward Top-1 because \
the engine exposes no n-best sentence list. Measured against the engine AS \
COMPILED: as of v1.4.0 the COMPOSE/ASSOCIATION/FUZZY/PREDICTION pipeline gates \
are hard-disabled consts (minimal-debug state) — only literal-syllable lookup \
+ FST prefix completion survive, so multi-syllable sentence rows almost always \
miss. This is the intended pre-Phase-1 floor; flipping the COMPOSE gate is \
Phase-1 work and is expected to lift these numbers.";

#[derive(Serialize)]
struct Results {
    date: String,
    engine_version: String,
    note: String,
    elapsed_secs: f64,
    counts: Counts,

    // Overall (gold ∪ silver).
    top1: f64,
    top5: f64,
    top10: f64,

    // Gold subset (1000 LLM-audited rows).
    gold_top1: f64,
    gold_top5: f64,
    gold_top10: f64,

    // Silver subset (pypinyin-labelled, un-audited).
    silver_top1: f64,
    silver_top5: f64,
    silver_top10: f64,

    // Polyphone-flagged rows across both sets.
    per_polyphone_top1: f64,
    per_polyphone_top5: f64,
    per_polyphone_top10: f64,
}

#[derive(Serialize)]
struct Counts {
    total: u64,
    gold: u64,
    silver: u64,
    polyphone: u64,
}

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

    // One adapter, reused across every row. `clear_all` resets the buffer
    // between rows; `refresh_candidates` (run on each keystroke) wipes all
    // speculative state, so there is no cross-row leakage.
    let started = Instant::now();
    let mut adapter = PinyinAdapter::new();
    adapter.warmup();

    let mut overall = Tally::default();
    let mut gold_t = Tally::default();
    let mut silver_t = Tally::default();
    let mut poly_t = Tally::default();

    let total = gold.len() + silver.len();
    let mut done = 0usize;
    for (rows, bucket) in [(&gold, &mut gold_t), (&silver, &mut silver_t)] {
        for row in rows {
            let rank = rank_of(&mut adapter, row);
            overall.add(rank);
            bucket.add(rank);
            if row.is_polyphone {
                poly_t.add(rank);
            }
            done += 1;
            if done.is_multiple_of(5000) {
                eprintln!("  {done}/{total}…");
            }
        }
    }

    let elapsed = started.elapsed().as_secs_f64();
    let results = Results {
        date: date.clone(),
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        note: NOTE.to_string(),
        elapsed_secs: (elapsed * 1000.0).round() / 1000.0,
        counts: Counts {
            total: overall.n,
            gold: gold_t.n,
            silver: silver_t.n,
            polyphone: poly_t.n,
        },
        top1: overall.top1(),
        top5: overall.top5(),
        top10: overall.top10(),
        gold_top1: gold_t.top1(),
        gold_top5: gold_t.top5(),
        gold_top10: gold_t.top10(),
        silver_top1: silver_t.top1(),
        silver_top5: silver_t.top5(),
        silver_top10: silver_t.top10(),
        per_polyphone_top1: poly_t.top1(),
        per_polyphone_top5: poly_t.top5(),
        per_polyphone_top10: poly_t.top10(),
    };

    write_results(&out_path, &results).unwrap_or_else(|e| die(&format!("write {out_path:?}: {e}")));

    print_summary(&results, &out_path);

    if let Some(base) = diff_path {
        print_diff(&base, &results);
    }
}

/// Drive the adapter for one row and return the 0-based rank of the gold
/// hanzi string in the candidate list, or `None` if absent within MAX_K.
fn rank_of(adapter: &mut PinyinAdapter, row: &Row) -> Option<usize> {
    adapter.clear_all();
    for b in row.pinyin.bytes() {
        // `handle_letter` ignores non-alphabetic bytes, so the syllable-
        // separating spaces in the TSV are dropped — the buffer becomes
        // the continuous pinyin a real user would type.
        adapter.handle_letter(b);
    }
    adapter
        .candidates()
        .iter()
        .take(MAX_K)
        .position(|cand| cand == &row.hanzi)
}

/// Parse a `pinyin<TAB>hanzi<TAB>flags…` TSV, skipping the header row.
fn load_tsv(path: &Path) -> std::io::Result<Vec<Row>> {
    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut rows = Vec::new();
    for (idx, line) in reader.lines().enumerate() {
        let line = line?;
        if idx == 0 || line.is_empty() {
            continue; // header or blank
        }
        let mut cols = line.split('\t');
        let pinyin = cols.next().unwrap_or("");
        let hanzi = cols.next().unwrap_or("");
        let flags = cols.next().unwrap_or("");
        if pinyin.is_empty() || hanzi.is_empty() {
            continue;
        }
        rows.push(Row {
            pinyin: pinyin.to_string(),
            hanzi: hanzi.to_string(),
            is_polyphone: flags.contains("polyphone"),
        });
    }
    Ok(rows)
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

/// Print a per-metric delta (in percentage points) of the current run vs a
/// prior results JSON. Only the flat numeric metric fields are compared.
fn print_diff(base_path: &Path, current: &Results) {
    let raw = match fs::read_to_string(base_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("--diff: cannot read {base_path:?}: {e}");
            return;
        }
    };
    let base: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("--diff: cannot parse {base_path:?}: {e}");
            return;
        }
    };
    let cur: serde_json::Value =
        serde_json::to_value(current).expect("results to value");

    println!("\ndiff vs {} (Δ in percentage points)", base_path.display());
    // Stable order via BTreeMap over the metric fields.
    let metrics = [
        "top1", "top5", "top10", "gold_top1", "gold_top5", "gold_top10", "silver_top1",
        "silver_top5", "silver_top10", "per_polyphone_top1", "per_polyphone_top5",
        "per_polyphone_top10",
    ];
    let mut table: BTreeMap<&str, (f64, f64)> = BTreeMap::new();
    for m in metrics {
        let b = base.get(m).and_then(serde_json::Value::as_f64);
        let c = cur.get(m).and_then(serde_json::Value::as_f64);
        if let (Some(b), Some(c)) = (b, c) {
            table.insert(m, (b, c));
        }
    }
    if let Some(d) = base.get("date").and_then(serde_json::Value::as_str) {
        println!("  baseline date: {d}");
    }
    for (m, (b, c)) in table {
        let delta = (c - b) * 100.0;
        let sign = if delta >= 0.0 { "+" } else { "" };
        println!("  {m:<20}  {:>6.2}% → {:>6.2}%   {sign}{delta:.2} pp", b * 100.0, c * 100.0);
    }
}

/// `tools/eval/` — the runner crate's parent-of-parent. Resolved from the
/// compile-time manifest dir so defaults work regardless of CWD.
fn default_eval_dir() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")); // tools/eval/runner
    manifest
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or(manifest)
}

/// Current UTC date as `YYYY-MM-DD`, no external crate. Uses Howard
/// Hinnant's days→civil algorithm on the Unix epoch day count.
fn today_utc() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Howard Hinnant's `civil_from_days`: day index (0 = 1970-01-01) → (y,m,d).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn print_help() {
    eprintln!(
        "inputx-eval-runner — Phase-0 MIU accuracy harness\n\
         \n\
         flags:\n\
         \x20 --gold PATH         gold TSV (default tools/eval/gold_1000.tsv)\n\
         \x20 --silver PATH       silver TSV (default tools/eval/silver_full.tsv)\n\
         \x20 --out PATH          results JSON (default tools/eval/results/<date>.json)\n\
         \x20 --date YYYY-MM-DD   override date for the default --out name\n\
         \x20 --silver-limit N    cap silver rows (default: all)\n\
         \x20 --diff PATH         print per-metric delta vs a prior results JSON"
    );
}

fn die(msg: &str) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(1);
}
