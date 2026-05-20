//! `wubi-build-weights` — turn `(corpus × rules × dict sources)` into
//! `data/weights/weights.tsv` plus a `provenance.toml` recording the SHAs of
//! every input. This is the script half of the "stable survey" — bumping
//! the output file constitutes a wubi data version bump.
//!
//! Usage:
//!     cargo run --features tools --release --bin wubi-build-weights
//!     cargo run --features tools --release --bin wubi-build-weights -- verify
//!
//! Modes:
//!   (default)  Generate `weights.tsv` and `provenance.toml`.
//!   `verify`   Re-run the pipeline in-memory and diff against the existing
//!              files. Exits 0 on byte-identical match, 1 on drift. Used by
//!              CI to enforce the (manifest × rules × script) → weights.tsv
//!              determinism guarantee.
//!
//! Pipeline:
//!   1. Enumerate every (code, word, layer) the FST will hold (mirrors
//!      build.rs's enumeration).
//!   2. Build a single Aho-Corasick automaton over all unique words.
//!   3. Scan each corpus once (decompressing per-format); accumulate per-word
//!      counts weighted by manifest.weight.
//!   4. Per-layer: log(1 + count) → min-max scale to [0, max_freq_score].
//!   5. Emit `weights.tsv` (sorted by code, word) and `provenance.toml`.

#[path = "../src/codec.rs"]
mod codec;
#[path = "../src/layer.rs"]
mod layer;

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

use codec::{DecompRef, Shape, Stroke, encode_with_lookup};
use layer::{LAYER_COUNT, Layer};

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
    /// Source byte encoding. Defaults to UTF-8. Common alternatives for
    /// older corpora: `"gb18030"`, `"gbk"`, `"big5"`, `"shift_jis"`. Looked
    /// up via `encoding_rs::Encoding::for_label`.
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
    code: String,
    word: String,
    layer: Layer,
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

    // 2. Enumerate entries.
    let entries = match enumerate_entries(&data_dir) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: enumerate dict entries: {e}");
            return ExitCode::from(1);
        }
    };
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

    // 4. Per-layer log + min-max normalization → freq_scores.
    let scored = normalize_per_layer(&entries, &raw_counts, &rules.normalization);

    // 5. Outputs.
    let new_weights = render_weights_tsv(&scored);
    let new_provenance = render_provenance(
        &manifest,
        &manifest_src,
        &rules_src,
        SCRIPT_SOURCE,
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
                    eprintln!("✗ {} drifted from regenerated content", weights_path.display());
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
                    "\nFix: run `cargo run --features tools --release --bin wubi-build-weights`\n\
                     and commit the regenerated files. That bump constitutes a wubi data\n\
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
    let actual =
        fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    Ok(actual == expected)
}

// ----------------------------------------------------------------------
// Entry enumeration — mirrors build.rs's logic (kept in sync manually).
// ----------------------------------------------------------------------

fn enumerate_entries(data_dir: &Path) -> Result<Vec<Entry>, String> {
    let zigen_src = read_required(data_dir, "zigen86.txt")?;
    let jianma1_src = read_required(data_dir, "jianma1.txt")?;
    let seed_src = read_required(data_dir, "seed.txt")?;
    let auto_src = fs::read_to_string(data_dir.join("auto_decomp.txt")).unwrap_or_default();
    let simplified_src =
        fs::read_to_string(data_dir.join("jianma_simplified.txt")).unwrap_or_default();
    let phrases_src = fs::read_to_string(data_dir.join("phrases.txt")).unwrap_or_default();

    let zigen_map = parse_zigen_map(&zigen_src);
    let jianma1_pairs = parse_jianma1_pairs(&jianma1_src);

    // Deduplicate by (code, word) keeping highest-priority layer.
    let mut by_key: HashMap<(String, String), Layer> = HashMap::new();
    let mut take = |code: String, word: String, layer: Layer| {
        by_key
            .entry((code, word))
            .and_modify(|l| {
                if (layer as u8) > (*l as u8) {
                    *l = layer;
                }
            })
            .or_insert(layer);
    };

    // Jianma1.
    for (letter, ch) in &jianma1_pairs {
        let code = (*letter as char).to_string();
        take(code, ch.to_string(), Layer::Jianma1);
    }

    // Seed (hand-curated).
    let lookup = |c: char| -> Option<u8> { zigen_map.get(&c).copied() };
    let mut seed_chars: HashSet<char> = HashSet::new();
    let mut buf = [0u8; 4];
    for (ch, zigen, strokes, shape) in parse_seed(&seed_src) {
        seed_chars.insert(ch);
        let decomp_ref = DecompRef {
            zigen: &zigen,
            strokes: &strokes,
            shape,
        };
        if let Ok(n) = encode_with_lookup(&decomp_ref, &lookup, &mut buf) {
            let code = std::str::from_utf8(&buf[..n]).unwrap_or("").to_string();
            if !code.is_empty() {
                take(code, ch.to_string(), Layer::Zigen);
            }
        }
    }

    // Auto-decomp (skip seed chars).
    for (ch, zigen, strokes, shape) in parse_seed(&auto_src) {
        if seed_chars.contains(&ch) {
            continue;
        }
        let decomp_ref = DecompRef {
            zigen: &zigen,
            strokes: &strokes,
            shape,
        };
        if let Ok(n) = encode_with_lookup(&decomp_ref, &lookup, &mut buf) {
            let code = std::str::from_utf8(&buf[..n]).unwrap_or("").to_string();
            if !code.is_empty() {
                take(code, ch.to_string(), Layer::Auto);
            }
        }
    }

    // Simplified 二级 / 三级.
    for raw in simplified_src.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(2, '\t');
        let (Some(code), Some(word)) = (parts.next(), parts.next()) else {
            continue;
        };
        let code = code.trim().to_string();
        let word = word.trim().to_string();
        if word.chars().count() != 1 {
            continue;
        }
        match code.len() {
            2 => take(code, word, Layer::Jianma2),
            3 => take(code, word, Layer::Jianma3),
            _ => {}
        }
    }

    // Phrases.
    for raw in phrases_src.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(2, '\t');
        let (Some(code), Some(phrase)) = (parts.next(), parts.next()) else {
            continue;
        };
        let code = code.trim().to_string();
        let phrase = phrase.trim().to_string();
        if code.len() != 4 || phrase.chars().count() < 2 {
            continue;
        }
        take(code, phrase, Layer::Phrase);
    }

    let mut out: Vec<Entry> = by_key
        .into_iter()
        .map(|((code, word), layer)| Entry { code, word, layer })
        .collect();
    out.sort_by(|a, b| a.code.cmp(&b.code).then(a.word.cmp(&b.word)));
    Ok(out)
}

fn read_required(data_dir: &Path, name: &str) -> Result<String, String> {
    let path = data_dir.join(name);
    fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))
}

fn parse_zigen_map(src: &str) -> HashMap<char, u8> {
    let mut map = HashMap::new();
    for raw in src.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(2, '\t');
        let (Some(letter), Some(zigen)) = (parts.next(), parts.next()) else {
            continue;
        };
        let l = match letter.trim().as_bytes() {
            [b] if b.is_ascii_alphabetic() && *b != b'z' => b.to_ascii_lowercase(),
            _ => continue,
        };
        for c in zigen.trim().chars() {
            map.insert(c, l);
        }
    }
    map
}

fn parse_jianma1_pairs(src: &str) -> Vec<(u8, char)> {
    let mut out = Vec::new();
    for raw in src.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(2, '\t');
        let (Some(letter), Some(ch)) = (parts.next(), parts.next()) else {
            continue;
        };
        let l = match letter.trim().as_bytes() {
            [b] if b.is_ascii_alphabetic() && *b != b'z' => b.to_ascii_lowercase(),
            _ => continue,
        };
        let ch = ch.trim();
        if ch.chars().count() == 1 {
            out.push((l, ch.chars().next().unwrap()));
        }
    }
    out
}

fn parse_seed(src: &str) -> Vec<(char, Vec<char>, Vec<Stroke>, Shape)> {
    let mut out = Vec::new();
    for raw in src.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(4, '\t');
        let (Some(ch), Some(zg), Some(strokes_field), Some(shape)) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let ch = ch.trim();
        if ch.chars().count() != 1 {
            continue;
        }
        let ch = ch.chars().next().unwrap();
        let zigen: Vec<char> = zg.split_whitespace().flat_map(|s| s.chars()).collect();
        if zigen.is_empty() {
            continue;
        }
        let strokes: Vec<Stroke> = strokes_field
            .split_whitespace()
            .filter_map(|s| s.parse::<u8>().ok().and_then(Stroke::from_u8))
            .collect();
        if strokes.is_empty() {
            continue;
        }
        let Ok(p) = shape.trim().parse::<u8>() else {
            continue;
        };
        let Some(shape) = Shape::from_u8(p) else {
            continue;
        };
        out.push((ch, zigen, strokes, shape));
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

    eprintln!("building Aho-Corasick over {} patterns…", unique_words.len());
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
            eprintln!("warning: {id}: cache file {} missing — run wubi-fetch-corpus first", path.display());
            continue;
        }
        let nonzero = match spec.format.as_str() {
            "frequency_list" | "freq_list" => {
                match scan_freq_list(&path, spec.encoding.as_deref(), &unique_set, spec.weight, &mut accum) {
                    Ok(n) => n,
                    Err(e) => {
                        eprintln!("warning: {id}: {e}");
                        continue;
                    }
                }
            }
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

/// Read a frequency-list corpus (TSV `word\tcount\t...`) and merge counts
/// into `accum`. Lines whose `count` field doesn't parse as `u64` are
/// silently skipped — handles SUBTLEX-CH's header line gracefully.
///
/// `encoding` selects the source byte encoding (default UTF-8). Older
/// corpora like SUBTLEX-CH (2010) ship as GB18030; pass `Some("gb18030")`
/// to decode them. Lookup uses `encoding_rs::Encoding::for_label`, so any
/// IANA-registered alias works.
///
/// Returns the number of words that contributed (after filtering against
/// the dictionary's unique-word set).
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

/// Decode bytes from a file using `encoding` (default UTF-8 if `None`).
/// Common labels: `"utf-8"`, `"gbk"`, `"gb18030"`, `"big5"`, `"shift_jis"`.
/// Unknown labels fall back to UTF-8 with a warning.
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

/// Read a corpus file into a UTF-8 String, decompressing on the fly per
/// `format`. Supported formats:
///   "plain"          — read as-is
///   "gzip" / "gz"    — single-stream gzip
///   "bzip2" / "bz2"  — single-stream bzip2 (pure Rust; no libbz2 dep)
///   "tar_gz"         — gzip then tar; concats `*.txt` entries (used by
///                      Leipzig Corpora Collection bundles)
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
            let entries = archive
                .entries()
                .map_err(|e| format!("tar entries: {e}"))?;
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
            "format `{other}` not supported (try `plain`, `gzip`, `bzip2`, or `tar_gz`)"
        )),
    }
}

// ----------------------------------------------------------------------
// Normalization
// ----------------------------------------------------------------------

fn normalize_per_layer(
    entries: &[Entry],
    counts: &HashMap<String, f64>,
    norm: &Normalization,
) -> Vec<(String, String, Layer, u64)> {
    // Collect log-counts per layer to find min/max for scaling.
    let mut per_layer: [Vec<(usize, f64)>; LAYER_COUNT] = Default::default();
    let mut log_counts: Vec<f64> = vec![0.0; entries.len()];
    for (i, e) in entries.iter().enumerate() {
        let raw = counts.get(&e.word).copied().unwrap_or(0.0);
        let lc = if raw < norm.min_count as f64 {
            0.0
        } else {
            (1.0 + raw).ln()
        };
        log_counts[i] = lc;
        per_layer[e.layer.as_index()].push((i, lc));
    }

    let mut freq_score: Vec<u64> = vec![0; entries.len()];
    for layer_entries in &per_layer {
        if layer_entries.is_empty() {
            continue;
        }
        let max_lc = layer_entries
            .iter()
            .map(|(_, lc)| *lc)
            .fold(f64::NEG_INFINITY, f64::max);
        if max_lc <= 0.0 {
            continue; // all zeros — leave freq_score = 0 for this layer
        }
        let cap = norm.max_freq_score as f64;
        for (i, lc) in layer_entries {
            freq_score[*i] = ((lc / max_lc) * cap).round() as u64;
        }
    }

    entries
        .iter()
        .enumerate()
        .map(|(i, e)| (e.code.clone(), e.word.clone(), e.layer, freq_score[i]))
        .collect()
}

// ----------------------------------------------------------------------
// Outputs
// ----------------------------------------------------------------------

fn render_weights_tsv(rows: &[(String, String, Layer, u64)]) -> String {
    let mut sorted: Vec<&(String, String, Layer, u64)> = rows.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    let mut s = String::with_capacity(rows.len() * 24);
    s.push_str("# code\tword\tlayer\tfreq_score\n");
    s.push_str("# generated by wubi-build-weights — DO NOT EDIT BY HAND. Regenerate via:\n");
    s.push_str("#   cargo run --features tools --release --bin wubi-build-weights\n");
    use std::fmt::Write as _;
    for (code, word, layer, freq) in sorted {
        let _ = writeln!(s, "{code}\t{word}\t{}\t{freq}", layer.as_u8());
    }
    s
}

fn render_provenance(
    manifest: &Manifest,
    manifest_src: &str,
    rules_src: &str,
    script_src: &str,
    corpora_count: usize,
    rows_count: usize,
) -> String {
    let manifest_sha = sha256_str(manifest_src);
    let rules_sha = sha256_str(rules_src);
    let script_sha = sha256_str(script_src);

    let mut s = String::new();
    s.push_str("# Wubi weight provenance — generated by wubi-build-weights.\n");
    s.push_str("# This file documents the exact inputs that produced the\n");
    s.push_str("# accompanying weights.tsv. Re-running the pipeline against\n");
    s.push_str("# the same SHAs MUST yield byte-identical weights.tsv.\n\n");
    s.push_str("[generator]\n");
    s.push_str(&format!("script_sha256   = \"{script_sha}\"\n"));
    s.push_str(&format!("manifest_sha256 = \"{manifest_sha}\"\n"));
    s.push_str(&format!("rules_sha256    = \"{rules_sha}\"\n\n"));
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
