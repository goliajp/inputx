//! `RepeatedLetterExpansion` — v3.0.2a first migrated CandidateRule.
//!
//! User typing 3+ copies of the same ASCII letter wants the
//! corresponding Chinese interjection repeated:
//!   - `hhh` / `hhhhh` → `哈哈哈` / `哈哈哈哈哈`
//!   - `aaa` → `啊啊啊`
//!   - `ooo` → `哦哦哦`
//!   - `eee` → `诶诶诶`
//!   - `mmm` / `nnn` → `嗯嗯嗯`
//!   - `www` → `呜呜呜`
//!
//! This rule's job is to emit ONE high-score candidate so cross-engine
//! merge surfaces it at position 0. The score floor matches the
//! existing inline implementation's `COMPOSED_SCORE` = 500,000 — high
//! enough to lead any normal pinyin candidate at the same buffer.
//!
//! v3.0.2a status (2026-05-24): rule struct + tests. NOT wired into
//! production refresh_candidates yet — v3.0.2b will do that. The
//! original inline `try_repeated_letter_expansion` in
//! `composite/pinyin_adapter.rs` is still authoritative until then.
//! This file is the migration test pattern; subsequent rules follow
//! the same template.

use super::super::candidate::{CandidateRule, RuleCandidate};
use super::super::{Context, Rule, RuleEffect};

/// Score given to a repeated-letter expansion. Matches the inline
/// `COMPOSED_SCORE` constant in pinyin_adapter.rs so candidate
/// ordering is identical to pre-migration behavior.
pub const REPEATED_LETTER_SCORE: f64 = 500_000.0;

pub struct RepeatedLetterExpansion;

impl Rule for RepeatedLetterExpansion {
    fn name(&self) -> &'static str {
        "RepeatedLetterExpansion"
    }
    fn priority(&self) -> i32 {
        // Tier 100: generation rules (lowest priority = runs first).
        // Sits BEFORE Viterbi composition (which would otherwise burn
        // DP cycles trying to segment hhh/aaa/etc) and before all
        // pinyin lookup paths.
        100
    }
}

impl CandidateRule for RepeatedLetterExpansion {
    fn applies(&self, ctx: &Context) -> bool {
        let b = ctx.buffer.as_bytes();
        if b.len() < 3 {
            return false;
        }
        // All bytes equal AND the byte is an ASCII letter (sub-script
        // codepoints could in theory all-match, but the buffer is
        // ASCII-only by construction).
        b.iter().all(|&x| x == b[0]) && b[0].is_ascii_alphabetic()
    }
    fn apply(&self, ctx: &Context, cands: &mut Vec<RuleCandidate>) -> RuleEffect {
        // applies() guaranteed buffer is ≥3 same-letter ASCII bytes.
        let first = ctx.buffer.as_bytes()[0];
        let ch = match first {
            b'h' => '哈',
            b'a' => '啊',
            b'o' => '哦',
            b'e' => '诶',
            b'm' | b'n' => '嗯',
            b'w' => '呜',
            // Most consonants have no standalone interjection; rule is
            // applicable but yields no expansion. NoOp keeps trace clean.
            _ => return RuleEffect::NoOp,
        };
        let word: String = std::iter::repeat_n(ch, ctx.buffer.len()).collect();
        cands.insert(
            0,
            RuleCandidate {
                word: word.clone(),
                score: REPEATED_LETTER_SCORE,
                source: "repeated-letter",
            },
        );
        RuleEffect::Added(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composite::Mode;
    use crate::rules::ContextFlags;

    fn ctx(buf: &str) -> Context {
        Context {
            mode: Mode::Mixed,
            buffer: buf.to_string(),
            prev_committed: None,
            second_prev_committed: None,
            flags: ContextFlags {
                buffer_len: buf.len(),
                ..ContextFlags::default()
            },
        }
    }

    #[test]
    fn applies_only_to_3plus_same_ascii_letter() {
        let r = RepeatedLetterExpansion;
        assert!(!r.applies(&ctx("")), "empty buffer");
        assert!(!r.applies(&ctx("h")), "single letter too short");
        assert!(!r.applies(&ctx("hh")), "two letters still too short");
        assert!(r.applies(&ctx("hhh")), "three letters fires");
        assert!(r.applies(&ctx("hhhhhh")), "six fires");
        assert!(!r.applies(&ctx("hha")), "mixed letters skip");
        assert!(!r.applies(&ctx("nihao")), "regular pinyin skip");
        // Buffer that happens to all-match a non-letter byte (couldn't
        // actually happen in practice since the engine only accepts
        // ASCII letters into the buffer, but be defensive).
        assert!(!r.applies(&ctx("111")), "digits don't fire");
    }

    #[test]
    fn hhh_yields_哈哈哈() {
        let r = RepeatedLetterExpansion;
        let mut cands = Vec::new();
        let effect = r.apply(&ctx("hhh"), &mut cands);
        assert!(matches!(effect, RuleEffect::Added(1)));
        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].word, "哈哈哈");
        assert_eq!(cands[0].score, REPEATED_LETTER_SCORE);
        assert_eq!(cands[0].source, "repeated-letter");
    }

    #[test]
    fn length_matches_buffer() {
        let r = RepeatedLetterExpansion;
        for n in 3..=12 {
            let buf: String = std::iter::repeat_n('h', n).collect();
            let mut cands = Vec::new();
            let _ = r.apply(&ctx(&buf), &mut cands);
            assert_eq!(
                cands[0].word.chars().count(),
                n,
                "n={n} buffer should yield n CJK chars"
            );
            assert!(cands[0].word.chars().all(|c| c == '哈'));
        }
    }

    #[test]
    fn all_supported_letters() {
        let cases = [
            (b'h', '哈'),
            (b'a', '啊'),
            (b'o', '哦'),
            (b'e', '诶'),
            (b'm', '嗯'),
            (b'n', '嗯'),
            (b'w', '呜'),
        ];
        let r = RepeatedLetterExpansion;
        for (letter, expected) in cases {
            let buf = String::from_utf8(vec![letter; 3]).unwrap();
            let mut cands = Vec::new();
            let _ = r.apply(&ctx(&buf), &mut cands);
            assert_eq!(
                cands[0].word.chars().next(),
                Some(expected),
                "{} → {}",
                letter as char,
                expected
            );
        }
    }

    #[test]
    fn unsupported_letter_is_noop() {
        // Consonants without interjection mapping (b/c/d/f/...) hit the
        // applies() gate but apply() returns NoOp without adding cands.
        let r = RepeatedLetterExpansion;
        let mut cands = Vec::new();
        let effect = r.apply(&ctx("ttt"), &mut cands);
        assert!(matches!(effect, RuleEffect::NoOp));
        assert!(cands.is_empty());
    }

    #[test]
    fn insertion_at_front_preserves_existing_cands() {
        // Cross-rule contract: this rule inserts at position 0, so any
        // later rule sees the expansion first. Pre-existing cands shift.
        let r = RepeatedLetterExpansion;
        let mut cands = vec![RuleCandidate {
            word: "preexisting".into(),
            score: 100.0,
            source: "test",
        }];
        let _ = r.apply(&ctx("hhh"), &mut cands);
        assert_eq!(cands.len(), 2);
        assert_eq!(cands[0].word, "哈哈哈");
        assert_eq!(cands[1].word, "preexisting");
    }

    #[test]
    fn end_to_end_via_rule_engine() {
        use crate::rules::candidate::CandidateRuleEngine;
        use std::sync::Arc;
        let engine = CandidateRuleEngine::new(vec![Arc::new(RepeatedLetterExpansion)]);
        let mut cands = Vec::new();
        let trace = engine.run(&ctx("hhhhh"), &mut cands);
        assert!(trace.rule_fired("RepeatedLetterExpansion"));
        assert_eq!(cands[0].word, "哈哈哈哈哈");
    }
}
