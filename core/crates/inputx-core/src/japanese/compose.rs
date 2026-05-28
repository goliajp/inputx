//! Sentence-level / multi-segment Japanese composition. Carved out
//! of the facade `inputx_nihongo::engine::compose_sentence` in
//! v1.5.1 WU-κ — same algorithm, same suffix tables, but the
//! jukugo / kanji lookups route through cement IDF readers
//! ([`crate::japanese::lookup`]) instead of the facade's `JUKUGO_TABLE`
//! / `KANJI_TABLE` const-table iteration.
//!
//! The facade copy is intentionally untouched (it's still used by
//! direct-facade consumers like `inputx-nihongo-wasm`). Suffix tables
//! are linguistic constants that rarely drift; future drift would be
//! caught by the baseline fixture diff-zero gate.

use inputx_nihongo::{Candidate, KanaKind};

use super::lookup::{lookup_jukugo_by_reading, lookup_kanji_by_reading};

/// Particle / copula suffixes for sentence-level segmentation. Longer
/// suffixes first so greedy prefix-stripping picks `dewanai` before
/// `wa`. Each entry is `(romaji_suffix, kana_form)`.
///
/// Source: `inputx_nihongo::engine::SENTENCE_SUFFIXES` (carved
/// verbatim; v1.5.1 — facade copy stays for direct-facade consumers).
pub(super) const SENTENCE_SUFFIXES: &[(&str, &str)] = &[
    // longest first
    ("dewanaikatta", "ではなかった"),
    ("dewaarimasen", "ではありません"),
    ("dewanaiyou", "ではないよう"),
    ("dewanakatta", "ではなかった"),
    ("dewanai", "ではない"),
    ("deshita", "でした"),
    ("dewashita", "ではした"),
    ("deshou", "でしょう"),
    ("darou", "だろう"),
    ("datta", "だった"),
    ("desu", "です"),
    ("dewa", "では"),
    ("kara", "から"),
    ("made", "まで"),
    ("yori", "より"),
    ("nado", "など"),
    ("toka", "とか"),
    ("nimo", "にも"),
    ("demo", "でも"),
    ("masu", "ます"),
    ("masen", "ません"),
    ("mashita", "ました"),
    ("mashou", "ましょう"),
    ("wa", "は"),
    ("ga", "が"),
    ("wo", "を"),
    ("ni", "に"),
    ("de", "で"),
    ("to", "と"),
    ("mo", "も"),
    ("no", "の"),
    ("ka", "か"),
    ("e", "へ"),
    ("ya", "や"),
];

/// Productive category-suffix kanji for "jukugo + suffix" composition
/// (東京+都 = 東京都). Whitelisted to keep the composition from
/// emitting junk like 東京渡 (渡 also reads `to`). Source: facade
/// `KANJI_SUFFIXES`.
pub(super) const KANJI_SUFFIXES: &[(&str, &str)] = &[
    ("to", "都"), ("fu", "府"), ("ken", "県"), ("shi", "市"),
    ("ku", "区"), ("chou", "町"), ("son", "村"), ("mura", "村"),
    ("shima", "島"), ("gun", "郡"), ("jin", "人"), ("go", "語"),
];

/// Single-segment compose: `(content_word, particle/copula_suffix)`.
/// Returns `(composed_word_string, content_freq)` pairs.
fn compose_one_segment(buffer: &str) -> Vec<(String, u32)> {
    let mut out: Vec<(String, u32)> = Vec::new();
    for (s_reading, s_kana) in SENTENCE_SUFFIXES {
        if let Some(prefix) = buffer.strip_suffix(s_reading) {
            if prefix.is_empty() {
                continue;
            }
            for (compound, freq) in lookup_jukugo_by_reading(prefix) {
                out.push((format!("{compound}{s_kana}"), freq));
            }
            for (ch, freq) in lookup_kanji_by_reading(prefix) {
                out.push((format!("{ch}{s_kana}"), freq));
            }
        }
    }
    out
}

/// Carved-out twin of `inputx_nihongo::engine::compose_sentence`.
/// Returns sentence candidates from 1-segment + 2-segment + bare-
/// content-tail composition paths, deduplicated by word with the
/// best (highest) freq retained, freq-desc sorted, truncated to 30.
///
/// Final 0.85 penalty matches the facade so direct-jukugo whole-
/// buffer matches still win freq ties against composed multi-segment
/// products.
pub(super) fn compose_sentence(buffer: &str) -> Vec<Candidate> {
    let mut hits: Vec<(String, u32)> = Vec::new();

    // 1-segment
    for (word, freq) in compose_one_segment(buffer) {
        hits.push((word, freq));
    }

    // jukugo + category-suffix kanji (東京+都 = 東京都).
    for (sfx_read, sfx_kanji) in KANJI_SUFFIXES {
        if let Some(prefix) = buffer.strip_suffix(sfx_read) {
            if prefix.is_empty() {
                continue;
            }
            for (compound, freq) in lookup_jukugo_by_reading(prefix) {
                hits.push((format!("{compound}{sfx_kanji}"), freq));
            }
        }
    }

    // 1-segment WITH bare content tail (no final particle/copula).
    // Handles "watashinonihon": (私+の) + 日本 where the right half is
    // a bare content word, no suffix.
    for split in 2..buffer.len() {
        let left = &buffer[..split];
        let right = &buffer[split..];
        let lefts = compose_one_segment(left);
        if lefts.is_empty() {
            continue;
        }
        let mut right_hits: Vec<(String, u32)> = Vec::new();
        for (compound, freq) in lookup_jukugo_by_reading(right) {
            right_hits.push((compound, freq));
        }
        for (ch, freq) in lookup_kanji_by_reading(right) {
            right_hits.push((ch.to_string(), freq));
        }
        for (lw, lf) in &lefts {
            for (rw, rf) in &right_hits {
                let combined = format!("{lw}{rw}");
                let combined_freq = ((*lf.min(rf) as f64) * 0.65) as u32;
                hits.push((combined, combined_freq));
            }
        }
    }

    // 2-segment: walk every split point, both halves 1-seg compose.
    if buffer.len() >= 4 {
        for split in 2..buffer.len() - 1 {
            let left = &buffer[..split];
            let right = &buffer[split..];
            let lefts = compose_one_segment(left);
            if lefts.is_empty() {
                continue;
            }
            let rights = compose_one_segment(right);
            if rights.is_empty() {
                continue;
            }
            for (lw, lf) in &lefts {
                for (rw, rf) in &rights {
                    let combined = format!("{lw}{rw}");
                    let combined_freq =
                        ((*lf.min(rf) as f64) * 0.7) as u32;
                    hits.push((combined, combined_freq));
                }
            }
        }
    }

    // Dedup by word, keeping highest freq.
    let mut best: std::collections::HashMap<String, u32> =
        std::collections::HashMap::new();
    for (w, f) in hits {
        let entry = best.entry(w).or_insert(0);
        if f > *entry {
            *entry = f;
        }
    }
    let mut sorted: Vec<(String, u32)> = best.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    sorted.truncate(30);
    sorted
        .into_iter()
        .map(|(word, freq)| Candidate {
            word,
            kind: KanaKind::Kanji,
            freq: ((freq as f64) * 0.85) as u32,
            composed: true,
            proximity_milli: 1000,
        })
        .collect()
}
