//! `pinyin-build-weights` — turn `(corpus × rules × readings.tsv)` into
//! `data/weights/weights.tsv` plus a `provenance.toml` recording the SHAs
//! of every input. This is the script half of the "stable survey":
//! bumping the output file constitutes a pinyin data version bump.
//!
//! Usage:
//!     cargo run --features tools --release --bin pinyin-build-weights
//!     cargo run --features tools --release --bin pinyin-build-weights -- verify
//!
//! Modes:
//!   (default)  Generate `weights.tsv` and `provenance.toml`.
//!   `verify`   Re-run the pipeline in-memory and diff against the existing
//!              files. Exits 0 on byte-identical match, 1 on drift. Used by
//!              CI to enforce the (manifest × rules × readings × script) →
//!              weights.tsv determinism guarantee.
//!
//! Pipeline:
//!   1. Enumerate every (pinyin, word) pair from `data/readings.tsv` (the
//!      output of item 17). Words appear once per distinct reading.
//!   2. Build a single Aho-Corasick automaton over all unique words.
//!   3. Scan each corpus once (decompressing per-format); accumulate per-
//!      word counts weighted by manifest.weight.
//!   4. Global log + min-max scale to [0, max_freq_score]. Pinyin v0.2 has
//!      no layer concept (lands in v0.3 J); a single global normalization
//!      replaces wubi's per-layer pass.
//!   5. Emit `weights.tsv` (sorted by pinyin, then word) and
//!      `provenance.toml`.
//!
//! Mirrors `wubi-build-weights` line-for-line where shape allows; the only
//! structural divergence is single-pass normalization (no layers).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use aho_corasick::{AhoCorasickBuilder, MatchKind};
use bzip2_rs::DecoderReader as BzDecoder;
use flate2::read::GzDecoder;
use serde::Deserialize;
use sha2::{Digest, Sha256};

const SCRIPT_SOURCE: &str = include_str!("build_weights.rs");

#[derive(Debug, Deserialize)]
struct Manifest {
    #[serde(default)]
    corpus: BTreeMap<String, CorpusSpec>,
}

#[derive(Debug, Deserialize)]
struct CorpusSpec {
    #[allow(dead_code)] // recorded into provenance, not used for scanning
    url: String,
    sha256: String,
    weight: f64,
    #[allow(dead_code)]
    license: String,
    #[allow(dead_code)]
    description: String,
    #[serde(default = "default_format")]
    format: String,
    #[serde(default)]
    encoding: Option<String>,
}

fn default_format() -> String {
    "plain".to_string()
}

#[derive(Debug, Deserialize)]
struct Rules {
    normalization: Normalization,
}

#[derive(Debug, Deserialize)]
struct Normalization {
    max_freq_score: u64,
    #[allow(dead_code)] // reserved — currently we use natural log via .ln()
    log_base: f64,
    min_count: u64,
    #[allow(dead_code)] // forward-compat
    aggregation: String,
}

#[derive(Debug, Clone)]
struct Entry {
    pinyin: String,
    word: String,
}

enum Mode {
    Generate,
    Verify,
}

fn parse_mode() -> Result<Mode, String> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        None => Ok(Mode::Generate),
        Some("verify") | Some("--verify") => Ok(Mode::Verify),
        Some(other) => Err(format!(
            "unknown argument `{other}` — expected (none) or `verify`"
        )),
    }
}

fn main() -> ExitCode {
    let mode = match parse_mode() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };

    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let data_dir = crate_dir.join("data");
    let manifest_path = data_dir.join("corpus/manifest.toml");
    let rules_path = crate_dir.join("tools/weights/rules.toml");
    let cache_dir = data_dir.join("corpus/cache");
    let readings_path = data_dir.join("readings.tsv");
    let weights_path = data_dir.join("weights/weights.tsv");
    let provenance_path = data_dir.join("weights/provenance.toml");

    // 1. Inputs.
    let manifest_src = match fs::read_to_string(&manifest_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: read {}: {e}", manifest_path.display());
            return ExitCode::from(1);
        }
    };
    let manifest: Manifest = match toml::from_str(&manifest_src) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: parse manifest.toml: {e}");
            return ExitCode::from(1);
        }
    };
    let rules_src = match fs::read_to_string(&rules_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: read {}: {e}", rules_path.display());
            return ExitCode::from(1);
        }
    };
    let rules: Rules = match toml::from_str(&rules_src) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: parse rules.toml: {e}");
            return ExitCode::from(1);
        }
    };
    let readings_src = match fs::read_to_string(&readings_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: read {}: {e}", readings_path.display());
            return ExitCode::from(1);
        }
    };

    // 2. Enumerate entries from readings.tsv.
    let entries = enumerate_entries(&readings_src);
    let unique_words: Vec<String> = entries
        .iter()
        .map(|e| e.word.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    eprintln!(
        "enumerated {} entries ({} unique words)",
        entries.len(),
        unique_words.len()
    );

    // 3. Scan corpora.
    let raw_counts = scan_corpora(&manifest, &cache_dir, &unique_words);
    let total_corpora = manifest.corpus.len();
    let counted_words = raw_counts.values().filter(|c| **c > 0.0).count();
    eprintln!(
        "scanned {} corpora; {}/{} words got non-zero counts",
        total_corpora,
        counted_words,
        unique_words.len()
    );

    // 4. Global log + min-max → freq_scores.
    let scored = normalize_global(&entries, &raw_counts, &rules.normalization);

    // 5. Outputs.
    let new_weights = render_weights_tsv(&scored);
    let readings_sha = sha256_str(&readings_src);
    let new_provenance = render_provenance(
        &manifest,
        &manifest_src,
        &rules_src,
        SCRIPT_SOURCE,
        &readings_sha,
        total_corpora,
        scored.len(),
    );

    match mode {
        Mode::Generate => {
            if let Err(e) = write_file(&weights_path, &new_weights) {
                eprintln!("error: write weights.tsv: {e}");
                return ExitCode::from(1);
            }
            eprintln!("wrote {} ({} rows)", weights_path.display(), scored.len());
            if let Err(e) = write_file(&provenance_path, &new_provenance) {
                eprintln!("error: write provenance.toml: {e}");
                return ExitCode::from(1);
            }
            eprintln!("wrote {}", provenance_path.display());
            ExitCode::SUCCESS
        }
        Mode::Verify => {
            let mut drifted = false;
            match diff_file(&weights_path, &new_weights) {
                Ok(true) => eprintln!("✓ {} matches", weights_path.display()),
                Ok(false) => {
                    eprintln!(
                        "✗ {} drifted from regenerated content",
                        weights_path.display()
                    );
                    drifted = true;
                }
                Err(e) => {
                    eprintln!("✗ {}: {e}", weights_path.display());
                    drifted = true;
                }
            }
            match diff_file(&provenance_path, &new_provenance) {
                Ok(true) => eprintln!("✓ {} matches", provenance_path.display()),
                Ok(false) => {
                    eprintln!(
                        "✗ {} drifted from regenerated content",
                        provenance_path.display()
                    );
                    drifted = true;
                }
                Err(e) => {
                    eprintln!("✗ {}: {e}", provenance_path.display());
                    drifted = true;
                }
            }
            if drifted {
                eprintln!(
                    "\nFix: run `cargo run --features tools --release --bin pinyin-build-weights`\n\
                     and commit the regenerated files. That bump constitutes a pinyin data\n\
                     version change."
                );
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
    }
}

fn write_file(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    fs::write(path, content).map_err(|e| format!("write {}: {e}", path.display()))
}

fn diff_file(path: &Path, expected: &str) -> Result<bool, String> {
    let actual = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    Ok(actual == expected)
}

// ----------------------------------------------------------------------
// Entry enumeration — flat traversal of readings.tsv.
// ----------------------------------------------------------------------

fn enumerate_entries(readings_src: &str) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::with_capacity(800_000);
    for raw in readings_src.lines() {
        let line = raw.trim_end_matches(['\r', '\n']);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split('\t');
        let Some(word) = parts.next() else {
            continue;
        };
        if word.is_empty() {
            continue;
        }
        for pinyin in parts {
            let pinyin = pinyin.trim();
            if pinyin.is_empty() {
                continue;
            }
            out.push(Entry {
                pinyin: pinyin.to_string(),
                word: word.to_string(),
            });
        }
    }
    out
}

// ----------------------------------------------------------------------
// Corpus scanning
// ----------------------------------------------------------------------

fn scan_corpora(
    manifest: &Manifest,
    cache_dir: &Path,
    unique_words: &[String],
) -> HashMap<String, f64> {
    if manifest.corpus.is_empty() {
        return HashMap::new();
    }
    if unique_words.is_empty() {
        return HashMap::new();
    }

    eprintln!(
        "building Aho-Corasick over {} patterns…",
        unique_words.len()
    );
    let ac = match AhoCorasickBuilder::new()
        .match_kind(MatchKind::Standard)
        .build(unique_words)
    {
        Ok(a) => a,
        Err(e) => {
            eprintln!("warning: aho-corasick build failed: {e}");
            return HashMap::new();
        }
    };

    // For freq_list dispatch we need fast membership testing; pre-build a
    // HashSet view of the dict words.
    let unique_set: HashSet<&str> = unique_words.iter().map(|s| s.as_str()).collect();

    let mut accum: HashMap<String, f64> = HashMap::new();
    for (id, spec) in &manifest.corpus {
        let path = cache_dir.join(id);
        if !path.exists() {
            eprintln!(
                "warning: {id}: cache file {} missing — run pinyin-fetch-corpus first",
                path.display()
            );
            continue;
        }
        let nonzero = match spec.format.as_str() {
            "frequency_list" | "freq_list" => match scan_freq_list(
                &path,
                spec.encoding.as_deref(),
                &unique_set,
                spec.weight,
                &mut accum,
            ) {
                Ok(n) => n,
                Err(e) => {
                    eprintln!("warning: {id}: {e}");
                    continue;
                }
            },
            _ => {
                let text = match read_corpus(&path, &spec.format) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("warning: {id}: {e}");
                        continue;
                    }
                };
                let mut local_counts = vec![0u64; unique_words.len()];
                for mat in ac.find_overlapping_iter(&text) {
                    local_counts[mat.pattern().as_usize()] += 1;
                }
                let mut nz = 0;
                for (i, c) in local_counts.iter().enumerate() {
                    if *c > 0 {
                        let entry = accum.entry(unique_words[i].clone()).or_insert(0.0);
                        *entry += (*c as f64) * spec.weight;
                        nz += 1;
                    }
                }
                nz
            }
        };
        let _ = spec.sha256.as_str(); // recorded in provenance only
        eprintln!(
            "  {id} ({}): {nonzero} non-zero word counts (weight {})",
            spec.format, spec.weight,
        );
    }
    accum
}

fn scan_freq_list(
    path: &Path,
    encoding: Option<&str>,
    unique: &HashSet<&str>,
    weight: f64,
    accum: &mut HashMap<String, f64>,
) -> Result<usize, String> {
    let src = read_text_with_encoding(path, encoding)?;
    let mut nonzero = 0usize;
    for raw in src.lines() {
        let line = raw.trim_end_matches(['\r', '\n']);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(3, '\t');
        let (Some(word), Some(count)) = (parts.next(), parts.next()) else {
            continue;
        };
        let Ok(count) = count.parse::<u64>() else {
            continue; // header line or non-numeric column 1
        };
        if unique.contains(word) {
            let entry = accum.entry(word.to_string()).or_insert(0.0);
            *entry += (count as f64) * weight;
            nonzero += 1;
        }
    }
    Ok(nonzero)
}

fn read_text_with_encoding(path: &Path, encoding: Option<&str>) -> Result<String, String> {
    let label = encoding.unwrap_or("utf-8");
    let lower = label.to_ascii_lowercase();
    if lower == "utf-8" || lower == "utf8" {
        return fs::read_to_string(path).map_err(|e| format!("read freq_list: {e}"));
    }
    let enc = encoding_rs::Encoding::for_label(lower.as_bytes()).ok_or_else(|| {
        format!("unknown encoding `{label}` (try `utf-8`, `gbk`, `gb18030`, `big5`)")
    })?;
    let bytes = fs::read(path).map_err(|e| format!("read freq_list: {e}"))?;
    let (decoded, _, had_errors) = enc.decode(&bytes);
    if had_errors {
        eprintln!(
            "warning: decoding {} as {} produced replacement chars (some malformed bytes)",
            path.display(),
            enc.name()
        );
    }
    Ok(decoded.into_owned())
}

fn read_corpus(path: &Path, format: &str) -> Result<String, String> {
    match format {
        "plain" => fs::read_to_string(path).map_err(|e| format!("read plain: {e}")),
        "gzip" | "gz" => {
            let f = fs::File::open(path).map_err(|e| format!("open gz: {e}"))?;
            let mut decoder = GzDecoder::new(f);
            let mut out = String::new();
            decoder
                .read_to_string(&mut out)
                .map_err(|e| format!("decompress gz: {e}"))?;
            Ok(out)
        }
        "bzip2" | "bz2" => {
            let f = fs::File::open(path).map_err(|e| format!("open bz2: {e}"))?;
            let mut decoder = BzDecoder::new(f);
            let mut out = String::new();
            decoder
                .read_to_string(&mut out)
                .map_err(|e| format!("decompress bz2: {e}"))?;
            Ok(out)
        }
        "tar_gz" | "tgz" => {
            let f = fs::File::open(path).map_err(|e| format!("open tar.gz: {e}"))?;
            let gz = GzDecoder::new(f);
            let mut archive = tar::Archive::new(gz);
            let mut out = String::new();
            let entries = archive.entries().map_err(|e| format!("tar entries: {e}"))?;
            let mut included = 0usize;
            let mut skipped = 0usize;
            for entry in entries {
                let mut entry = entry.map_err(|e| format!("tar entry: {e}"))?;
                let header = entry.header();
                if !header.entry_type().is_file() {
                    continue;
                }
                let entry_path = entry
                    .path()
                    .map_err(|e| format!("tar entry path: {e}"))?
                    .to_path_buf();
                let is_txt = entry_path
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(|ext| ext.eq_ignore_ascii_case("txt"))
                    .unwrap_or(false);
                if !is_txt {
                    skipped += 1;
                    continue;
                }
                let mut buf = String::new();
                entry
                    .read_to_string(&mut buf)
                    .map_err(|e| format!("read {}: {e}", entry_path.display()))?;
                out.push_str(&buf);
                if !out.ends_with('\n') {
                    out.push('\n');
                }
                included += 1;
            }
            if included == 0 {
                return Err(format!(
                    "tar.gz contained no *.txt entries (skipped {skipped} non-txt)"
                ));
            }
            Ok(out)
        }
        other => Err(format!(
            "format `{other}` not supported (try `plain`, `gzip`, `bzip2`, `tar_gz`, or `frequency_list`)"
        )),
    }
}

// ----------------------------------------------------------------------
// Normalization — global single-pass (no layers in v0.2)
// ----------------------------------------------------------------------

fn normalize_global(
    entries: &[Entry],
    counts: &HashMap<String, f64>,
    norm: &Normalization,
) -> Vec<(String, String, u64)> {
    let mut log_counts: Vec<f64> = vec![0.0; entries.len()];
    let mut max_lc: f64 = 0.0;
    for (i, e) in entries.iter().enumerate() {
        let raw = counts.get(&e.word).copied().unwrap_or(0.0);
        let lc = if raw < norm.min_count as f64 {
            0.0
        } else {
            (1.0 + raw).ln()
        };
        log_counts[i] = lc;
        if lc > max_lc {
            max_lc = lc;
        }
    }

    let cap = norm.max_freq_score as f64;
    entries
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let freq = if max_lc > 0.0 {
                ((log_counts[i] / max_lc) * cap).round() as u64
            } else {
                0
            };
            (e.pinyin.clone(), e.word.clone(), freq)
        })
        .collect()
}

// ----------------------------------------------------------------------
// Outputs
// ----------------------------------------------------------------------

fn render_weights_tsv(rows: &[(String, String, u64)]) -> String {
    let mut sorted: Vec<&(String, String, u64)> = rows.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    let mut s = String::with_capacity(rows.len() * 24);
    s.push_str("# pinyin\tword\tfreq_score\n");
    s.push_str("# generated by pinyin-build-weights — DO NOT EDIT BY HAND. Regenerate via:\n");
    s.push_str("#   cargo run --features tools --release --bin pinyin-build-weights\n");
    use std::fmt::Write as _;
    for (pinyin, word, freq) in sorted {
        let _ = writeln!(s, "{pinyin}\t{word}\t{freq}");
    }
    s
}

fn render_provenance(
    manifest: &Manifest,
    manifest_src: &str,
    rules_src: &str,
    script_src: &str,
    readings_sha: &str,
    corpora_count: usize,
    rows_count: usize,
) -> String {
    let manifest_sha = sha256_str(manifest_src);
    let rules_sha = sha256_str(rules_src);
    let script_sha = sha256_str(script_src);

    let mut s = String::new();
    s.push_str("# Pinyin weight provenance — generated by pinyin-build-weights.\n");
    s.push_str("# This file documents the exact inputs that produced the\n");
    s.push_str("# accompanying weights.tsv. Re-running the pipeline against\n");
    s.push_str("# the same SHAs MUST yield byte-identical weights.tsv.\n\n");
    s.push_str("[generator]\n");
    s.push_str(&format!("script_sha256   = \"{script_sha}\"\n"));
    s.push_str(&format!("manifest_sha256 = \"{manifest_sha}\"\n"));
    s.push_str(&format!("rules_sha256    = \"{rules_sha}\"\n"));
    s.push_str(&format!("readings_sha256 = \"{readings_sha}\"\n\n"));
    s.push_str("[output]\n");
    s.push_str(&format!("entries = {rows_count}\n"));
    s.push_str(&format!("corpora = {corpora_count}\n\n"));
    s.push_str("[corpora]\n");
    let ids: BTreeSet<&String> = manifest.corpus.keys().collect();
    for id in ids {
        let spec = &manifest.corpus[id];
        s.push_str(&format!(
            "{id}.sha256 = \"{}\"\n{id}.weight = {}\n",
            spec.sha256, spec.weight
        ));
    }
    s
}

fn sha256_str(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    let bytes = h.finalize();
    let mut out = String::with_capacity(64);
    for b in bytes.as_slice() {
        out.push(nibble((b >> 4) & 0xF));
        out.push(nibble(b & 0xF));
    }
    out
}

fn nibble(n: u8) -> char {
    match n {
        0..=9 => (b'0' + n) as char,
        _ => (b'a' + n - 10) as char,
    }
}
