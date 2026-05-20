//! Import 二级 / 三级 简码 entries from a reference dictionary (rime).
//!
//!     cargo run --release --bin golia-import-simplified > data/jianma_simplified.txt
//!
//! Wubi 86's simplified codes (二级简码 ≈ 626 chars at 2-letter codes,
//! 三级简码 thousands at 3-letter codes) are **table-defined** by 王永民's
//! 1986 standard, not algorithmic. They're public knowledge; this tool just
//! tabulates them in our format.
//!
//! Output is `<code>\t<char>` TSV. Filters to:
//!   - char in CJK Unified Ideographs (U+4E00..=U+9FFF)
//!   - code length 2 or 3
//!   - lowercase ASCII a-y (no z) only
//!   - one entry per (code, char) pair
//!
//! Note: 三级简码 are in many cases ALSO produced by our algorithmic encoder
//! when the char's longest code in rime is 3-letter. This tool emits them
//! anyway as table data for the build to dedup.

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
        if word.chars().count() != 1 {
            continue;
        }
        let ch = word.chars().next().unwrap();
        let cp = ch as u32;
        if !(0x4E00..=0x9FFF).contains(&cp) {
            continue;
        }
        if !(2..=3).contains(&code.len()) {
            continue;
        }
        if !code.bytes().all(|b| (b'a'..=b'y').contains(&b)) {
            continue;
        }
        entries.insert((code.to_string(), ch.to_string()));
    }

    println!(
        "# Auto-imported by golia-import-simplified.\n\
         # Format: <code>\\t<char>\n\
         # Source: Wubi 86 standard 简码 table (公开规范).\n\
         # Counts: see footer."
    );
    let mut n2 = 0usize;
    let mut n3 = 0usize;
    for (code, ch) in &entries {
        if code.len() == 2 {
            n2 += 1;
        } else {
            n3 += 1;
        }
        println!("{code}\t{ch}");
    }
    eprintln!(
        "[import-simplified] 2-letter: {n2}, 3-letter: {n3}, total: {}",
        entries.len()
    );
}
