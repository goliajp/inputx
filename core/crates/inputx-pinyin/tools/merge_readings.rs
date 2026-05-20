//! Merge canonical readings.tsv from per-source files.
//!
//! Inputs:
//!   data/readings_unihan.tsv      (single chars)
//!   data/phrases_composed.tsv     (multi-char phrases × cartesian readings)
//!   data/heteronyms_curated.tsv   (canonical reading bias for ~100 phrases)
//!
//! Output:
//!   data/readings.tsv             (sorted, deduplicated, conflict-resolved)
//!
//! Conflict resolution (per ROADMAP item 17):
//!   heteronyms > composed > unihan
//!
//! In practice unihan and composed live in disjoint namespaces (unihan =
//! single CJK chars; composed = phrases of length ≥ 2 that were skipped by
//! the composer), so no actual conflict between those two. The heteronym
//! filter overrides the composed reading set for listed phrases:
//! the dict will only carry the canonical reading for each curated phrase,
//! suppressing all other reading variants from the cartesian product.
//!
//! Run via:
//!     cargo run --features tools --bin merge-readings --release

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::env;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

fn main() -> std::io::Result<()> {
    let crate_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string()));
    let unihan_path = crate_dir.join("data/readings_unihan.tsv");
    let composed_path = crate_dir.join("data/phrases_composed.tsv");
    let hetero_path = crate_dir.join("data/heteronyms_curated.tsv");
    let out_path = crate_dir.join("data/readings.tsv");

    // 1. Load heteronyms (phrase → canonical_pinyin).
    let hetero_txt = fs::read_to_string(&hetero_path)
        .unwrap_or_else(|_| panic!("{} missing — run item 16 first", hetero_path.display()));
    let mut heteronyms: HashMap<String, String> = HashMap::with_capacity(200);
    for line in hetero_txt.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split('\t');
        let phrase = parts.next().unwrap_or("").trim();
        let canonical = parts.next().unwrap_or("").trim();
        if !phrase.is_empty() && !canonical.is_empty() {
            heteronyms.insert(phrase.to_string(), canonical.to_string());
        }
    }
    eprintln!(
        "loaded {} heteronym entries from {}",
        heteronyms.len(),
        hetero_path.display()
    );

    // 2. Load phrases_composed → phrase → set of pinyins.
    let composed_txt = fs::read_to_string(&composed_path)
        .unwrap_or_else(|_| panic!("{} missing — run item 15 first", composed_path.display()));
    let mut phrase_readings: HashMap<String, BTreeSet<String>> = HashMap::with_capacity(400_000);
    for line in composed_txt.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split('\t');
        let pinyin = parts.next().unwrap_or("").trim();
        let phrase = parts.next().unwrap_or("").trim();
        if pinyin.is_empty() || phrase.is_empty() {
            continue;
        }
        phrase_readings
            .entry(phrase.to_string())
            .or_default()
            .insert(pinyin.to_string());
    }
    eprintln!(
        "loaded {} unique phrases from {}",
        phrase_readings.len(),
        composed_path.display()
    );

    // 3. Apply heteronym filter — replace readings with [canonical].
    let mut suppressed_total = 0u64;
    let mut hetero_applied = 0u64;
    let mut hetero_phrase_missing = 0u64;
    for (phrase, canonical) in &heteronyms {
        match phrase_readings.get_mut(phrase) {
            Some(readings) => {
                let before = readings.len();
                readings.clear();
                readings.insert(canonical.clone());
                suppressed_total += before.saturating_sub(1) as u64;
                hetero_applied += 1;
            }
            None => {
                hetero_phrase_missing += 1;
                eprintln!(
                    "  WARN: heteronym phrase {phrase:?} not found in composed.tsv (skipping)"
                );
            }
        }
    }

    // 4. Build merged output via BTreeMap for sorted iteration.
    let mut merged: BTreeMap<String, Vec<String>> = BTreeMap::new();

    // 4a. Single chars from Unihan.
    let unihan_txt = fs::read_to_string(&unihan_path)
        .unwrap_or_else(|_| panic!("{} missing — run item 14 first", unihan_path.display()));
    let mut char_count = 0u64;
    for line in unihan_txt.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split('\t');
        let Some(ch_str) = parts.next() else {
            continue;
        };
        let pinyins: Vec<String> = parts
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if pinyins.is_empty() {
            continue;
        }
        merged.insert(ch_str.to_string(), pinyins);
        char_count += 1;
    }

    // 4b. Phrases (with heteronym filter applied).
    let mut phrase_count = 0u64;
    for (phrase, readings) in phrase_readings {
        let pys: Vec<String> = readings.into_iter().collect();
        merged.insert(phrase, pys);
        phrase_count += 1;
    }

    // 5. Write output.
    let mut w = BufWriter::new(fs::File::create(&out_path)?);
    writeln!(
        w,
        "# readings.tsv — canonical merged readings (chars + phrases)"
    )?;
    writeln!(w, "# format: <word>\\t<pinyin1>[\\t<pinyin2>...]")?;
    writeln!(w, "# sources:")?;
    writeln!(
        w,
        "#   - data/readings_unihan.tsv (single chars, Unicode License v3)"
    )?;
    writeln!(
        w,
        "#   - data/phrases_composed.tsv (jieba MIT × Unihan readings)"
    )?;
    writeln!(
        w,
        "#   - data/heteronyms_curated.tsv (canonical bias for {} phrases)",
        heteronyms.len()
    )?;
    writeln!(
        w,
        "# conflict resolution: heteronyms > composed > unihan (disjoint namespaces; no actual collision)"
    )?;
    for (word, pys) in &merged {
        write!(w, "{word}")?;
        for p in pys {
            write!(w, "\t{p}")?;
        }
        writeln!(w)?;
    }
    w.flush()?;

    eprintln!(
        "chars merged: {char_count}\n\
         phrases merged: {phrase_count}\n\
         total entries: {}\n\
         heteronyms applied: {hetero_applied} (suppressed {suppressed_total} reading variants)\n\
         heteronyms missing in composed: {hetero_phrase_missing}\n\
         output: {}",
        merged.len(),
        out_path.display()
    );

    Ok(())
}
