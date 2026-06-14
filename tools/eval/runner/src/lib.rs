//! Phase-0 MIU-accuracy harness — shared library core (CP-0.5 / CP-0.7).
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
//! record an L0 pick), never pins, never writes engine state.
//!
//! The binary (`main.rs`) is the CLI; this lib holds the reusable eval core
//! so the CP-0.7 regression test can call it without shelling out.

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use inputx_core::PinyinAdapter;
use serde::Serialize;

/// How deep we read the candidate list. Top-10 is the deepest K reported.
pub const MAX_K: usize = 10;

/// Methodology stamp baked into every results file so a baseline stays
/// self-explanatory when a future session diffs against it.
pub const NOTE: &str = "MIU accuracy = exact full-string match of the gold hanzi \
within the engine's top-K candidate list, driven through the production \
PinyinAdapter one keystroke at a time. Top-5/10 collapse toward Top-1 because \
the engine exposes no n-best sentence list. Measured against the engine AS \
COMPILED: as of v1.4.0 the COMPOSE/ASSOCIATION/FUZZY/PREDICTION pipeline gates \
are hard-disabled consts (minimal-debug state) — only literal-syllable lookup \
+ FST prefix completion survive, so multi-syllable sentence rows almost always \
miss. This is the intended pre-Phase-1 floor; flipping the COMPOSE gate is \
Phase-1 work and is expected to lift these numbers.";

/// The flat metric field names, in stable display order. Used by `--diff`
/// and the regression check so both stay in sync with [`Results`].
pub const METRIC_FIELDS: [&str; 12] = [
    "top1", "top5", "top10", "gold_top1", "gold_top5", "gold_top10", "silver_top1", "silver_top5",
    "silver_top10", "per_polyphone_top1", "per_polyphone_top5", "per_polyphone_top10",
];

/// One eval row: continuous pinyin, the gold hanzi string, and whether the
/// source flags column marks it polyphone (e.g. `polyphone_default`).
pub struct Row {
    pub pinyin: String,
    pub hanzi: String,
    pub is_polyphone: bool,
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
#[derive(Serialize)]
pub struct Results {
    pub date: String,
    pub engine_version: String,
    pub note: String,
    pub elapsed_secs: f64,
    pub counts: Counts,

    // Overall (gold ∪ silver).
    pub top1: f64,
    pub top5: f64,
    pub top10: f64,

    // Gold subset (1000 LLM-audited rows).
    pub gold_top1: f64,
    pub gold_top5: f64,
    pub gold_top10: f64,

    // Silver subset (pypinyin-labelled, un-audited).
    pub silver_top1: f64,
    pub silver_top5: f64,
    pub silver_top10: f64,

    // Polyphone-flagged rows across both sets.
    pub per_polyphone_top1: f64,
    pub per_polyphone_top5: f64,
    pub per_polyphone_top10: f64,
}

#[derive(Serialize)]
pub struct Counts {
    pub total: u64,
    pub gold: u64,
    pub silver: u64,
    pub polyphone: u64,
}

impl Results {
    /// Look up a flat metric field by [`METRIC_FIELDS`] name.
    pub fn metric(&self, name: &str) -> Option<f64> {
        Some(match name {
            "top1" => self.top1,
            "top5" => self.top5,
            "top10" => self.top10,
            "gold_top1" => self.gold_top1,
            "gold_top5" => self.gold_top5,
            "gold_top10" => self.gold_top10,
            "silver_top1" => self.silver_top1,
            "silver_top5" => self.silver_top5,
            "silver_top10" => self.silver_top10,
            "per_polyphone_top1" => self.per_polyphone_top1,
            "per_polyphone_top5" => self.per_polyphone_top5,
            "per_polyphone_top10" => self.per_polyphone_top10,
            _ => return None,
        })
    }
}

/// Run the full eval over the given gold + silver rows, returning the
/// scored [`Results`]. Creates and warms one adapter, reused across every
/// row (`clear_all` resets the buffer between rows; `refresh_candidates`
/// wipes speculative state on each keystroke, so there is no leakage).
pub fn run_eval(gold: &[Row], silver: &[Row], date: &str) -> Results {
    let started = Instant::now();
    let mut adapter = PinyinAdapter::new();
    adapter.warmup();

    let mut overall = Tally::default();
    let mut gold_t = Tally::default();
    let mut silver_t = Tally::default();
    let mut poly_t = Tally::default();

    let total = gold.len() + silver.len();
    let mut done = 0usize;
    for (rows, bucket) in [(gold, &mut gold_t), (silver, &mut silver_t)] {
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
    Results {
        date: date.to_string(),
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
pub fn load_tsv(path: &Path) -> std::io::Result<Vec<Row>> {
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

/// One metric whose change vs baseline exceeded the alert threshold.
pub struct Regression {
    pub metric: String,
    pub baseline: f64,
    pub current: f64,
    pub delta_pp: f64,
}

/// Compare `(name, baseline, current)` metric triples and return every one
/// whose absolute change exceeds `threshold` (a fraction, e.g. 0.02 = 2pp).
/// Both directions are flagged: a baseline-locked gate treats any move past
/// the threshold — drop OR jump — as "the baseline must be refreshed".
pub fn check_regression(pairs: &[(&str, f64, f64)], threshold: f64) -> Vec<Regression> {
    pairs
        .iter()
        .filter(|(_, base, cur)| (cur - base).abs() > threshold)
        .map(|(name, base, cur)| Regression {
            metric: (*name).to_string(),
            baseline: *base,
            current: *cur,
            delta_pp: (cur - base) * 100.0,
        })
        .collect()
}

/// Current UTC date as `YYYY-MM-DD`, no external crate. Uses Howard
/// Hinnant's days→civil algorithm on the Unix epoch day count.
pub fn today_utc() -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regression_passes_within_threshold() {
        // A 0.5pp wobble is under the 2pp gate — no alert.
        let pairs = [("gold_top1", 0.4000, 0.3950)];
        assert!(check_regression(&pairs, 0.02).is_empty());
    }

    #[test]
    fn regression_fires_on_5pp_drop() {
        // The CP-0.7 acceptance scenario: a change that drops Top-1 by 5pp
        // must trip the alert.
        let pairs = [("gold_top1", 0.4000, 0.3500)];
        let breaches = check_regression(&pairs, 0.02);
        assert_eq!(breaches.len(), 1);
        assert_eq!(breaches[0].metric, "gold_top1");
        assert!((breaches[0].delta_pp + 5.0).abs() < 1e-9);
    }

    #[test]
    fn regression_fires_on_large_jump() {
        // Improvements past the gate also fire — the baseline is stale and
        // must be refreshed.
        let pairs = [("top1", 0.0073, 0.3200)];
        assert_eq!(check_regression(&pairs, 0.02).len(), 1);
    }

    #[test]
    fn civil_from_days_known_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_888), (2024, 6, 14));
    }
}
