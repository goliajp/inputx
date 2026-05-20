//! Compose phrase pinyin readings from char-level readings.
//!
//! Reads:
//!   data/readings_unihan.tsv     (char → [pinyin1, pinyin2, ...])
//!   data/phrases_jieba.tsv       (phrase → freq)
//!   data/pypinyin_phrases.json   (phrase → [[reading_per_char], ...], pypinyin MIT)
//!
//! Writes:
//!   data/phrases_composed.tsv (pinyin_string → phrase, freq)
//!
//! Algorithm (revised 2026-05-11):
//!   For each jieba phrase:
//!     1. **pypinyin override** — if pypinyin's phrases_dict.json has an
//!        entry, use only those exact per-char readings (single canonical
//!        variant; or small inner cartesian if a char's reading list has
//!        > 1 entry, but in practice this is almost always 1). pypinyin
//!        is hand-curated for ~47k high-frequency phrases including all
//!        common heteronyms (银行 → yin+hang, 暖和 → nuǎn+huo, 着陆 → zhuó+lù,
//!        重新 → chóng+xīn, 调研 → diào+yán, 唱和 → chàng+hè), so this
//!        eliminates the wrong-reading variants the cartesian fallback
//!        used to emit.
//!     2. **Cartesian fallback** — phrase not in pypinyin: take the
//!        product over each char's Unihan readings. This is the same
//!        algorithm as before, now applied only to the long tail of
//!        ~300k phrases pypinyin doesn't cover (mostly proper nouns,
//!        4-char idioms, and rare compounds where multi-reading is uncommon
//!        anyway).
//!
//! Skips:
//!   - single-char "phrases" (already covered by readings_unihan.tsv)
//!   - phrases containing any char not in readings_unihan
//!   - non-CJK characters (numbers, latin letters in jieba dict like "AT&T")
//!
//! Cap: MAX_PER_PHRASE = 32 (cartesian path only). pypinyin path is
//! intrinsically capped by phrase length × per-char-reading-list size
//! which is typically 1.
//!
//! Run via:
//!     cargo run --features tools --bin compose-phrase-readings --release
//!
//! Attribution:
//!   pypinyin: github.com/mozillazg/python-pinyin, MIT license,
//!     copyright (c) 2016 mozillazg, 闲耘 <hotoo.cn@gmail.com>

use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

const MAX_PER_PHRASE: usize = 32;

fn main() -> std::io::Result<()> {
    let crate_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string()));
    let unihan_path = crate_dir.join("data/readings_unihan.tsv");
    let jieba_path = crate_dir.join("data/phrases_jieba.tsv");
    let pypinyin_path = crate_dir.join("data/pypinyin_phrases.json");
    let out_path = crate_dir.join("data/phrases_composed.tsv");

    let unihan_txt = fs::read_to_string(&unihan_path).unwrap_or_else(|_| {
        panic!(
            "{} missing — run tools/build_readings_unihan first (item 14)",
            unihan_path.display()
        )
    });
    let jieba_txt = fs::read_to_string(&jieba_path).unwrap_or_else(|_| {
        panic!(
            "{} missing — run tools/fetch_jieba_dict.sh first",
            jieba_path.display()
        )
    });

    // char → readings
    let mut readings: HashMap<char, Vec<String>> = HashMap::with_capacity(50_000);
    for line in unihan_txt.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split('\t');
        let Some(ch_str) = parts.next() else {
            continue;
        };
        let Some(ch) = ch_str.chars().next() else {
            continue;
        };
        let pys: Vec<String> = parts
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !pys.is_empty() {
            readings.insert(ch, pys);
        }
    }
    eprintln!(
        "loaded {} chars from {}",
        readings.len(),
        unihan_path.display()
    );

    // Load pypinyin phrases_dict — phrase → Vec<Vec<reading>> where the
    // outer Vec is per-character, inner is reading-choices-for-that-char.
    // We normalize each inner reading (strip tones, ü→v after n/l) so the
    // output format matches the Unihan-derived readings.
    let pypinyin_txt = fs::read_to_string(&pypinyin_path).unwrap_or_else(|_| {
        panic!(
            "{} missing — run tools/fetch_pypinyin_phrases.sh first",
            pypinyin_path.display()
        )
    });
    let pypinyin_raw: HashMap<String, Vec<Vec<String>>> =
        serde_json::from_str(&pypinyin_txt).expect("pypinyin_phrases.json: malformed JSON");
    let mut pypinyin_norm: HashMap<String, Vec<Vec<String>>> =
        HashMap::with_capacity(pypinyin_raw.len());
    for (phrase, char_readings) in &pypinyin_raw {
        let normalized: Vec<Vec<String>> = char_readings
            .iter()
            .map(|reading_choices| {
                reading_choices
                    .iter()
                    .map(|r| normalize_reading(r))
                    .filter(|r| !r.is_empty())
                    .collect()
            })
            .collect();
        // Skip entries where any char ended up with zero readings (shouldn't
        // happen with well-formed pypinyin data but be defensive).
        if normalized.iter().all(|rs| !rs.is_empty()) {
            pypinyin_norm.insert(phrase.clone(), normalized);
        }
    }
    eprintln!(
        "loaded {} phrases from {} (after normalization)",
        pypinyin_norm.len(),
        pypinyin_path.display()
    );

    let mut composed: Vec<(String, String, u64)> = Vec::with_capacity(800_000);
    let mut total_phrases = 0u64;
    let mut skipped_too_short = 0u64;
    let mut skipped_missing_reading = 0u64;
    let mut skipped_non_cjk = 0u64;
    let mut capped_phrases = 0u64;
    let mut pypinyin_hits = 0u64;
    let mut cartesian_phrases = 0u64;

    for line in jieba_txt.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        total_phrases += 1;
        let mut parts = line.split('\t');
        let phrase = parts.next().unwrap_or("").trim();
        let freq: u64 = parts
            .next()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(1);

        if phrase.is_empty() {
            continue;
        }

        let chars: Vec<char> = phrase.chars().collect();
        if chars.len() < 2 {
            skipped_too_short += 1;
            continue;
        }

        // Skip if any char is non-CJK (jieba dict has entries like "AT&T", "C#", "T恤").
        // Heuristic: every char must be in CJK Unified or extension blocks.
        if !chars.iter().all(is_cjk_char) {
            skipped_non_cjk += 1;
            continue;
        }

        // pypinyin override path: if the phrase is in pypinyin_norm, use
        // only those exact per-char readings (handles heteronyms correctly
        // for the ~47k pypinyin-covered phrases — banks on hand-curated
        // disambiguation rather than blind cartesian).
        let char_reading_choices: Vec<Vec<String>> = if let Some(pypy) =
            pypinyin_norm.get(phrase)
        {
            // pypinyin char-list length must match phrase char-list length
            // (it always does for well-formed entries; defend anyway).
            if pypy.len() == chars.len() {
                pypinyin_hits += 1;
                pypy.clone()
            } else {
                eprintln!(
                    "  WARN: pypinyin entry for {phrase:?} has {} char-readings but phrase has {} chars; falling back to cartesian",
                    pypy.len(),
                    chars.len()
                );
                Vec::new()
            }
        } else {
            Vec::new()
        };

        // Cartesian fallback path: pypinyin not covering → use Unihan
        // per-char readings (current behavior).
        let char_reading_choices: Vec<Vec<String>> = if char_reading_choices.is_empty() {
            cartesian_phrases += 1;
            let mut out: Vec<Vec<String>> = Vec::with_capacity(chars.len());
            let mut any_missing = false;
            for c in &chars {
                match readings.get(c) {
                    Some(rs) if !rs.is_empty() => out.push(rs.clone()),
                    _ => {
                        any_missing = true;
                        break;
                    }
                }
            }
            if any_missing {
                skipped_missing_reading += 1;
                continue;
            }
            out
        } else {
            char_reading_choices
        };

        // Per-phrase cartesian product, capped.
        let mut combos: Vec<String> = vec![String::new()];
        let mut hit_cap = false;
        for char_rs in &char_reading_choices {
            let mut next: Vec<String> = Vec::with_capacity(combos.len() * char_rs.len());
            'inner: for prefix in &combos {
                for r in char_rs.iter() {
                    let mut s = String::with_capacity(prefix.len() + r.len());
                    s.push_str(prefix);
                    s.push_str(r);
                    next.push(s);
                    if next.len() >= MAX_PER_PHRASE {
                        hit_cap = true;
                        break 'inner;
                    }
                }
            }
            combos = next;
            if hit_cap {
                break;
            }
        }
        if hit_cap {
            capped_phrases += 1;
        }

        for c in combos {
            composed.push((c, phrase.to_string(), freq));
        }
    }

    eprintln!(
        "jieba phrases: {total_phrases}\n\
         skipped (single char): {skipped_too_short}\n\
         skipped (non-CJK chars): {skipped_non_cjk}\n\
         skipped (char missing from unihan): {skipped_missing_reading}\n\
         capped phrases (>{MAX_PER_PHRASE} combos): {capped_phrases}\n\
         pypinyin hits (precise readings): {pypinyin_hits}\n\
         cartesian fallback (Unihan readings): {cartesian_phrases}\n\
         composed entries pre-sort: {}",
        composed.len()
    );

    composed.sort();

    let mut w = BufWriter::new(fs::File::create(&out_path)?);
    writeln!(
        w,
        "# phrases_composed.tsv — generated by compose_phrase_readings"
    )?;
    writeln!(w, "# format: <pinyin_string>\\t<phrase>\\t<jieba_freq>")?;
    writeln!(
        w,
        "# source: data/readings_unihan.tsv × data/phrases_jieba.tsv"
    )?;
    writeln!(
        w,
        "# generation: cartesian product of char readings, capped at {MAX_PER_PHRASE} per phrase"
    )?;
    writeln!(
        w,
        "# attribution: char readings © Unicode Inc. (Unicode License v3); phrases from jieba (MIT, © 2013 Sun Junyi)"
    )?;
    for (p, ph, f) in &composed {
        writeln!(w, "{p}\t{ph}\t{f}")?;
    }
    w.flush()?;

    eprintln!(
        "composed entries written: {}\noutput: {}",
        composed.len(),
        out_path.display()
    );
    Ok(())
}

/// Strip Mandarin tone marks and convert ü to the IME-input form (`v` after
/// n/l, `u` elsewhere). Matches the normalization in build_readings_unihan
/// so pypinyin-sourced readings join the Unihan-sourced pool in the same
/// ASCII representation.
fn normalize_reading(s: &str) -> String {
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
            }
        }
    }
    out
}

/// CJK Unified Ideographs + Extension A-G + Compatibility Ideographs.
fn is_cjk_char(c: &char) -> bool {
    let n = *c as u32;
    (0x4E00..=0x9FFF).contains(&n)         // CJK Unified
        || (0x3400..=0x4DBF).contains(&n)  // Ext A
        || (0x20000..=0x2A6DF).contains(&n) // Ext B
        || (0x2A700..=0x2B73F).contains(&n) // Ext C
        || (0x2B740..=0x2B81F).contains(&n) // Ext D
        || (0x2B820..=0x2CEAF).contains(&n) // Ext E
        || (0x2CEB0..=0x2EBEF).contains(&n) // Ext F
        || (0x30000..=0x3134F).contains(&n) // Ext G
        || (0xF900..=0xFAFF).contains(&n)   // CJK Compat
        || (0x2F800..=0x2FA1F).contains(&n) // CJK Compat Supp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cjk_detection() {
        // CJK Unified + extensions are recognized.
        assert!(is_cjk_char(&'中'));
        assert!(is_cjk_char(&'国'));
        assert!(is_cjk_char(&'恤')); // U+6064 — CJK Unified
        assert!(is_cjk_char(&'𠀀')); // U+20000 — Ext B
        // Non-CJK chars are rejected.
        assert!(!is_cjk_char(&'A'));
        assert!(!is_cjk_char(&'1'));
        assert!(!is_cjk_char(&'#'));
        assert!(!is_cjk_char(&'T'));
    }

    #[test]
    fn t_xu_phrase_skipped_via_t_not_xu() {
        // The "T恤" jieba entry is skipped during composition because 'T'
        // is non-CJK, NOT because 恤 is non-CJK.
        assert!(!is_cjk_char(&'T'));
        assert!(is_cjk_char(&'恤'));
    }

    #[test]
    fn normalize_reading_strips_tones() {
        assert_eq!(normalize_reading("nuǎn"), "nuan");
        assert_eq!(normalize_reading("huo"), "huo");
        assert_eq!(normalize_reading("zhuó"), "zhuo");
        assert_eq!(normalize_reading("lù"), "lu");
        assert_eq!(normalize_reading("chóng"), "chong");
        assert_eq!(normalize_reading("xīn"), "xin");
    }

    #[test]
    fn normalize_reading_handles_v_for_nl() {
        assert_eq!(normalize_reading("nǚ"), "nv");
        assert_eq!(normalize_reading("lǜ"), "lv");
        // u after j/q/x/y stays u
        assert_eq!(normalize_reading("ju"), "ju");
    }
}
