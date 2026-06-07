//! v3.0 rule engine — scaffolding (v3.0.1 checkpoint, 2026-05-24).
//!
//! This module is the foundation of the rule-engine refactor. See
//! `.claude/PLAN-rule-engine.md` for the full design doc; this file is
//! the scaffolding-only first slice.
//!
//! # Why a rule engine?
//!
//! The composite engine currently expresses candidate-generation,
//! ranking, prediction, and commit logic as inline `if` / `match`
//! statements scattered across `dispatch.rs`, `pinyin_adapter.rs`,
//! `composite/engine.rs`, and `wubi/engine.rs`. That worked for the
//! first ~20 rules, but as the project adds more (fuzzy pinyin / typo
//! fallback / repeated-letter expansion / Viterbi composition /
//! per-layer demote / pinyin_intent gate / prediction conservative-
//! mode / trigram context / ...), several debt symptoms appear:
//!
//! - **Hidden coupling**: the `xlab → 向量` bug (user-reported, fixed
//!   2026-05-24) was a case where pinyin Path 1c's flag set in
//!   `pinyin_adapter.rs` silently triggered wubi layer demote in
//!   `dispatch.rs`. Diagnosing required reading 3 files to construct
//!   the causal chain.
//! - **No "why" trace**: when a candidate appears at #1 vs #3, there's
//!   no way to ask "which rules fired and in what order, what did each
//!   change?" Users (and Claude) reverse-engineer from diff inspection.
//! - **Hard to A/B**: turning a single rule off requires editing the
//!   if-condition in source and rebuilding. No feature-flag granularity.
//! - **Hard to test in isolation**: tests are buffer-driven integration
//!   tests; a "did rule X fire under condition Y" assertion is verbose.
//!
//! # Goals
//!
//! - **Trace**: every rule firing produces a `TraceEntry` so
//!   `inputx-trace 'xlab'` can dump the full causal chain.
//! - **Test**: rules implement traits with `applies()` and `apply()`,
//!   testable in fixture form.
//! - **Toggle**: runtime feature flags (env var or settings) can disable
//!   any rule without recompile.
//! - **Order**: explicit priority (lower = earlier) replaces "place in
//!   source file" as the ordering mechanism.
//!
//! # Non-goals (v3.0.1)
//!
//! - Migrating existing rules — that's v3.0.2 / v3.0.3 / v3.0.4.
//! - DAG-based dependency declaration — priority is enough for now.
//! - Removing inline rules — they coexist with the trait machinery
//!   during migration; this file is additive.
//!
//! # Module layout
//!
//! - `mod.rs` (this file): `Context`, `RuleEffect`, `TraceEntry`,
//!   `ExecutionTrace`, `Rule` base trait.
//! - `candidate.rs`: `CandidateRule` + `CandidateRuleEngine`.
//! - `prediction.rs`: `PredictionRule` + `PredictionRuleEngine`.
//! - `commit.rs`: `CommitRule` + `CommitRuleEngine`.

use std::time::Duration;

use crate::composite::Mode;

pub mod builtin;
pub mod candidate;
pub mod commit;
pub mod prediction;

/// Snapshot of the IME state a rule sees when deciding whether/how to
/// fire. Read-only — rules don't mutate this; effect routes via the
/// payload parameter (Vec<Candidate> for candidate rules, etc.).
///
/// Built once per rule-engine invocation by the composite engine. All
/// fields are owned `String`/copies so the Context is cheap to pass and
/// can outlive the source state if needed (e.g. during trace dump).
#[derive(Clone, Debug)]
pub struct Context {
    /// Current engine mode (WubiOnly / PinyinOnly / Mixed / JapaneseOnly).
    pub mode: Mode,
    /// Active input buffer (the un-committed code, e.g. "xlab", "nihao",
    /// "tjvs"). Empty in prediction-mode (post-commit) Contexts.
    pub buffer: String,
    /// Most recently committed CJK word, if any. Used for bigram
    /// rescore + first-step prediction context.
    pub prev_committed: Option<String>,
    /// Word committed before `prev_committed`, if any. Used for trigram
    /// prediction context (and conservative-mode chain gating — chained
    /// predictions require trigram context, no bigram fallback).
    pub second_prev_committed: Option<String>,
    /// Derived flags (computed once at Context creation; rules read).
    pub flags: ContextFlags,
}

/// Boolean flags derived from `buffer` + sub-engine state. Computed
/// once at Context creation so rules don't recompute. Adding a new
/// flag is fine; renaming an existing one breaks rule code, so they're
/// stable.
#[derive(Clone, Copy, Debug, Default)]
pub struct ContextFlags {
    /// True iff `buffer` contains at least one a/e/i/o/u/v.
    pub has_vowel: bool,
    /// True iff the pinyin engine has at least one non-speculative
    /// match for `buffer` (exact lookup or initials index — NOT fuzzy
    /// substitution or typo-initials fallback, which are speculative).
    pub has_non_speculative_pinyin: bool,
    /// Derived: short buffer (≤4) + has_vowel + has_non_speculative.
    /// When true, "user is typing pinyin not wubi" — wubi Auto/Phrase
    /// layers should demote to make room for pinyin top candidates.
    pub pinyin_intent: bool,
    /// True iff buffer's first char is 'z' (wubi 'z' is rare so we
    /// suppress wubi candidates in Mixed mode for 'z*' inputs).
    pub starts_with_z: bool,
    /// Length of `buffer` in bytes (== chars since ASCII).
    pub buffer_len: usize,
}

/// What a rule did when it fired. Recorded in TraceEntry for debug
/// inspection.
#[derive(Clone, Debug)]
pub enum RuleEffect {
    /// Rule was applicable, ran, but didn't change anything (e.g.
    /// looked for fuzzy matches, found none).
    NoOp,
    /// Rule added N candidates (or predictions).
    Added(usize),
    /// Rule removed N candidates.
    Removed(usize),
    /// Rule kept K of K+D candidates, dropped D.
    Filtered { kept: usize, dropped: usize },
    /// Rule re-scored or re-ordered the candidate list.
    Reranked,
    /// Rule promoted `word` to position 0 (e.g. L0 pin).
    PromotedToFront(String),
    /// Commit rule triggered auto-commit of `word` with stated reason.
    AutoCommitted { word: String, reason: &'static str },
    /// Rule ran but failed (panic'd, returned an error). The rule's
    /// effect is NoOp and execution proceeds — failures don't block
    /// later rules. Reason is human-readable.
    Failed(String),
}

/// One row in the execution trace — what one rule did, when, how long.
#[derive(Clone, Debug)]
pub struct TraceEntry {
    pub rule_name: &'static str,
    pub priority: i32,
    pub effect: RuleEffect,
    pub elapsed: Duration,
}

/// Full trace of a single rule-engine run. The composite engine
/// optionally surfaces this via `last_trace()` for debug commands.
///
/// `fired` and `skipped` are disjoint — every rule the engine
/// considered shows up exactly once. `fired` means `applies(ctx)`
/// returned true and `apply()` was called; `skipped` means `applies()`
/// returned false (predicate failed, e.g. wrong mode for this rule).
#[derive(Clone, Debug, Default)]
pub struct ExecutionTrace {
    pub fired: Vec<TraceEntry>,
    pub skipped: Vec<TraceEntry>,
    pub total_elapsed: Duration,
}

impl ExecutionTrace {
    pub fn new() -> Self {
        Self::default()
    }

    /// True iff a rule with this name fired (not skipped, not absent).
    pub fn rule_fired(&self, name: &str) -> bool {
        self.fired.iter().any(|e| e.rule_name == name)
    }

    /// True iff a rule with this name was considered but skipped
    /// (predicate returned false).
    pub fn rule_skipped(&self, name: &str) -> bool {
        self.skipped.iter().any(|e| e.rule_name == name)
    }
}

/// Common base for all rule traits. Rules are static — name and
/// priority don't change at runtime.
///
/// Implementations are `Send + Sync` so a single `RuleEngine` can be
/// shared across threads (the IME today is single-threaded but the
/// trait stays open for future wasm/web/server use).
pub trait Rule: Send + Sync {
    /// Stable string id used in trace + feature-flag lookup. Must be
    /// unique within its rule kind (candidate / prediction / commit).
    fn name(&self) -> &'static str;
    /// Lower priority = runs earlier. Convention: 100s for generation
    /// rules, 500s for rerank / demote, 900s for final merge.
    fn priority(&self) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_default_is_empty() {
        let t = ExecutionTrace::default();
        assert!(t.fired.is_empty());
        assert!(t.skipped.is_empty());
        assert_eq!(t.total_elapsed, Duration::ZERO);
        assert!(!t.rule_fired("any"));
        assert!(!t.rule_skipped("any"));
    }

    #[test]
    fn rule_effect_variants_compile() {
        // Smoke: the variants we plan to use are constructible.
        let _ = RuleEffect::NoOp;
        let _ = RuleEffect::Added(3);
        let _ = RuleEffect::Removed(1);
        let _ = RuleEffect::Filtered {
            kept: 5,
            dropped: 2,
        };
        let _ = RuleEffect::Reranked;
        let _ = RuleEffect::PromotedToFront("理想".to_string());
        let _ = RuleEffect::AutoCommitted {
            word: "复杂".to_string(),
            reason: "OnFourCodesIfUnique",
        };
        let _ = RuleEffect::Failed("bigram FST stream error".to_string());
    }

    #[test]
    fn context_flags_default_all_false() {
        let f = ContextFlags::default();
        assert!(!f.has_vowel);
        assert!(!f.has_non_speculative_pinyin);
        assert!(!f.pinyin_intent);
        assert!(!f.starts_with_z);
        assert_eq!(f.buffer_len, 0);
    }
}
