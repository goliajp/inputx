//! Pinyin syllable segmentation via dynamic programming.
//!
//! Splits a continuous pinyin string like `zhonghuarenmin` into
//! `[zhong, hua, ren, min]`. Ambiguous inputs (e.g., `xian` could be
//! `[xi, an]` or `[xian]`) get all valid segmentations enumerated; the
//! caller picks one based on dict hits.
//!
//! Algorithm: standard DP. `dp[i]` = list of (prev_index, syllable_str)
//! reachable from position `i`. The final segmentations are reconstructed by
//! backtracking from `dp[len]`.
//!
//! Performance: the input length is bounded by user input (pinyin
//! sub-string typed before commit, typically < 20 chars). DP is O(n × max_syl)
//! where max_syl is the longest valid syllable byte-length (6: `zhuang`).

use crate::syllable;

/// One valid segmentation of an input string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segmentation {
    /// Syllables in left-to-right order. Each is a substring of the input.
    pub syllables: Vec<String>,
}

impl Segmentation {
    /// Number of syllables in this segmentation.
    pub fn len(&self) -> usize {
        self.syllables.len()
    }

    /// `true` iff there are no syllables.
    pub fn is_empty(&self) -> bool {
        self.syllables.is_empty()
    }
}

/// Longest valid syllable byte-length. Computed at construction; cap for the
/// DP inner loop.
const MAX_SYL_LEN: usize = 6; // "zhuang", "shuang", "chuang"

/// Returns all valid segmentations of `input`. Empty input yields a single
/// empty segmentation. Inputs that don't fully split return an empty `Vec`.
///
/// Results are ordered by syllable count ascending (fewer syllables first =
/// longer-match preference, the default IME convention).
pub fn segment(input: &str) -> Vec<Segmentation> {
    let s = input.to_ascii_lowercase();
    if s.is_empty() {
        return vec![Segmentation {
            syllables: Vec::new(),
        }];
    }

    let bytes = s.as_bytes();
    let n = bytes.len();

    // dp[i] = list of (prev_pos, syllable_starting_at_prev) that reach
    // position i. dp[0] is implicitly the start.
    let mut dp: Vec<Vec<(usize, &str)>> = vec![Vec::new(); n + 1];
    let mut reachable = vec![false; n + 1];
    reachable[0] = true;

    for i in 0..n {
        if !reachable[i] {
            continue;
        }
        let max_end = (i + MAX_SYL_LEN).min(n);
        for j in (i + 1)..=max_end {
            // SAFETY: we built `s` from a valid str; ASCII slicing is safe.
            let candidate = &s[i..j];
            if syllable::is_valid(candidate) {
                dp[j].push((i, candidate));
                reachable[j] = true;
            }
        }
    }

    if !reachable[n] {
        return Vec::new();
    }

    // Backtrack from n. Collect all paths.
    let mut results: Vec<Segmentation> = Vec::new();
    let mut path: Vec<&str> = Vec::new();
    backtrack(&dp, n, &mut path, &mut results);

    results.sort_by_key(|seg| seg.syllables.len());
    results
}

fn backtrack<'a>(
    dp: &[Vec<(usize, &'a str)>],
    pos: usize,
    path: &mut Vec<&'a str>,
    out: &mut Vec<Segmentation>,
) {
    if pos == 0 {
        // path is built end→start as we recurse from pos=n down to pos=0;
        // reverse to get start→end order.
        let mut syllables: Vec<String> = path.iter().map(|s| s.to_string()).collect();
        syllables.reverse();
        out.push(Segmentation { syllables });
        return;
    }
    for (prev, syl) in &dp[pos] {
        path.push(*syl);
        backtrack(dp, *prev, path, out);
        path.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_yields_empty_segmentation() {
        let out = segment("");
        assert_eq!(out.len(), 1);
        assert!(out[0].is_empty());
    }

    #[test]
    fn single_syllable() {
        let out = segment("zhong");
        assert!(!out.is_empty());
        assert_eq!(out[0].syllables, vec!["zhong"]);
    }

    #[test]
    fn two_syllable_unambiguous() {
        let out = segment("zhongguo");
        // Should at minimum contain [zhong, guo].
        assert!(
            out.iter().any(|s| s.syllables == vec!["zhong", "guo"]),
            "expected [zhong, guo] in {out:?}"
        );
    }

    #[test]
    fn long_unambiguous() {
        let out = segment("zhonghuarenmin");
        assert!(
            out.iter()
                .any(|s| s.syllables == vec!["zhong", "hua", "ren", "min"]),
            "expected [zhong, hua, ren, min] in {out:?}"
        );
    }

    #[test]
    fn ambiguous_xian_returns_multiple() {
        // `xian` itself is one syllable; `xi` + `an` is also valid.
        let out = segment("xian");
        let has_xian = out.iter().any(|s| s.syllables == vec!["xian"]);
        let has_xi_an = out.iter().any(|s| s.syllables == vec!["xi", "an"]);
        assert!(has_xian, "missing [xian] in {out:?}");
        assert!(has_xi_an, "missing [xi, an] in {out:?}");
    }

    #[test]
    fn fewer_syllables_first() {
        let out = segment("xian");
        // Sort order: [xian] (1) before [xi, an] (2).
        assert_eq!(out[0].len(), 1);
    }

    #[test]
    fn invalid_input_returns_empty() {
        assert!(segment("xxqz").is_empty());
        assert!(segment("zhongq").is_empty()); // trailing partial syllable
    }

    #[test]
    fn case_insensitive() {
        let out = segment("ZhongGuo");
        assert!(out.iter().any(|s| s.syllables == vec!["zhong", "guo"]));
    }
}
