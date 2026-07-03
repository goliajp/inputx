//! Phase-5 CP-5.3 step-1 — keyboard adjacency model for typo edges.
//!
//! This module ships the scaffolding the climb plan's CP-5.3 calls for:
//! a QWERTY-layout-aware distance metric between letter keys, plus a
//! helper that enumerates single-edit substitutions of a pinyin string
//! ranked by how plausible each substitution is given the layout.
//!
//! It is **scoring-only** — no lattice integration in step-1. The
//! production caller (CP-5.3 step-2) will plug
//! [`single_edit_neighbors`] into the typo-rescue path so the lattice
//! sees adjacency-weighted typo edges, but until that wiring lands the
//! engine's runtime behavior is unchanged.
//!
//! # Distance model
//!
//! QWERTY US layout, modelled with floating-point `(row, col)`
//! coordinates so the natural row-stagger is captured:
//!
//! ```text
//! row 0 (top)   q  w  e  r  t  y  u  i  o  p
//! row 1 (home)  a  s  d  f  g  h  j  k  l
//! row 2 (bot)   z  x  c  v  b  n  m
//! ```
//!
//! Column offsets follow the physical stagger: the home row sits
//! +0.5 to the right of the top row, the bottom row +1.0. So `q` and
//! `a` are not at the same column — they're at (0, 0) and (1, 0.5),
//! distance √(1 + 0.25) ≈ 1.118.
//!
//! [`key_distance`] returns Euclidean distance between two keys, or
//! [`f32::INFINITY`] when either input isn't a known letter (digits,
//! punctuation, non-ASCII).
//!
//! [`adjacency_log_prob`] converts a distance into a `log10 P(typed |
//! intended)` cost, monotone non-increasing in distance — adjacent keys
//! are common typos, far keys are rare. The table is hand-picked to
//! the same order of magnitude as the existing Path 1c uniform default
//! `log10(0.05) ≈ -1.301`, so swapping in adjacency-weighted scores
//! doesn't introduce a step change in overall channel cost.

/// QWERTY US row coordinate (0 = top, 1 = home, 2 = bottom).
type Row = u8;

/// QWERTY US column coordinate, scaled by 2 so we can store
/// row-stagger half-step offsets as integers (`col = 1` means
/// physical column 0.5). This keeps the lookup table a plain `&[u8]`
/// instead of a float table.
type Col = u8;

/// Lookup `(row, col_scaled_by_2)` for an ASCII lowercase letter, or
/// `None` for non-keyboard chars. Implemented as a `match` (rather
/// than an indexed table) so the compiler can constant-fold it into a
/// jump table — same speed, less initialization code.
fn key_coord(c: char) -> Option<(Row, Col)> {
    // col_scaled_by_2: top row starts at 0, home row +1 (= +0.5
    // physical), bottom row +2 (= +1.0 physical). Each successive
    // key on the same row adds 2 (= +1.0 physical).
    Some(match c {
        // Top row: q w e r t y u i o p
        'q' => (0, 0),
        'w' => (0, 2),
        'e' => (0, 4),
        'r' => (0, 6),
        't' => (0, 8),
        'y' => (0, 10),
        'u' => (0, 12),
        'i' => (0, 14),
        'o' => (0, 16),
        'p' => (0, 18),
        // Home row: a s d f g h j k l (stagger +0.5 = scaled +1)
        'a' => (1, 1),
        's' => (1, 3),
        'd' => (1, 5),
        'f' => (1, 7),
        'g' => (1, 9),
        'h' => (1, 11),
        'j' => (1, 13),
        'k' => (1, 15),
        'l' => (1, 17),
        // Bottom row: z x c v b n m (stagger +1.0 = scaled +2)
        'z' => (2, 2),
        'x' => (2, 4),
        'c' => (2, 6),
        'v' => (2, 8),
        'b' => (2, 10),
        'n' => (2, 12),
        'm' => (2, 14),
        _ => return None,
    })
}

/// Euclidean distance between two QWERTY keys, in physical-key units
/// (1.0 = one key over). Returns [`f32::INFINITY`] when either char
/// isn't on the modelled keyboard.
///
/// Examples (round to 2 decimals):
/// - `key_distance('q', 'q')` = 0.00 (same key)
/// - `key_distance('q', 'w')` = 1.00 (adjacent same row)
/// - `key_distance('q', 'a')` ≈ 1.12 (down + 0.5 stagger)
/// - `key_distance('a', 's')` = 1.00
/// - `key_distance('q', 'p')` = 9.00 (top-row opposite ends)
/// - `key_distance('q', '0')` = INFINITY (digit isn't modelled)
pub fn key_distance(a: char, b: char) -> f32 {
    let (Some((ra, ca)), Some((rb, cb))) = (key_coord(a), key_coord(b)) else {
        return f32::INFINITY;
    };
    let dr = ra as f32 - rb as f32;
    // col is scaled by 2 in the table; divide back to physical units.
    let dc = (ca as f32 - cb as f32) / 2.0;
    (dr * dr + dc * dc).sqrt()
}

/// Channel cost `log10 P(typed | intended)` derived from
/// [`key_distance`]. Monotone non-increasing in distance: adjacent
/// keys are common typos (least negative), far keys are rare (most
/// negative). Non-keyboard / identical chars return `f32::INFINITY`
/// (caller must handle — identical chars aren't a "typo" by
/// definition, and unknown chars shouldn't form typo edges at all).
///
/// The cost ladder is calibrated to the same order of magnitude as
/// the legacy Path 1c uniform `log10(0.05) ≈ -1.301` so dropping
/// adjacency weighting into the lattice doesn't shift the overall
/// channel-vs-LM balance.
///
/// | distance band | P  | log10 P    |
/// |---------------|----|-----------:|
/// | 0 ≤ d ≤ 1.05  | 5% | -1.301     |
/// | d ≤ 1.5       | 3% | -1.523     |
/// | d ≤ 2.5       | 2% | -1.699     |
/// | d ≤ 4         | 1% | -2.000     |
/// | d ≤ 6         | 0.3% | -2.523   |
/// | otherwise     | 0.1% | -3.000   |
pub fn adjacency_log_prob(typed: char, intended: char) -> f32 {
    let d = key_distance(typed, intended);
    if !d.is_finite() || typed == intended {
        return f32::INFINITY;
    }
    if d <= 1.05 {
        -1.301
    } else if d <= 1.5 {
        -1.523
    } else if d <= 2.5 {
        -1.699
    } else if d <= 4.0 {
        -2.000
    } else if d <= 6.0 {
        -2.523
    } else {
        -3.000
    }
}

/// Enumerate the **single-letter substitution** variants of `typed`
/// that are at most `max_distance` keys away from the original
/// letter. Each result is `(variant_string, log10_P(typed |
/// variant))`.
///
/// The intended use is: the user typed `typed` and meant something
/// nearby on the keyboard. For each position `i`, we consider
/// swapping `typed[i]` for every other letter on the modelled
/// keyboard whose [`key_distance`] is ≤ `max_distance`, and emit the
/// resulting variant with the corresponding [`adjacency_log_prob`].
/// Callers (CP-5.3 step-2 lattice wiring) look each variant up in the
/// dict; variants that aren't valid pinyin will simply yield no
/// candidates and contribute no edges.
///
/// Returns an empty `Vec` for empty input, for input containing any
/// non-keyboard char, or for `max_distance ≤ 0`.
///
/// Cost: O(n × 25) per call where n = `typed.len()`. With
/// `max_distance = 1.5` we typically emit 4-8 variants per position
/// (4 cardinal + diagonals filtered by distance), so for a typical
/// pinyin syllable of length 4 the result is ~16-32 variants.
pub fn single_edit_neighbors(typed: &str, max_distance: f32) -> Vec<(String, f32)> {
    if typed.is_empty() || max_distance <= 0.0 {
        return Vec::new();
    }
    // Ensure all input chars are on the modelled keyboard. If any
    // char isn't, return empty — partial neighbor sets would mislead
    // the lattice into thinking the chunk is partially recoverable.
    let chars: Vec<char> = typed.chars().collect();
    for c in &chars {
        if key_coord(*c).is_none() {
            return Vec::new();
        }
    }

    // The 26 ASCII lowercase letters that have a key_coord entry —
    // computed once and shared across positions.
    const KEYBOARD_LETTERS: &[char] = &[
        'q', 'w', 'e', 'r', 't', 'y', 'u', 'i', 'o', 'p', 'a', 's', 'd', 'f', 'g', 'h', 'j', 'k',
        'l', 'z', 'x', 'c', 'v', 'b', 'n', 'm',
    ];

    let mut out: Vec<(String, f32)> = Vec::with_capacity(chars.len() * 6);
    for (i, &orig) in chars.iter().enumerate() {
        for &cand in KEYBOARD_LETTERS {
            if cand == orig {
                continue;
            }
            let d = key_distance(orig, cand);
            if !d.is_finite() || d > max_distance {
                continue;
            }
            let lp = adjacency_log_prob(orig, cand);
            if !lp.is_finite() {
                continue;
            }
            let mut variant: String = String::with_capacity(typed.len());
            for (k, c) in chars.iter().enumerate() {
                if k == i {
                    variant.push(cand);
                } else {
                    variant.push(*c);
                }
            }
            out.push((variant, lp));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coords_cover_all_26_letters() {
        for c in 'a'..='z' {
            assert!(key_coord(c).is_some(), "letter {c} should have a coord");
        }
        assert!(key_coord('0').is_none());
        assert!(key_coord('-').is_none());
        assert!(key_coord(' ').is_none());
    }

    #[test]
    fn distance_zero_for_same_key() {
        assert_eq!(key_distance('q', 'q'), 0.0);
        assert_eq!(key_distance('m', 'm'), 0.0);
    }

    #[test]
    fn distance_one_for_horizontal_adjacency() {
        assert!((key_distance('q', 'w') - 1.0).abs() < 1e-6);
        assert!((key_distance('a', 's') - 1.0).abs() < 1e-6);
        assert!((key_distance('z', 'x') - 1.0).abs() < 1e-6);
        // Reverse direction is symmetric.
        assert!((key_distance('w', 'q') - 1.0).abs() < 1e-6);
    }

    #[test]
    fn distance_close_to_one_for_vertical_with_stagger() {
        // q is at (0, 0), a is at (1, 0.5). Distance = √(1 + 0.25).
        let d = key_distance('q', 'a');
        assert!(
            (d - 1.118).abs() < 0.01,
            "q→a expected ≈ 1.118 (vertical + stagger), got {d}"
        );
    }

    #[test]
    fn distance_far_for_opposite_ends() {
        // Top row q and p are 9 keys apart.
        let d = key_distance('q', 'p');
        assert!((d - 9.0).abs() < 1e-4, "q→p expected 9.0, got {d}");
        // Diagonal opposite (top-left q to bottom-right m).
        let d2 = key_distance('q', 'm');
        assert!(d2 > 6.0, "q→m expected > 6.0, got {d2}");
    }

    #[test]
    fn distance_infinite_for_non_keyboard_chars() {
        assert!(key_distance('q', '0').is_infinite());
        assert!(key_distance('-', 'a').is_infinite());
        assert!(key_distance('我', 'q').is_infinite());
    }

    #[test]
    fn adjacency_log_prob_is_monotone_decreasing() {
        // q→w (adjacent, d=1) > q→e (d=2) > q→r (d=3) > q→p (d=9)
        let qw = adjacency_log_prob('q', 'w');
        let qe = adjacency_log_prob('q', 'e');
        let qr = adjacency_log_prob('q', 'r');
        let qp = adjacency_log_prob('q', 'p');
        assert!(
            qw > qe && qe >= qr && qr > qp,
            "expected monotone qw {qw} > qe {qe} >= qr {qr} > qp {qp}"
        );
        // Adjacency floor matches the legacy uniform default
        // (log10(0.05) ≈ -1.301) so adjacency-weighting doesn't shift
        // the overall channel-vs-LM scale.
        assert!(
            (qw - (-1.301)).abs() < 1e-6,
            "q→w log_prob should match legacy uniform default, got {qw}"
        );
    }

    #[test]
    fn adjacency_log_prob_infinite_for_same_or_unknown() {
        assert!(adjacency_log_prob('q', 'q').is_infinite());
        assert!(adjacency_log_prob('q', '0').is_infinite());
    }

    #[test]
    fn single_edit_neighbors_empty_inputs() {
        assert!(single_edit_neighbors("", 1.5).is_empty());
        assert!(single_edit_neighbors("ni", 0.0).is_empty());
        assert!(single_edit_neighbors("ni", -1.0).is_empty());
        // Non-keyboard char in input → empty (we don't speculate).
        assert!(single_edit_neighbors("我", 1.5).is_empty());
        assert!(single_edit_neighbors("ni1", 1.5).is_empty());
    }

    #[test]
    fn single_edit_neighbors_surface_expected_swaps_for_ni() {
        // max_distance=1.5 should cover horizontal-adjacent (d=1) and
        // vertical-with-stagger (d≈1.118).
        let variants = single_edit_neighbors("ni", 1.5);
        let words: std::collections::HashSet<String> =
            variants.iter().map(|(w, _)| w.clone()).collect();
        // Position 0: n's adjacent keys at d ≤ 1.5
        //   horizontal: 'b' (d=1), 'm' (d=1) → "bi", "mi"
        //   vertical/diagonal: 'h' (d≈1.118), 'j' (d≈1.118)
        //     → "hi", "ji"
        assert!(
            words.contains("bi"),
            "expected 'bi' among ni-variants, got {variants:?}"
        );
        assert!(words.contains("mi"), "expected 'mi' among ni-variants");
        // Position 1: i's adjacent keys
        //   horizontal: 'u', 'o' (d=1) → "nu", "no"
        //   vertical/stagger: 'j', 'k' (d≈1.118) → "nj", "nk"
        assert!(words.contains("nu"), "expected 'nu' among ni-variants");
        assert!(words.contains("no"), "expected 'no' among ni-variants");
        // Original syllable must NOT be in the variant set.
        assert!(!words.contains("ni"));
        // All log_probs are finite, negative, and at least as
        // negative as the d≤1.05 ceiling (-1.301).
        for (w, lp) in &variants {
            assert!(
                lp.is_finite() && *lp < 0.0,
                "variant {w} has invalid log_prob {lp}"
            );
            assert!(
                *lp <= -1.301,
                "variant {w} log_prob {lp} should be ≤ adjacency ceiling -1.301"
            );
        }
    }

    #[test]
    fn single_edit_neighbors_far_distance_returns_more() {
        // At max_distance=1.0 (strict horizontal only), only ~2-4
        // variants per position; at max_distance=3.0 we should see
        // many more.
        let strict = single_edit_neighbors("a", 1.0);
        let loose = single_edit_neighbors("a", 3.0);
        assert!(
            loose.len() > strict.len(),
            "loose ({}) should yield more than strict ({})",
            loose.len(),
            strict.len()
        );
    }
}
