//! `wubi-fetch-corpus` — download every corpus declared in
//! `data/corpus/manifest.toml` into `data/corpus/cache/`, validating the
//! SHA-256 against the manifest. Re-runs are idempotent: cached files whose
//! SHA still matches are skipped.
//!
//! Usage:
//!     cargo run --features tools --release --bin wubi-fetch-corpus
//!     cargo run --features tools --release --bin wubi-fetch-corpus -- probe <url>
//!
//! Modes:
//!   (default)        Sync every manifest entry into `data/corpus/cache/`.
//!   `probe <url>`    Download `<url>` once, print its SHA-256 and a
//!                    suggested `[corpus.<id>]` block for review. Doesn't
//!                    touch the manifest. Use this when adding a new corpus
//!                    so the maintainer never has to hand-shasum.
//!
//! Exit codes:
//!     0 — sync clean / probe succeeded
//!     1 — at least one download failed or SHA mismatched
//!     2 — invalid arguments

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Deserialize)]
struct Manifest {
    #[serde(default)]
    corpus: BTreeMap<String, CorpusSpec>,
}

#[derive(Debug, Deserialize)]
struct CorpusSpec {
    url: String,
    sha256: String,
    #[allow(dead_code)] // metadata used by build_weights
    weight: f64,
    #[allow(dead_code)]
    license: String,
    #[allow(dead_code)]
    description: String,
    #[allow(dead_code)]
    #[serde(default = "default_format")]
    format: String,
}

fn default_format() -> String {
    "plain".to_string()
}

enum Mode {
    Sync,
    Probe(String),
}

fn parse_mode() -> Result<Mode, String> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        None => Ok(Mode::Sync),
        Some("probe") => match args.next() {
            Some(url) if !url.is_empty() => Ok(Mode::Probe(url)),
            _ => Err("`probe` requires a URL argument".into()),
        },
        Some(other) => Err(format!(
            "unknown subcommand `{other}` — expected (none) or `probe <url>`"
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

    if let Mode::Probe(url) = mode {
        return run_probe(&url);
    }

    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let manifest_path = crate_dir.join("data/corpus/manifest.toml");
    let cache_dir = crate_dir.join("data/corpus/cache");

    let manifest_src = match fs::read_to_string(&manifest_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read {}: {e}", manifest_path.display());
            return ExitCode::from(1);
        }
    };
    let manifest: Manifest = match toml::from_str(&manifest_src) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: cannot parse manifest.toml: {e}");
            return ExitCode::from(1);
        }
    };

    if manifest.corpus.is_empty() {
        eprintln!(
            "manifest has no [corpus.*] entries — nothing to fetch.\n\
             see {} for the schema.",
            manifest_path.display()
        );
        return ExitCode::SUCCESS;
    }

    if let Err(e) = fs::create_dir_all(&cache_dir) {
        eprintln!("error: cannot create {}: {e}", cache_dir.display());
        return ExitCode::from(1);
    }

    let mut had_error = false;
    for (id, spec) in &manifest.corpus {
        let path = cache_dir.join(id);
        match ensure_corpus(&path, spec) {
            Ok(FetchOutcome::Cached) => println!("✓ {id}: already cached + verified"),
            Ok(FetchOutcome::Fetched(n)) => println!("✓ {id}: fetched {n} bytes, sha256 ok"),
            Err(e) => {
                eprintln!("✗ {id}: {e}");
                had_error = true;
            }
        }
    }

    if had_error {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

enum FetchOutcome {
    Cached,
    Fetched(u64),
}

/// Download `url` once into a temp file, hash it, print a suggested manifest
/// stanza, and clean up. Doesn't write to the manifest — that's the
/// maintainer's job after eyeballing the output.
fn run_probe(url: &str) -> ExitCode {
    let tmpdir = std::env::temp_dir();
    let tmp = tmpdir.join(format!("wubi-probe-{}.bin", std::process::id()));

    eprintln!("downloading {url} → {} …", tmp.display());
    let n = match download(url, &tmp) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("error: {e}");
            let _ = fs::remove_file(&tmp);
            return ExitCode::from(1);
        }
    };
    let sha = match sha256_file(&tmp) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: hash {}: {e}", tmp.display());
            let _ = fs::remove_file(&tmp);
            return ExitCode::from(1);
        }
    };

    let suggested_format = guess_format_from_url(url);
    let suggested_id = guess_id_from_url(url);
    eprintln!("\n✓ {} bytes, sha256 ok\n", n);

    println!("# Suggested manifest stanza (review + paste into");
    println!("# data/corpus/manifest.toml; tweak weight/license/format):");
    println!();
    println!("[corpus.{suggested_id}]");
    println!("url         = \"{url}\"");
    println!("sha256      = \"{sha}\"");
    println!("weight      = 1.0");
    println!("license     = \"\"  # TBD — fill from the source's license file");
    println!("description = \"\"  # TBD — one-line provenance note");
    println!("format      = \"{suggested_format}\"");

    let _ = fs::remove_file(&tmp);
    ExitCode::SUCCESS
}

fn guess_format_from_url(url: &str) -> &'static str {
    let lower = url.to_ascii_lowercase();
    if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        "tar_gz"
    } else if lower.ends_with(".bz2") {
        "bzip2"
    } else if lower.ends_with(".gz") {
        "gzip"
    } else {
        "plain"
    }
}

fn guess_id_from_url(url: &str) -> String {
    // Last URL path segment, minus any extension chain. Replace non-alphanum
    // with `_` so it's a valid TOML key without quoting.
    let stem = url
        .rsplit('/')
        .find(|s| !s.is_empty())
        .unwrap_or("corpus")
        .split('.')
        .next()
        .unwrap_or("corpus");
    let mut out = String::with_capacity(stem.len());
    for c in stem.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        return "corpus".into();
    }
    if !out.starts_with(|c: char| c.is_ascii_alphabetic()) {
        out.insert(0, 'c');
    }
    out
}

fn ensure_corpus(path: &Path, spec: &CorpusSpec) -> Result<FetchOutcome, String> {
    if path.exists() {
        let sha = sha256_file(path).map_err(|e| format!("hashing {}: {e}", path.display()))?;
        if sha.eq_ignore_ascii_case(&spec.sha256) {
            return Ok(FetchOutcome::Cached);
        }
        eprintln!(
            "  cache mismatch for {}: re-downloading (expected {}, got {})",
            path.display(),
            &spec.sha256,
            &sha,
        );
    }

    let tmp = path.with_extension("part");
    let bytes = download(&spec.url, &tmp)?;
    let actual = sha256_file(&tmp).map_err(|e| format!("hashing {}: {e}", tmp.display()))?;
    if !actual.eq_ignore_ascii_case(&spec.sha256) {
        let _ = fs::remove_file(&tmp);
        return Err(format!(
            "sha256 mismatch — expected {}, got {} (download discarded)",
            spec.sha256, actual
        ));
    }
    fs::rename(&tmp, path).map_err(|e| format!("rename {} → {}: {e}", tmp.display(), path.display()))?;
    Ok(FetchOutcome::Fetched(bytes))
}

fn download(url: &str, dest: &Path) -> Result<u64, String> {
    let resp = ureq::get(url)
        .call()
        .map_err(|e| format!("GET {url}: {e}"))?;
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    let mut file = fs::File::create(dest).map_err(|e| format!("create {}: {e}", dest.display()))?;
    let mut reader = resp.into_reader();
    let n = io::copy(&mut reader, &mut file)
        .map_err(|e| format!("copy → {}: {e}", dest.display()))?;
    file.flush().ok();
    Ok(n)
}

fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(hasher.finalize().as_slice()))
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(nibble((b >> 4) & 0xF));
        s.push(nibble(b & 0xF));
    }
    s
}

fn nibble(n: u8) -> char {
    match n {
        0..=9 => (b'0' + n) as char,
        _ => (b'a' + n - 10) as char,
    }
}
