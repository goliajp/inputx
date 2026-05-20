//! Build `data/readings_unihan.tsv` from `data/unihan/Unihan_Readings.txt`.
//!
//! Data source choice (revised 2026-05-11):
//!   - **Primary**: `kHanyuPinlu` — `《现代汉语频率词典》` readings with their
//!     occurrence counts, e.g., `行 → xíng(2943) háng(218)`. This is the only
//!     Unihan field that explicitly tracks **modern Mandarin** frequency for
//!     each reading, so it cleanly excludes古音 (e.g., 共 only has gòng here,
//!     not the 通假字 hóng) and 方言 readings.
//!   - **Fallback**: `kMandarin` — primary modern single-reading, used when
//!     the char has no kHanyuPinlu entry (mostly rare extension chars).
//!   - **Discarded**: `kHanyuPinyin` — `《汉语大词典》` field listing every
//!     historical / dialectal / surname reading (e.g., 共 here lists hóng, 和
//!     lists huó/huò/hú, etc.). Including it caused phrase composition to
//!     emit nonsensical variants like `honghe → 共和` and `gonghuo → 共和`,
//!     which polluted both full-pinyin lookup AND the 简拼 (initial-letter)
//!     fallback. The bug surfaced via lab8-ime sim testing on 2026-05-11.
//!
//! Why kHanyuPinlu over kTGHZ2013 / kXHC1983 (other modern-Mandarin sources):
//!   - kHanyuPinlu directly gives a **frequency** number per reading; the
//!     others only give 字典 page locators. Frequency is what we need for
//!     downstream weighting of multi-reading phrase combinations (planned
//!     future improvement: weight cartesian-product combinations by the
//!     product of per-char reading frequencies, so 共和→gonghe ranks far
//!     above any combinatorially-possible weird variant).
//!   - kHanyuPinlu also has **broad** coverage of common chars (~3700 most-
//!     frequent modern chars). For the long tail we fall back to kMandarin.
//!
//! Output schema unchanged — readings are still emitted in priority order
//! (highest-frequency first, then kMandarin if it adds anything new).
//!
//! Output normalization: ASCII, IME convention `v` for ü-after-n/l.
//!
//! Run via:
//!     cargo run --features tools --bin unihan-extract-readings --release
//!
//! Source data is gitignored under `data/unihan/`; refetch with
//! `tools/fetch_unihan.sh` (idempotent).
//!
//! Output schema (UTF-8, tab-delimited, sorted by codepoint):
//!     # comment lines starting with '#' (header + provenance)
//!     <char>\t<pinyin1>[\t<pinyin2> …]
//!
//! Coverage target (per workspace ROADMAP §14): all CJK Unified + Ext A.
//! Actual coverage from Unicode 17.0.0 Unihan: ~44k chars (also includes
//! Ext B–G partial). Single Ideographic Description Characters and
//! compatibility variants are passed through if Unihan lists them.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

fn main() -> std::io::Result<()> {
    let crate_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string()));
    let src = crate_dir.join("data/unihan/Unihan_Readings.txt");
    let dst = crate_dir.join("data/readings_unihan.tsv");

    let txt = fs::read_to_string(&src).unwrap_or_else(|_| {
        panic!(
            "{} missing — run tools/fetch_unihan.sh first",
            src.display()
        )
    });

    // Pass 1: collect kHanyuPinlu and kMandarin separately so we can emit
    // kHanyuPinlu (modern-frequency-ranked) first and use kMandarin only as
    // fallback for chars not in kHanyuPinlu. Don't read kHanyuPinyin at all
    // (古音 / 方言 pollution — see module doc).
    let mut k_pinlu: BTreeMap<char, Vec<String>> = BTreeMap::new();
    let mut k_mandarin: BTreeMap<char, Vec<String>> = BTreeMap::new();
    let mut k_pinlu_seen = 0u32;
    let mut k_mandarin_seen = 0u32;
    let mut skipped_lines = 0u32;

    for line in txt.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(3, '\t');
        let cp = parts.next().unwrap_or("");
        let field = parts.next().unwrap_or("");
        let value = parts.next().unwrap_or("");

        let Some(ch) = parse_codepoint(cp) else {
            skipped_lines += 1;
            continue;
        };

        match field {
            "kHanyuPinlu" => {
                k_pinlu_seen += 1;
                let pys = extract_pinlu(value);
                if !pys.is_empty() {
                    k_pinlu.insert(ch, pys);
                }
            }
            "kMandarin" => {
                k_mandarin_seen += 1;
                let pys: Vec<String> = value
                    .split_whitespace()
                    .map(normalize)
                    .filter(|s| !s.is_empty())
                    .collect();
                if !pys.is_empty() {
                    k_mandarin.insert(ch, pys);
                }
            }
            _ => {}
        }
    }

    // Pass 2: merge into a single output, kHanyuPinlu first (highest-freq
    // first thanks to extract_pinlu's sort), then kMandarin readings that
    // weren't already covered by Pinlu. Chars with neither field don't get
    // an entry — safer than emitting古音 or accidental combinations.
    let mut readings: BTreeMap<char, Vec<String>> = BTreeMap::new();
    let all_chars: std::collections::BTreeSet<char> =
        k_pinlu.keys().chain(k_mandarin.keys()).copied().collect();
    for ch in all_chars {
        let mut entry: Vec<String> = Vec::new();
        if let Some(pys) = k_pinlu.get(&ch) {
            for p in pys {
                if !entry.contains(p) {
                    entry.push(p.clone());
                }
            }
        }
        if let Some(pys) = k_mandarin.get(&ch) {
            for p in pys {
                if !entry.contains(p) {
                    entry.push(p.clone());
                }
            }
        }
        if !entry.is_empty() {
            readings.insert(ch, entry);
        }
    }

    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut w = BufWriter::new(fs::File::create(&dst)?);
    writeln!(
        w,
        "# readings_unihan.tsv — generated by build_readings_unihan"
    )?;
    writeln!(w, "# format: <char>\\t<pinyin1>[\\t<pinyin2> ...]")?;
    writeln!(
        w,
        "# source: Unihan_Readings.txt (kHanyuPinlu primary, kMandarin fallback)"
    )?;
    writeln!(
        w,
        "# kHanyuPinyin field intentionally NOT used — its 古音/方言 entries"
    )?;
    writeln!(
        w,
        "# polluted phrase composition (e.g., 共→hong, 和→huo) before 2026-05-11."
    )?;
    writeln!(
        w,
        "# upstream: https://www.unicode.org/Public/UCD/latest/ucd/Unihan.zip"
    )?;
    writeln!(
        w,
        "# license: Unicode License v3 — see https://www.unicode.org/license.txt"
    )?;
    writeln!(
        w,
        "# attribution: © 1991-2026 Unicode, Inc. All rights reserved."
    )?;
    writeln!(
        w,
        "# normalization: tone marks stripped; ü → v after n/l, u after j/q/x/y"
    )?;

    let mut count = 0u32;
    for (ch, pys) in &readings {
        write!(w, "{ch}")?;
        for p in pys {
            write!(w, "\t{p}")?;
        }
        writeln!(w)?;
        count += 1;
    }
    w.flush()?;

    eprintln!(
        "kHanyuPinlu entries seen: {k_pinlu_seen}\n\
         kMandarin entries seen: {k_mandarin_seen}\n\
         skipped (bad codepoint): {skipped_lines}\n\
         distinct chars written: {count}\n\
         output: {}",
        dst.display()
    );

    Ok(())
}

fn parse_codepoint(s: &str) -> Option<char> {
    let hex = s.strip_prefix("U+")?;
    let n = u32::from_str_radix(hex, 16).ok()?;
    char::from_u32(n)
}

/// kHanyuPinyin format examples (no longer used — see module doc):
///   "10011.060:bù,fǒu,fōu,fū"
///   "20811.060:háng,xìng,xíng,hàng,héng 21006.030:xíng"
/// Locator (digits + ".") before colon is dropped; comma-separated readings
/// after colon are normalized.
#[allow(dead_code)]
fn extract_hanyu_pinyin(value: &str) -> Vec<String> {
    let mut out = Vec::new();
    for chunk in value.split_whitespace() {
        let after_colon = chunk.split_once(':').map(|(_, r)| r).unwrap_or(chunk);
        for r in after_colon.split(',') {
            let n = normalize(r);
            if !n.is_empty() && !out.contains(&n) {
                out.push(n);
            }
        }
    }
    out
}

/// kHanyuPinlu format example (`行` char):
///   "xíng(2943) háng(218)"
/// Each whitespace-separated chunk is `<reading>(<freq>)`; both fields are
/// required. Returns the readings sorted by freq desc (so the most-common
/// modern reading comes first), normalized + deduplicated.
fn extract_pinlu(value: &str) -> Vec<String> {
    let mut scored: Vec<(String, u64)> = Vec::new();
    for chunk in value.split_whitespace() {
        // Find the "(...)" suffix; everything before it is the reading.
        let Some(open) = chunk.rfind('(') else {
            continue;
        };
        let Some(close) = chunk.rfind(')') else {
            continue;
        };
        if close <= open + 1 {
            continue;
        }
        let reading = &chunk[..open];
        let freq_str = &chunk[open + 1..close];
        let Ok(freq) = freq_str.parse::<u64>() else {
            continue;
        };
        let normalized = normalize(reading);
        if normalized.is_empty() {
            continue;
        }
        // Dedup: same reading appearing twice → keep the higher freq.
        if let Some(existing) = scored.iter_mut().find(|(r, _)| r == &normalized) {
            if freq > existing.1 {
                existing.1 = freq;
            }
        } else {
            scored.push((normalized, freq));
        }
    }
    scored.sort_by_key(|x| std::cmp::Reverse(x.1));
    scored.into_iter().map(|(r, _)| r).collect()
}

/// Strip Mandarin tone marks and convert ü to the IME-input form (`v` after
/// n/l, `u` elsewhere — the latter being defensive; standard pinyin
/// orthography after j/q/x/y already writes plain `u`).
///
/// Non-alphabetic chars (digits, punctuation, combining marks) are dropped.
fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'ā' | 'á' | 'ǎ' | 'à' => out.push('a'),
            'ē' | 'é' | 'ě' | 'è' | 'ê' | 'ế' | 'ề' => out.push('e'),
            'ī' | 'í' | 'ǐ' | 'ì' => out.push('i'),
            'ō' | 'ó' | 'ǒ' | 'ò' => out.push('o'),
            'ū' | 'ú' | 'ǔ' | 'ù' => out.push('u'),
            'ǖ' | 'ǘ' | 'ǚ' | 'ǜ' | 'ü' => {
                let last = out.chars().last();
                let mapped = if matches!(last, Some('n') | Some('l')) {
                    'v'
                } else {
                    'u'
                };
                out.push(mapped);
            }
            'ḿ' => out.push('m'),
            'ń' | 'ň' | 'ǹ' => out.push('n'),
            _ => {
                if c.is_ascii_alphabetic() {
                    out.push(c.to_ascii_lowercase());
                }
                // Skip everything else (digits, punctuation, combining marks).
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_tones() {
        assert_eq!(normalize("zhōng"), "zhong");
        assert_eq!(normalize("yín"), "yin");
        assert_eq!(normalize("xíng"), "xing");
        assert_eq!(normalize("bù"), "bu");
    }

    #[test]
    fn normalize_v_for_nl() {
        assert_eq!(normalize("nǚ"), "nv");
        assert_eq!(normalize("lǜ"), "lv");
        assert_eq!(normalize("lǚ"), "lv");
        assert_eq!(normalize("nǜ"), "nv");
    }

    #[test]
    fn normalize_u_for_jqxy() {
        // Unihan should already emit plain u after j/q/x/y, but defend
        // against ü-after-jqxy if it ever shows up in source data.
        assert_eq!(normalize("jü"), "ju");
        assert_eq!(normalize("xüe"), "xue");
    }

    #[test]
    fn parse_codepoint_basic() {
        assert_eq!(parse_codepoint("U+4E2D"), Some('中'));
        assert_eq!(parse_codepoint("U+4E0D"), Some('不'));
        assert_eq!(parse_codepoint("zzz"), None);
    }

    #[test]
    fn extract_hanyu_pinyin_chunks() {
        assert_eq!(
            extract_hanyu_pinyin("10011.060:bù,fǒu,fōu,fū"),
            vec!["bu", "fou", "fu"]
        );
        assert_eq!(
            extract_hanyu_pinyin("20811.060:háng,xìng,xíng 21006.030:xíng"),
            vec!["hang", "xing"]
        );
    }

    #[test]
    fn extract_pinlu_orders_by_frequency_desc() {
        // 行 → xíng(2943) háng(218): xing far more common than hang.
        assert_eq!(
            extract_pinlu("xíng(2943) háng(218)"),
            vec!["xing", "hang"]
        );
        // 长 → cháng(1179) zhǎng(1879) (in Unicode source the order may vary;
        // we always sort by freq desc).
        assert_eq!(
            extract_pinlu("zhǎng(1879) cháng(1179)"),
            vec!["zhang", "chang"]
        );
        // Single reading.
        assert_eq!(extract_pinlu("gòng(500)"), vec!["gong"]);
    }

    #[test]
    fn extract_pinlu_handles_v_for_nl() {
        assert_eq!(extract_pinlu("nǚ(50) lǜ(20)"), vec!["nv", "lv"]);
    }

    #[test]
    fn extract_pinlu_skips_malformed() {
        assert!(extract_pinlu("").is_empty());
        assert!(extract_pinlu("xíng").is_empty()); // no (freq)
        assert!(extract_pinlu("xíng(abc)").is_empty()); // freq non-numeric
        // Mixed: keep the one well-formed entry, drop the bad.
        assert_eq!(extract_pinlu("xíng(100) bad háng(50)"), vec!["xing", "hang"]);
    }
}
