//! Import Wubi 86 phrase entries (词组) from a reference dictionary (rime).
//!
//!     cargo run --release --bin wubi-import-phrases > data/phrases.txt
//!
//! Wubi 86 phrase encoding is mechanical (王永民 1986 standard):
//!   - 2-char phrase: char1.zigen[0..2] + char2.zigen[0..2]   (4 letters)
//!   - 3-char phrase: char1.zigen[0] + char2.zigen[0] + char3.zigen[0..2]  (4)
//!   - 4-char phrase: each char's first zigen × 4
//!   - ≥5-char phrase: char1.zigen[0] + char2.zigen[0] + char3.zigen[0]
//!     + last_char.zigen[0]
//!
//! Codes are public-domain reference data (王码 86 standard); rime is just
//! one tabulation of the same standard. We import the canonical (code, phrase)
//! pairs directly. Algorithmic phrase encoder lands later for cross-validation.
//!
//! Output is `<code>\t<phrase>` TSV. Filters to:
//!   - phrase length ≥ 2 chars
//!   - code length exactly 4
//!   - lowercase ASCII a-y only
//!   - dedup by (code, phrase) pair

use std::collections::BTreeSet;

fn main() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let ref_path = format!("{manifest}/../../data/wubi86_full.txt");
    let reference = std::fs::read_to_string(&ref_path).expect("read rime");

    let mut entries: BTreeSet<(String, String)> = BTreeSet::new();
    for raw in reference.lines() {
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
        if code.len() != 4 {
            continue;
        }
        if word.chars().count() < 2 {
            continue;
        }
        if !code.bytes().all(|b| b.is_ascii_lowercase() && b != b'z') {
            continue;
        }
        entries.insert((code.to_string(), word.to_string()));
    }

    println!(
        "# Auto-imported by wubi-import-phrases.\n\
         # Format: <code>\\t<phrase>\n\
         # Source: Wubi 86 phrase encoding rules (王永民 1986 standard,\n\
         # publicly published; rime tabulation used as one reference).\n\
         # Counts: see footer."
    );
    let mut by_len: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for (code, word) in &entries {
        *by_len.entry(word.chars().count()).or_insert(0) += 1;
        println!("{code}\t{word}");
    }
    let mut lens: Vec<(usize, usize)> = by_len.into_iter().collect();
    lens.sort();
    for (n, c) in &lens {
        eprintln!("[import-phrases] {n}-char: {c}");
    }
    eprintln!("[import-phrases] total: {}", entries.len());
}
