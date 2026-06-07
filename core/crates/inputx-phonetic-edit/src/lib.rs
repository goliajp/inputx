#![deny(missing_docs)]
#![forbid(unsafe_code)]
#![cfg_attr(not(feature = "std"), no_std)]

//! `inputx-phonetic-edit` — weighted phonetic edit-distance for Mandarin
//! Pinyin and other ASCII-coded phonetic systems.
//!
//! Plain Levenshtein treats every substitution as cost 1. That's wrong for
//! pinyin: a southern-Mandarin speaker who types `zongguo` for `zhongguo`
//! made a single low-confidence swap, not a one-cost typo on par with
//! `xongguo`. This crate generalizes Wagner–Fischer with a **cost table**
//! of arbitrary-length `(from, to, cost)` substitutions, then surfaces the
//! Mandarin default table covering the nine canonical fuzzy pairs.
//!
//! # Quick start
//!
//! ```
//! use inputx_phonetic_edit::{edit_distance, MANDARIN_DEFAULT};
//!
//! // Plain typo — costs 1.0 (no entry in the table)
//! assert!((edit_distance("apple", "applx", &MANDARIN_DEFAULT) - 1.0).abs() < 1e-9);
//!
//! // Southern fuzzy: zh↔z swap costs 0.3, not 1.0
//! assert!((edit_distance("zhongguo", "zongguo", &MANDARIN_DEFAULT) - 0.3).abs() < 1e-9);
//!
//! // Canonical insertion vs canonical fuzzy: in↔ing is 0.2, not 1.0
//! assert!((edit_distance("xin", "xing", &MANDARIN_DEFAULT) - 0.2).abs() < 1e-9);
//! ```
//!
//! # Properties
//!
//! - **Zero dependencies.**
//! - **`#![no_std]`** with `--no-default-features` (alloc only).
//! - **`#![forbid(unsafe_code)]`**.
//! - **Deterministic** for the same `(a, b, table)` input.
//! - Distance is `f64`; substitution costs are user-supplied (`0.0..=1.0`
//!   by convention but no upper bound is enforced).
//!
//! # Not in this crate
//!
//! - Runtime fuzzy *expansion* (generating alternate spellings) lives in
//!   the consuming engine — this crate only scores a *pair* of strings.
//!   v1.4 Inputx still uses [`inputx_pinyin`]'s in-place expansion on the
//!   hot path; this crate is the building block for post-v1.4 polish that
//!   replaces hand-tuned tier-mixing with a single distance metric.
//! - Tone-aware scoring (pinyin tone digits) — out of scope; the table
//!   sees only the segmental ASCII string.

extern crate alloc;
use alloc::vec;

/// A weighted substitution table: pairs of strings that count as a *partial*
/// edit when matched, instead of paying full per-character insert/delete cost.
///
/// Entries are bidirectional — `(a, b, cost)` covers both `a → b` and
/// `b → a`. Duplicates are tolerated; the lowest-cost match wins.
///
/// The table is `&'static`-slice-backed so it can be declared `const`
/// (see [`MANDARIN_DEFAULT`]). Build a custom one with
/// [`EditCostTable::from_pairs`] for runtime data.
#[derive(Debug, Clone, Copy)]
pub struct EditCostTable {
    /// Tuples of `(from, to, cost)` — bidirectional. Each `from` / `to` is
    /// the literal segment that gets substituted (e.g. `"zh"` and `"z"`,
    /// or `"ing"` and `"in"`).
    pub pairs: &'static [(&'static str, &'static str, f64)],
}

impl EditCostTable {
    /// An empty cost table — every substitution falls back to per-char
    /// cost 1.0. Useful as a baseline (plain Levenshtein).
    pub const EMPTY: Self = Self { pairs: &[] };

    /// Construct from a `&'static` slice of `(from, to, cost)` tuples.
    /// Both directions are searched at scoring time — you don't need
    /// to list `(a,b)` and `(b,a)` separately.
    ///
    /// ```
    /// use inputx_phonetic_edit::{EditCostTable, edit_distance};
    /// static PAIRS: &[(&str, &str, f64)] = &[
    ///     ("aa", "a", 0.1),
    /// ];
    /// const TABLE: EditCostTable = EditCostTable::from_pairs(PAIRS);
    /// assert!((edit_distance("aa", "a", &TABLE) - 0.1).abs() < 1e-9);
    /// ```
    pub const fn from_pairs(pairs: &'static [(&'static str, &'static str, f64)]) -> Self {
        Self { pairs }
    }
}

/// The canonical Mandarin Pinyin fuzzy-pair table — nine
/// southern-dialect-tolerant substitutions.
///
/// | Pair      | Cost | Class                |
/// |-----------|------|----------------------|
/// | zh ↔ z    | 0.3  | retroflex initial    |
/// | sh ↔ s    | 0.3  | retroflex initial    |
/// | ch ↔ c    | 0.3  | retroflex initial    |
/// | f ↔ h     | 0.3  | labial/glottal       |
/// | r ↔ l     | 0.3  | initial confusion    |
/// | n ↔ l     | 0.2  | initial nasal/lateral|
/// | in ↔ ing  | 0.2  | nasal final          |
/// | en ↔ eng  | 0.2  | nasal final          |
/// | an ↔ ang  | 0.2  | nasal final          |
///
/// Mirrors `inputx_pinyin::fuzzy::FuzzyConfig` — the same nine pairs the
/// Inputx Pinyin engine has shipped as the canonical Mandarin tolerance set.
pub const MANDARIN_DEFAULT: EditCostTable = EditCostTable::from_pairs(MANDARIN_PAIRS);

static MANDARIN_PAIRS: &[(&str, &str, f64)] = &[
    // retroflex initials
    ("zh", "z", 0.3),
    ("sh", "s", 0.3),
    ("ch", "c", 0.3),
    // labial / glottal
    ("f", "h", 0.3),
    // initial confusion
    ("r", "l", 0.3),
    ("n", "l", 0.2),
    // nasal finals
    ("in", "ing", 0.2),
    ("en", "eng", 0.2),
    ("an", "ang", 0.2),
];

/// Compute the weighted edit distance between `a` and `b` under `table`.
///
/// Cost model:
///
/// - **Insertion / deletion** of one byte: cost `1.0`.
/// - **Substitution** of one byte for another: cost `1.0` (or `0.0` if
///   the bytes are equal).
/// - **Multi-byte segment substitution**: for each table entry
///   `(from, to, cost)`, the slice ending at the current DP position
///   that matches `from` on one side and `to` on the other (or
///   vice-versa) lets the DP take a shortcut of `cost`.
///
/// Cell value at `(i, j)` is the minimum over: delete, insert, single
/// substitute, and *every* applicable multi-byte substitution. The
/// function operates on `&str` but treats the input as a byte sequence —
/// safe for ASCII pinyin / jyutping / Hepburn romaji; non-ASCII inputs
/// yield byte-edit-distance, not char-edit-distance.
///
/// Worst-case time `O(|a| · |b| · k)` where `k = table.pairs.len()`;
/// memory `O(|a| · |b|)`.
///
/// ```
/// use inputx_phonetic_edit::{edit_distance, MANDARIN_DEFAULT, EditCostTable};
///
/// // Identity is zero
/// assert_eq!(edit_distance("nihao", "nihao", &MANDARIN_DEFAULT), 0.0);
/// assert_eq!(edit_distance("", "", &EditCostTable::EMPTY), 0.0);
///
/// // Empty table → plain Levenshtein
/// assert_eq!(edit_distance("kitten", "sitten", &EditCostTable::EMPTY), 1.0);
/// assert_eq!(edit_distance("kitten", "sitting", &EditCostTable::EMPTY), 3.0);
/// ```
pub fn edit_distance(a: &str, b: &str, table: &EditCostTable) -> f64 {
    let a = a.as_bytes();
    let b = b.as_bytes();
    let n = a.len();
    let m = b.len();
    if n == 0 {
        return m as f64;
    }
    if m == 0 {
        return n as f64;
    }
    // dp[i][j] = best cost to transform a[..i] → b[..j].
    let mut dp = vec![vec![0.0f64; m + 1]; n + 1];
    for i in 0..=n {
        dp[i][0] = i as f64;
    }
    for j in 0..=m {
        dp[0][j] = j as f64;
    }
    for i in 1..=n {
        for j in 1..=m {
            let sub = dp[i - 1][j - 1] + if a[i - 1] == b[j - 1] { 0.0 } else { 1.0 };
            let del = dp[i - 1][j] + 1.0;
            let ins = dp[i][j - 1] + 1.0;
            let mut best = min3(sub, del, ins);

            // Multi-byte substitution lookup — for each table entry, both
            // directions: a's tail matches `from` and b's tail matches `to`,
            // or vice-versa. Both bytes of the tails must be inside the
            // already-computed DP region (i >= from.len, j >= to.len).
            for &(from, to, cost) in table.pairs {
                let fl = from.len();
                let tl = to.len();
                // Forward: a's tail = from, b's tail = to
                if i >= fl
                    && j >= tl
                    && a[i - fl..i] == *from.as_bytes()
                    && b[j - tl..j] == *to.as_bytes()
                {
                    let cand = dp[i - fl][j - tl] + cost;
                    if cand < best {
                        best = cand;
                    }
                }
                // Reverse: a's tail = to, b's tail = from
                if fl != tl
                    && i >= tl
                    && j >= fl
                    && a[i - tl..i] == *to.as_bytes()
                    && b[j - fl..j] == *from.as_bytes()
                {
                    let cand = dp[i - tl][j - fl] + cost;
                    if cand < best {
                        best = cand;
                    }
                } else if fl == tl
                    && i >= fl
                    && j >= tl
                    && a[i - fl..i] == *to.as_bytes()
                    && b[j - tl..j] == *from.as_bytes()
                {
                    // Same-length pairs: still check the swapped direction.
                    let cand = dp[i - fl][j - tl] + cost;
                    if cand < best {
                        best = cand;
                    }
                }
            }
            dp[i][j] = best;
        }
    }
    dp[n][m]
}

#[inline]
fn min3(a: f64, b: f64, c: f64) -> f64 {
    let ab = if a < b { a } else { b };
    if ab < c { ab } else { c }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    // ---- Standard Levenshtein invariants (empty table) ----

    #[test]
    fn identity_is_zero() {
        assert_eq!(edit_distance("", "", &EditCostTable::EMPTY), 0.0);
        assert_eq!(edit_distance("nihao", "nihao", &EditCostTable::EMPTY), 0.0);
    }

    #[test]
    fn empty_a_costs_len_b() {
        assert_eq!(edit_distance("", "abc", &EditCostTable::EMPTY), 3.0);
        assert_eq!(edit_distance("xyz", "", &EditCostTable::EMPTY), 3.0);
    }

    #[test]
    fn classical_kitten_sitting_is_three() {
        // Textbook Wagner-Fischer example.
        assert_eq!(
            edit_distance("kitten", "sitting", &EditCostTable::EMPTY),
            3.0
        );
    }

    #[test]
    fn classical_intention_execution_is_five() {
        assert_eq!(
            edit_distance("intention", "execution", &EditCostTable::EMPTY),
            5.0
        );
    }

    // ---- Mandarin table — each canonical pair, both directions ----

    #[test]
    fn mandarin_zh_z_swap() {
        assert!(approx(
            edit_distance("zhongguo", "zongguo", &MANDARIN_DEFAULT),
            0.3
        ));
        assert!(approx(
            edit_distance("zongguo", "zhongguo", &MANDARIN_DEFAULT),
            0.3
        ));
    }

    #[test]
    fn mandarin_sh_s_swap() {
        assert!(approx(edit_distance("shi", "si", &MANDARIN_DEFAULT), 0.3));
    }

    #[test]
    fn mandarin_ch_c_swap() {
        assert!(approx(edit_distance("chi", "ci", &MANDARIN_DEFAULT), 0.3));
    }

    #[test]
    fn mandarin_f_h_swap() {
        assert!(approx(edit_distance("fei", "hei", &MANDARIN_DEFAULT), 0.3));
        assert!(approx(edit_distance("hei", "fei", &MANDARIN_DEFAULT), 0.3));
    }

    #[test]
    fn mandarin_n_l_swap() {
        assert!(approx(edit_distance("ni", "li", &MANDARIN_DEFAULT), 0.2));
        assert!(approx(edit_distance("li", "ni", &MANDARIN_DEFAULT), 0.2));
    }

    #[test]
    fn mandarin_r_l_swap() {
        assert!(approx(edit_distance("ri", "li", &MANDARIN_DEFAULT), 0.3));
    }

    #[test]
    fn mandarin_in_ing_swap() {
        assert!(approx(edit_distance("xin", "xing", &MANDARIN_DEFAULT), 0.2));
        assert!(approx(edit_distance("xing", "xin", &MANDARIN_DEFAULT), 0.2));
    }

    #[test]
    fn mandarin_en_eng_swap() {
        assert!(approx(edit_distance("fen", "feng", &MANDARIN_DEFAULT), 0.2));
    }

    #[test]
    fn mandarin_an_ang_swap() {
        assert!(approx(edit_distance("fan", "fang", &MANDARIN_DEFAULT), 0.2));
    }

    // ---- Composite cases ----

    #[test]
    fn mandarin_two_swaps_in_one_syllable() {
        // zh↔z (0.3) AND in↔ing (0.2) — the DP must combine both as a
        // single 0.5 cost, never a 1.0+ "plain Levenshtein" fallback.
        assert!(approx(
            edit_distance("zhin", "zing", &MANDARIN_DEFAULT),
            0.5
        ));
    }

    #[test]
    fn mandarin_unrelated_typo_still_costs_one() {
        // No table entry for x↔p — full per-char insert cost.
        assert!(approx(edit_distance("xin", "pin", &MANDARIN_DEFAULT), 1.0));
    }

    #[test]
    fn mandarin_table_does_not_make_unrelated_words_cheap() {
        // Confidence guard: random near-pair without overlapping fuzzy
        // segments should not benefit from the table.
        let d = edit_distance("abc", "xyz", &MANDARIN_DEFAULT);
        assert!(approx(d, 3.0), "got {d}, expected 3.0");
    }

    #[test]
    fn empty_table_is_pure_levenshtein() {
        for (a, b, expected) in [
            ("", "", 0.0),
            ("a", "", 1.0),
            ("", "a", 1.0),
            ("ab", "abc", 1.0),
            ("kitten", "sitting", 3.0),
            ("flaw", "lawn", 2.0),
            ("gumbo", "gambol", 2.0),
        ] {
            assert!(
                approx(edit_distance(a, b, &EditCostTable::EMPTY), expected),
                "edit_distance({a:?}, {b:?}, EMPTY) != {expected}"
            );
        }
    }

    // ---- inputx_pinyin::fuzzy alignment ----
    //
    // The Mandarin default table is the same 9 fuzzy pairs that
    // inputx_pinyin::fuzzy::FuzzyConfig wires. Mismatched pair lists would
    // produce silently divergent ranking once the engine migrates onto
    // this stone (post-v1.4 polish). This test pins the count + pair set.

    #[test]
    fn mandarin_pair_count_matches_inputx_pinyin_fuzzy_config() {
        assert_eq!(MANDARIN_DEFAULT.pairs.len(), 9);
    }

    #[test]
    fn mandarin_pair_set_covers_all_nine_canonical_swaps() {
        // Each item asserts an entry exists (either direction).
        let needed: &[(&str, &str)] = &[
            ("zh", "z"),
            ("sh", "s"),
            ("ch", "c"),
            ("f", "h"),
            ("r", "l"),
            ("n", "l"),
            ("in", "ing"),
            ("en", "eng"),
            ("an", "ang"),
        ];
        for &(a, b) in needed {
            let found = MANDARIN_DEFAULT
                .pairs
                .iter()
                .any(|&(f, t, _)| (f == a && t == b) || (f == b && t == a));
            assert!(found, "MANDARIN_DEFAULT missing pair ({a:?}, {b:?})");
        }
    }
}
