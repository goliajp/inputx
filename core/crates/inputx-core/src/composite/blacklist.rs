//! Pollution blacklist — candidate strings that must NEVER surface, no matter
//! what the engines score them.
//!
//! Why a list separate from scoring: the per-engine quality heuristics (the
//! Viterbi-composition per-char gate, JP pure-kana demote, etc.) catch most
//! junk, but they are heuristics — a future dict rebuild or a scoring retune
//! could resurrect a specific known-bad string. Listing it here guarantees it
//! stays gone regardless of the numbers (user 2026-05-26: "以后更新又有可能污染").
//!
//! User-driven: when 实测 turns up a candidate that is pure pollution and
//! should never appear, add the exact string here. Keep entries sorted.

use std::collections::HashSet;
use std::sync::OnceLock;

/// Exact candidate strings that are pure pollution — never show them.
const BLACKLIST: &[&str] = &[
    "是嗯据库", // forced Viterbi junk for the Japanese romaji `shinjuku`
    "片你",     // pianni Path-5 K-best #1 (freq 片>骗, (骗,你) bigram = 0
                // so K-best can't rerank); not a real phrase. User
                // 2026-05-26 policy: no ad-hoc dict patches for OOV
                // collocations — blacklist the wrong reading, let
                // K-best surface 便你/骗你/偏你/篇你.
];

/// `true` if `word` must be dropped from the merged candidate list.
pub fn is_blacklisted(word: &str) -> bool {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| BLACKLIST.iter().copied().collect())
        .contains(word)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_pollution_is_blacklisted() {
        assert!(is_blacklisted("是嗯据库"));
        assert!(is_blacklisted("片你"));
        // real words must NOT be blacklisted
        assert!(!is_blacklisted("继续"));
        assert!(!is_blacklisted("新宿"));
        assert!(!is_blacklisted("你好吗我叫"));
        // adjacent compositions that ARE legitimate must NOT be caught
        assert!(!is_blacklisted("骗你"));
        assert!(!is_blacklisted("便你"));
        assert!(!is_blacklisted("偏你"));
        assert!(!is_blacklisted("篇你"));
    }
}
