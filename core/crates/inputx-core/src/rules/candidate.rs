//! Candidate-rule trait + engine (v3.0.1 scaffolding).
//!
//! A `CandidateRule` is anything that, given the current `Context` and
//! the in-progress candidate list, may add / remove / re-rank
//! candidates. Examples (to be migrated in v3.0.2):
//!   - `ExactPinyinLookup` (adds candidates from the pinyin FST)
//!   - `FuzzyPinyinSubstitution` (adds fuzzy alternatives)
//!   - `WubiLayerDemoteByPinyinIntent` (reranks by demoting Auto/Phrase)
//!   - `L0PinPromote` (moves pinned candidate to position 0)
//!
//! Rules run in priority order (ascending). Each rule sees the
//! candidate list AS LEFT BY PREVIOUS RULES — this is the simplest
//! useful model (and matches how the existing inline rules work).
//!
//! Failure semantics: if a rule's `apply()` panics, the engine catches
//! it (via `std::panic::catch_unwind`) and records `RuleEffect::Failed`.
//! Subsequent rules still run. The IME never crashes on a rule bug.

use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Instant;

use super::{Context, ExecutionTrace, Rule, RuleEffect, TraceEntry};

/// A candidate is the minimal common shape of an IME suggestion.
/// Avoids depending on `composite::Candidate` so the rules module
/// can stay self-contained during the migration; an adapter in
/// v3.0.2 will convert between the two.
#[derive(Clone, Debug)]
pub struct RuleCandidate {
    pub word: String,
    pub score: f64,
    pub source: &'static str,
}

pub trait CandidateRule: Rule {
    /// Decides whether the rule wants to run for this context. Default
    /// = always. Override to skip cheaply when not applicable (saves
    /// `apply()` from doing its own predicate check).
    fn applies(&self, _ctx: &Context) -> bool {
        true
    }
    /// Mutate the candidate list. Return what changed (for trace).
    /// Must be deterministic given the same Context + input list.
    fn apply(&self, ctx: &Context, cands: &mut Vec<RuleCandidate>) -> RuleEffect;
}

/// Container for a set of CandidateRules pre-sorted by priority.
pub struct CandidateRuleEngine {
    rules: Vec<Arc<dyn CandidateRule>>,
}

impl CandidateRuleEngine {
    pub fn new(mut rules: Vec<Arc<dyn CandidateRule>>) -> Self {
        rules.sort_by_key(|r| r.priority());
        Self { rules }
    }

    /// Run every rule in priority order; return the trace. The
    /// candidate list reflects all applied rules on return.
    pub fn run(&self, ctx: &Context, cands: &mut Vec<RuleCandidate>) -> ExecutionTrace {
        let mut trace = ExecutionTrace::default();
        let total_start = Instant::now();
        for rule in &self.rules {
            let entry_start = Instant::now();
            if !rule.applies(ctx) {
                trace.skipped.push(TraceEntry {
                    rule_name: rule.name(),
                    priority: rule.priority(),
                    effect: RuleEffect::NoOp,
                    elapsed: entry_start.elapsed(),
                });
                continue;
            }
            // Catch panic so one bad rule doesn't crash the IME. The
            // AssertUnwindSafe assert is conservative: rules shouldn't
            // hold non-poisoned state across panic boundaries; if a
            // rule does need that, it should use its own RwLock and
            // poison-guard internally.
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| rule.apply(ctx, cands)));
            let effect = match result {
                Ok(e) => e,
                Err(_) => RuleEffect::Failed(format!("{} panicked", rule.name())),
            };
            trace.fired.push(TraceEntry {
                rule_name: rule.name(),
                priority: rule.priority(),
                effect,
                elapsed: entry_start.elapsed(),
            });
        }
        trace.total_elapsed = total_start.elapsed();
        trace
    }

    /// Number of registered rules (after dedupe wouldn't make sense —
    /// duplicate-name rules are allowed and run twice, but that's
    /// a configuration bug we'd surface in the registry, not here).
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composite::Mode;

    struct NoOpRule {
        n: &'static str,
        p: i32,
    }
    impl Rule for NoOpRule {
        fn name(&self) -> &'static str {
            self.n
        }
        fn priority(&self) -> i32 {
            self.p
        }
    }
    impl CandidateRule for NoOpRule {
        fn apply(&self, _ctx: &Context, _cands: &mut Vec<RuleCandidate>) -> RuleEffect {
            RuleEffect::NoOp
        }
    }

    struct AddRule {
        n: &'static str,
        p: i32,
        word: &'static str,
    }
    impl Rule for AddRule {
        fn name(&self) -> &'static str {
            self.n
        }
        fn priority(&self) -> i32 {
            self.p
        }
    }
    impl CandidateRule for AddRule {
        fn apply(&self, _ctx: &Context, cands: &mut Vec<RuleCandidate>) -> RuleEffect {
            cands.push(RuleCandidate {
                word: self.word.to_string(),
                score: 1.0,
                source: "test",
            });
            RuleEffect::Added(1)
        }
    }

    struct PanicRule;
    impl Rule for PanicRule {
        fn name(&self) -> &'static str {
            "PanicRule"
        }
        fn priority(&self) -> i32 {
            999
        }
    }
    impl CandidateRule for PanicRule {
        fn apply(&self, _ctx: &Context, _cands: &mut Vec<RuleCandidate>) -> RuleEffect {
            panic!("intentional test panic")
        }
    }

    fn ctx() -> Context {
        Context {
            mode: Mode::Mixed,
            buffer: String::new(),
            prev_committed: None,
            second_prev_committed: None,
            flags: super::super::ContextFlags::default(),
        }
    }

    #[test]
    fn engine_runs_rules_in_priority_order() {
        let engine = CandidateRuleEngine::new(vec![
            Arc::new(AddRule {
                n: "second",
                p: 200,
                word: "B",
            }),
            Arc::new(AddRule {
                n: "first",
                p: 100,
                word: "A",
            }),
        ]);
        let mut cands = Vec::new();
        let trace = engine.run(&ctx(), &mut cands);
        assert_eq!(
            cands.iter().map(|c| c.word.as_str()).collect::<Vec<_>>(),
            vec!["A", "B"]
        );
        assert_eq!(trace.fired.len(), 2);
        assert_eq!(trace.fired[0].rule_name, "first");
        assert_eq!(trace.fired[1].rule_name, "second");
    }

    #[test]
    fn applies_false_skips_rule() {
        struct Gated;
        impl Rule for Gated {
            fn name(&self) -> &'static str {
                "Gated"
            }
            fn priority(&self) -> i32 {
                100
            }
        }
        impl CandidateRule for Gated {
            fn applies(&self, _: &Context) -> bool {
                false
            }
            fn apply(&self, _: &Context, cands: &mut Vec<RuleCandidate>) -> RuleEffect {
                cands.push(RuleCandidate {
                    word: "SHOULD_NOT_APPEAR".into(),
                    score: 0.0,
                    source: "test",
                });
                RuleEffect::Added(1)
            }
        }
        let engine = CandidateRuleEngine::new(vec![Arc::new(Gated)]);
        let mut cands = Vec::new();
        let trace = engine.run(&ctx(), &mut cands);
        assert!(cands.is_empty());
        assert_eq!(trace.fired.len(), 0);
        assert_eq!(trace.skipped.len(), 1);
        assert_eq!(trace.skipped[0].rule_name, "Gated");
    }

    #[test]
    fn panicking_rule_does_not_block_subsequent_rules() {
        let engine = CandidateRuleEngine::new(vec![
            Arc::new(PanicRule),
            Arc::new(AddRule {
                n: "after",
                p: 1000,
                word: "AFTER",
            }),
        ]);
        let mut cands = Vec::new();
        let trace = engine.run(&ctx(), &mut cands);
        // AFTER rule still ran.
        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].word, "AFTER");
        // Trace records the panic.
        assert_eq!(trace.fired.len(), 2);
        match &trace.fired[0].effect {
            RuleEffect::Failed(msg) => assert!(msg.contains("PanicRule")),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn rule_fired_lookup_works() {
        let engine = CandidateRuleEngine::new(vec![
            Arc::new(NoOpRule { n: "alpha", p: 100 }),
            Arc::new(NoOpRule { n: "beta", p: 200 }),
        ]);
        let mut cands = Vec::new();
        let trace = engine.run(&ctx(), &mut cands);
        assert!(trace.rule_fired("alpha"));
        assert!(trace.rule_fired("beta"));
        assert!(!trace.rule_fired("gamma"));
    }
}
