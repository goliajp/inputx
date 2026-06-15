//! Phase-5 CP-5.4 step-1 — length-aware channel model for 简拼 (initials
//! abbreviation) and a helper that splits a vowel-free abbreviation
//! string into its component syllable-initials.
//!
//! This module ships the scoring scaffolding the climb plan's CP-5.4
//! calls for. It is **scoring-only** — no lattice integration in step-1.
//! The production caller in [`crate::lattice::Path2Abbrev`] still uses
//! its fixed [`Path2Abbrev::DEFAULT_CHANNEL_LOG_PROB`] (= log10(0.02));
//! CP-5.4 step-2 will wire [`abbrev_channel_log_prob`] in once
//! `INITIALS_INDEX` in `inputx-core` grows the 3+-letter lookup that
//! makes long abbreviations like "zhrmghg" → 中华人民共和国 resolvable.
//!
//! # Length-aware channel cost
//!
//! Today's `Path2Abbrev` uses a single uniform `log10 P(abbrev | full)`
//! ≈ -1.699 (P = 2%). That value is calibrated for the two-letter case
//! ("zg", "bj", "sh") where many words share the same initial-letter
//! cluster and per-match confidence is genuinely low. The same cost
//! over-penalises a 7-letter abbreviation, where the initial-letter
//! sequence is far more specific and the channel "match" carries much
//! more information.
//!
//! [`abbrev_channel_log_prob`] maps `n_syllables` (the count of
//! syllable-initial cells the abbreviation decomposes into, **not** the
//! raw byte length — "zh" counts as one syllable) to a `log10 P` value
//! that is monotone non-decreasing in length:
//!
//! | n_syllables | P    | log10 P |
//! |-------------|------|--------:|
//! | 0           | n/a  | +∞      |
//! | 1           | 1%   | -2.000  |
//! | 2           | 2%   | -1.699  | ← anchored to [`super::lattice::Path2Abbrev::DEFAULT_CHANNEL_LOG_PROB`]
//! | 3           | 5%   | -1.301  |
//! | 4           | 10%  | -1.000  |
//! | 5           | 20%  | -0.699  |
//! | 6           | 33%  | -0.481  |
//! | 7 +         | 50%  | -0.301  |
//!
//! The n=2 row is the deliberate hinge: dropping the length-aware
//! channel into the lattice doesn't shift behaviour for today's
//! two-letter abbrev hits, only loosens the cost ceiling for longer
//! matches that the current code can't even surface yet.
//!
//! # Initials decomposition
//!
//! Pinyin has three compound initials — `zh`, `ch`, `sh` — that count
//! as a single syllable initial. [`split_initials`] runs a greedy
//! left-to-right pass over the input, peeling off a compound whenever
//! the next two chars are one of `z|c|s` followed by `h`, otherwise a
//! single letter.
//!
//! ```text
//! "zhrmghg" → ["zh", "r", "m", "g", "h", "g"]   // 6 syllables (中华人民共和国)
//! "zg"      → ["z", "g"]                          // 2 syllables (中国)
//! "chsh"    → ["ch", "sh"]                        // 2 compounds
//! "zsh"     → ["z", "sh"]                         // z standalone, then sh compound
//! "zhh"     → ["zh", "h"]                         // zh compound, then h single
//! ```
//!
//! Inputs containing a vowel (`a e i o u`), the special letter `v`
//! (which is never an initial), or any non-lowercase-ASCII character
//! return an empty `Vec` — the abbreviation channel speaks only to
//! vowel-free initial sequences, and partial decompositions would
//! mislead the lattice into treating malformed input as a recoverable
//! abbreviation.

/// Whether `c` is a valid Mandarin pinyin syllable-initial consonant.
///
/// The 20 letters that can appear in initial position:
/// `b p m f / d t n l / g k h / j q x / r z c s / y w`.
/// Vowels (`a e i o u`) and the placeholder `v` (used for `ü` in
/// other contexts) are deliberately excluded — they never start a
/// pinyin syllable, and rejecting them prevents
/// [`split_initials`] from confidently chopping up a string that
/// the user didn't intend as an abbreviation.
fn is_initial_letter(c: char) -> bool {
    matches!(
        c,
        'b' | 'p' | 'm' | 'f'
        | 'd' | 't' | 'n' | 'l'
        | 'g' | 'k' | 'h'
        | 'j' | 'q' | 'x'
        | 'r' | 'z' | 'c' | 's'
        | 'y' | 'w'
    )
}

/// log10 P(abbrev | full) calibrated by the abbreviation's
/// **syllable** length (after compound-initial collapsing — see
/// [`split_initials`]).
///
/// Monotone non-decreasing in `n_syllables`: longer abbreviations
/// are more specific so a successful match carries more channel
/// confidence, i.e. a less-negative cost.
///
/// `n_syllables == 0` returns [`f32::INFINITY`] — that's caller
/// error (calling with an empty decomposition), and an infinite
/// cost ensures any downstream `weight = log_prob + channel`
/// arithmetic blocks the bad edge instead of silently using a
/// junk default.
///
/// `n_syllables == 2` returns exactly `log10(0.02) ≈ -1.699` so
/// it lines up byte-for-byte with the legacy
/// [`super::lattice::Path2Abbrev::DEFAULT_CHANNEL_LOG_PROB`]. CP-5.4
/// step-2's lattice wiring can replace the const with this function
/// without shifting behaviour for any abbreviation that's already
/// matchable today.
pub fn abbrev_channel_log_prob(n_syllables: usize) -> f32 {
    match n_syllables {
        0 => f32::INFINITY,
        1 => -2.000,
        2 => -1.6989700043360187, // log10(0.02), matches Path2Abbrev legacy const
        3 => -1.301,
        4 => -1.000,
        5 => -0.699,
        6 => -0.481,
        _ => -0.301, // 7+, capped at log10(0.5)
    }
}

/// Decompose a vowel-free pinyin abbreviation into its sequence of
/// syllable-initials, preferring the compound initials `zh`, `ch`,
/// `sh` whenever they appear.
///
/// Returns an empty `Vec` for:
/// - empty input
/// - any character that isn't an ASCII lowercase letter
/// - any character that isn't a pinyin initial consonant (vowels
///   `a e i o u` and the placeholder `v` all reject)
///
/// The decomposition is greedy left-to-right: at each position
/// `i`, if `typed[i]` is one of `z | c | s` and `typed[i+1] == 'h'`,
/// we take a compound and advance by two; otherwise we take a
/// single letter and advance by one. This handles the only
/// ambiguity in the language (`zh` vs `z`+`h`) by always preferring
/// the compound, which matches how an abbreviation typer actually
/// writes "中华" as `zh` (one syllable initial) rather than as
/// `z` + `h` (two).
///
/// Cost: O(n) where n = `typed.len()`. No allocation beyond the
/// returned `Vec` (each syllable is at most 2 bytes).
pub fn split_initials(typed: &str) -> Vec<String> {
    if typed.is_empty() {
        return Vec::new();
    }
    let chars: Vec<char> = typed.chars().collect();
    for c in &chars {
        if !is_initial_letter(*c) {
            return Vec::new();
        }
    }
    let mut out: Vec<String> = Vec::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        let next_is_h = i + 1 < chars.len() && chars[i + 1] == 'h';
        if next_is_h && matches!(chars[i], 'z' | 'c' | 's') {
            let mut s = String::with_capacity(2);
            s.push(chars[i]);
            s.push('h');
            out.push(s);
            i += 2;
        } else {
            out.push(chars[i].to_string());
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- abbrev_channel_log_prob --------------------------------------

    #[test]
    fn legacy_anchor_at_len_2() {
        // n=2 must equal Path2Abbrev::DEFAULT_CHANNEL_LOG_PROB so
        // wiring this function into the lattice doesn't shift today's
        // two-letter abbrev behaviour.
        let v = abbrev_channel_log_prob(2);
        let legacy: f32 = -1.6989700043360187;
        assert!(
            (v - legacy).abs() < 1e-6,
            "n=2 should match legacy const, got {v} vs {legacy}"
        );
    }

    #[test]
    fn log_prob_monotone_non_decreasing_in_length() {
        // For n ≥ 1 the cost should be monotone non-decreasing
        // (less-negative as n grows) — longer abbreviation is more
        // specific so the channel match is more confident.
        let mut prev = abbrev_channel_log_prob(1);
        for n in 2..=10 {
            let cur = abbrev_channel_log_prob(n);
            assert!(
                cur >= prev,
                "abbrev_channel_log_prob({n}) = {cur} should be >= prev {prev}"
            );
            prev = cur;
        }
    }

    #[test]
    fn len_zero_returns_infinity() {
        // n=0 is caller error: empty decomposition. Returning +∞
        // makes any downstream weight arithmetic explicitly block
        // the edge rather than silently using a junk default.
        let v = abbrev_channel_log_prob(0);
        assert!(v.is_infinite() && v.is_sign_positive(), "n=0 → +∞, got {v}");
    }

    #[test]
    fn len_clamps_at_high_band() {
        // n=7, 8, 9, 100 should all collapse to the same ceiling so
        // an exceptionally long abbreviation doesn't get an
        // unbounded reward. The ceiling sits at log10(0.5) ≈ -0.301
        // so the cost stays a negative number — abbrev edges never
        // become "free" relative to exact edges.
        let v7 = abbrev_channel_log_prob(7);
        let v100 = abbrev_channel_log_prob(100);
        assert_eq!(v7, v100, "n=7 and n=100 should clamp to the same value");
        assert!((v7 - (-0.301)).abs() < 1e-6, "ceiling should be -0.301, got {v7}");
    }

    #[test]
    fn ladder_band_values_are_correct() {
        // Spot-check each band so future edits to the table can't
        // silently shift one row's value without breaking a test.
        assert!((abbrev_channel_log_prob(1) - (-2.000)).abs() < 1e-6);
        assert!((abbrev_channel_log_prob(3) - (-1.301)).abs() < 1e-6);
        assert!((abbrev_channel_log_prob(4) - (-1.000)).abs() < 1e-6);
        assert!((abbrev_channel_log_prob(5) - (-0.699)).abs() < 1e-6);
        assert!((abbrev_channel_log_prob(6) - (-0.481)).abs() < 1e-6);
    }

    // -- split_initials ----------------------------------------------

    #[test]
    fn split_initials_empty_inputs() {
        assert!(split_initials("").is_empty());
    }

    #[test]
    fn split_initials_zhrmghg_decomposes_into_six_syllables() {
        // The 简拼 of 中华人民共和国: ["zh","r","m","g","h","g"].
        // The character count of the phrase is 6 → exactly six
        // syllable-initial cells.
        let v = split_initials("zhrmghg");
        assert_eq!(
            v,
            vec!["zh", "r", "m", "g", "h", "g"],
            "zhrmghg should decompose into six syllables, got {v:?}"
        );
    }

    #[test]
    fn split_initials_single_letter() {
        // Single-letter abbrev is one syllable.
        assert_eq!(split_initials("z"), vec!["z"]);
        assert_eq!(split_initials("h"), vec!["h"]);
        assert_eq!(split_initials("m"), vec!["m"]);
    }

    #[test]
    fn split_initials_zsh_no_compound_at_start() {
        // 'z' followed by 's' — not h, so 'z' stays single. Then
        // 's' + 'h' forms a compound. Result: ["z", "sh"].
        assert_eq!(split_initials("zsh"), vec!["z", "sh"]);
    }

    #[test]
    fn split_initials_chsh_two_compounds() {
        // Two compounds back-to-back.
        assert_eq!(split_initials("chsh"), vec!["ch", "sh"]);
    }

    #[test]
    fn split_initials_zhh_keeps_compound_then_single() {
        // The first "zh" is greedy-claimed as a compound, leaving a
        // single 'h' afterwards.
        assert_eq!(split_initials("zhh"), vec!["zh", "h"]);
    }

    #[test]
    fn split_initials_vowel_rejects() {
        // Vowels (and v) are not initials. Any vowel anywhere in
        // the input rejects the whole string.
        assert!(split_initials("a").is_empty());
        assert!(split_initials("zhe").is_empty(), "e vowel should reject");
        assert!(split_initials("zhi").is_empty(), "i vowel should reject");
        assert!(split_initials("zo").is_empty(), "o vowel should reject");
        assert!(split_initials("nv").is_empty(), "v should reject");
    }

    #[test]
    fn split_initials_non_ascii_rejects() {
        // Uppercase, digits, punctuation, multi-byte chars all
        // reject — we only speak lowercase ASCII initials.
        assert!(split_initials("Zh").is_empty());
        assert!(split_initials("zh1").is_empty());
        assert!(split_initials("zh-rm").is_empty());
        assert!(split_initials("我").is_empty());
        assert!(split_initials(" zh").is_empty());
    }
}
