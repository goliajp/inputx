//! Cross-check our encoder output against a reference dictionary.
//!
//!     cargo run --release --bin wubi-verify -- [path/to/reference.txt]
//!
//! The reference is a TSV (`<code>\t<word>`, lines starting with `#` are
//! ignored). Default path: `<repo>/data/wubi86_full.txt` (gitignored,
//! populated by `data/fetch_wubi86_rime.sh`).

use std::collections::{HashMap, HashSet};
use std::io::{self, Write};

use wubi::{embedded_seed, encode, iter_jianma1};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let ref_path = args.get(1).cloned().unwrap_or_else(default_reference_path);

    let stdout = io::stdout();
    let mut out = stdout.lock();

    let reference = match std::fs::read_to_string(&ref_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[verify] cannot read {ref_path}: {e}");
            eprintln!("[verify] run data/fetch_wubi86_rime.sh first, or pass an explicit path");
            std::process::exit(1);
        }
    };

    let (codes_for_word, words_for_code) = parse_reference(&reference);
    writeln!(
        out,
        "[verify] reference {ref_path}: {} unique words, {} unique codes",
        codes_for_word.len(),
        words_for_code.len(),
    )
    .ok();

    let mut entries: Vec<(String, String, &'static str)> = Vec::new();
    for (letter, ch) in iter_jianma1() {
        entries.push(((letter as char).to_string(), ch.to_string(), "jianma1"));
    }
    for (ch, decomp) in embedded_seed() {
        match encode(&decomp) {
            Ok(code) => entries.push((code.as_str().to_string(), ch.to_string(), "seed")),
            Err(e) => {
                writeln!(out, "  ! encode error for {ch}: {e}").ok();
            }
        }
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    let mut exact = 0;
    let mut wrong_code = 0;
    let mut not_in_ref = 0;

    writeln!(out, "\n[verify] our entries vs reference:").ok();
    for (code, word, src) in &entries {
        let ref_codes = codes_for_word.get(word.as_str());
        let verdict = match ref_codes {
            Some(codes) if codes.contains(code.as_str()) => {
                exact += 1;
                format!("✓ {src:8}")
            }
            Some(codes) => {
                wrong_code += 1;
                let alt: Vec<&str> = codes.iter().take(4).copied().collect();
                format!("✗ {src:8} (ref has: {})", alt.join(", "))
            }
            None => {
                not_in_ref += 1;
                format!("? {src:8} (word absent from reference)")
            }
        };
        writeln!(out, "  {verdict:36}  {code}\t{word}").ok();
    }

    let total = entries.len();
    writeln!(out, "\n[verify] summary").ok();
    writeln!(out, "  total:        {total}").ok();
    writeln!(out, "  ✓ exact:      {exact}").ok();
    writeln!(out, "  ✗ wrong code: {wrong_code}").ok();
    writeln!(out, "  ? not in ref: {not_in_ref}").ok();
}

fn parse_reference(src: &str) -> (HashMap<&str, HashSet<&str>>, HashMap<&str, HashSet<&str>>) {
    let mut codes_for_word: HashMap<&str, HashSet<&str>> = HashMap::new();
    let mut words_for_code: HashMap<&str, HashSet<&str>> = HashMap::new();
    for raw in src.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(2, '\t');
        let (Some(code), Some(word)) = (parts.next(), parts.next()) else {
            continue;
        };
        let code = code.trim();
        let word = word.trim();
        if code.is_empty() || word.is_empty() {
            continue;
        }
        codes_for_word.entry(word).or_default().insert(code);
        words_for_code.entry(code).or_default().insert(word);
    }
    (codes_for_word, words_for_code)
}

fn default_reference_path() -> String {
    // New repo layout: <workspace>/crates/wubi → <workspace>/data
    let manifest = env!("CARGO_MANIFEST_DIR");
    format!("{manifest}/../../data/wubi86_full.txt")
}
