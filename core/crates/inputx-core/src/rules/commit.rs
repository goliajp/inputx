//! Commit-rule trait + engine (v3.0.1 scaffolding).
//!
//! A `CommitRule` answers: "given the current context + candidates,
//! should the engine auto-commit one of them and reset the buffer?"
//! Examples (v3.0.4 migration):
//!   - `WubiOnFourCodesIfUnique` — the existing wubi default policy.
//!   - `AsciiFallback` — 5+ char no-pinyin-match buffer commits as ASCII.
//!   - `CompositeOverrideNever` — composite engine forces wubi sub-
//!     engine into Never to prevent surprise commits, then applies
//!     these rules at the composite layer.
//!
//! Unlike candidate/prediction rules (which accumulate into a list),
//! commit rules SHORT-CIRCUIT: the first rule that returns
//! `Some(CommitDecision)` wins. Later rules don't run for that frame.

use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Instant;

use super::{Context, ExecutionTrace, Rule, RuleEffect, TraceEntry};
use super::candidate::RuleCandidate;

#[derive(Clone, Debug)]
pub struct CommitDecision {
    /// Word to commit (typically the top candidate).
    pub word: String,
    /// Static reason string for trace / debug logs.
    pub reason: &'static str,
}

pub trait CommitRule: Rule {
    fn applies(&self, _ctx: &Context) -> bool {
        true
    }
    /// Returns `Some(decision)` to trigger commit; `None` to defer to
    /// the next rule or no-commit.
    fn check(&self, ctx: &Context, cands: &[RuleCandidate]) -> Option<CommitDecision>;
}

pub struct CommitRuleEngine {
    rules: Vec<Arc<dyn CommitRule>>,
}

impl CommitRuleEngine {
    pub fn new(mut rules: Vec<Arc<dyn CommitRule>>) -> Self {
        rules.sort_by_key(|r| r.priority());
        Self { rules }
    }

    /// Return the first commit decision (if any) + the trace. Rules
    /// AFTER the winning one are recorded as `skipped` in the trace
    /// for debug clarity ("here's what would have run if the chosen
    /// rule hadn't fired").
    pub fn run(
        &self,
        ctx: &Context,
        cands: &[RuleCandidate],
    ) -> (Option<CommitDecision>, ExecutionTrace) {
        let mut trace = ExecutionTrace::default();
        let total_start = Instant::now();
        let mut decision = None;
        for rule in &self.rules {
            let entry_start = Instant::now();
            if decision.is_some() {
                // Short-circuit: a higher-priority rule already won.
                trace.skipped.push(TraceEntry {
                    rule_name: rule.name(),
                    priority: rule.priority(),
                    effect: RuleEffect::NoOp,
                    elapsed: entry_start.elapsed(),
                });
                continue;
            }
            if !rule.applies(ctx) {
                trace.skipped.push(TraceEntry {
                    rule_name: rule.name(),
                    priority: rule.priority(),
                    effect: RuleEffect::NoOp,
                    elapsed: entry_start.elapsed(),
                });
                continue;
            }
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                rule.check(ctx, cands)
            }));
            let (effect, this_decision) = match result {
                Ok(Some(d)) => {
                    let e = RuleEffect::AutoCommitted {
                        word: d.word.clone(),
                        reason: d.reason,
                    };
                    (e, Some(d))
                }
                Ok(None) => (RuleEffect::NoOp, None),
                Err(_) => (
                    RuleEffect::Failed(format!("{} panicked", rule.name())),
                    None,
                ),
            };
            trace.fired.push(TraceEntry {
                rule_name: rule.name(),
                priority: rule.priority(),
                effect,
                elapsed: entry_start.elapsed(),
            });
            if let Some(d) = this_decision {
                decision = Some(d);
                // Don't break — continue iterating so later rules are
                // recorded as `skipped` for the trace.
            }
        }
        trace.total_elapsed = total_start.elapsed();
        (decision, trace)
    }

    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composite::Mode;

    struct AlwaysCommit { n: &'static str, p: i32 }
    impl Rule for AlwaysCommit {
        fn name(&self) -> &'static str { self.n }
        fn priority(&self) -> i32 { self.p }
    }
    impl CommitRule for AlwaysCommit {
        fn check(&self, _: &Context, _: &[RuleCandidate]) -> Option<CommitDecision> {
            Some(CommitDecision {
                word: format!("from-{}", self.n),
                reason: "test",
            })
        }
    }

    struct NeverCommit { n: &'static str, p: i32 }
    impl Rule for NeverCommit {
        fn name(&self) -> &'static str { self.n }
        fn priority(&self) -> i32 { self.p }
    }
    impl CommitRule for NeverCommit {
        fn check(&self, _: &Context, _: &[RuleCandidate]) -> Option<CommitDecision> {
            None
        }
    }

    fn ctx() -> Context {
        Context {
            mode: Mode::Mixed,
            buffer: "x".into(),
            prev_committed: None,
            second_prev_committed: None,
            flags: super::super::ContextFlags::default(),
        }
    }

    #[test]
    fn first_winning_rule_short_circuits() {
        let engine = CommitRuleEngine::new(vec![
            Arc::new(NeverCommit { n: "skip-me", p: 100 }),
            Arc::new(AlwaysCommit { n: "first-winner", p: 200 }),
            Arc::new(AlwaysCommit { n: "later-loser", p: 300 }),
        ]);
        let (decision, trace) = engine.run(&ctx(), &[]);
        let d = decision.expect("expected a commit");
        assert_eq!(d.word, "from-first-winner");
        // skip-me fired and returned None (NoOp).
        assert!(trace.rule_fired("skip-me"));
        // first-winner fired and committed.
        assert!(trace.rule_fired("first-winner"));
        // later-loser was skipped because of short-circuit.
        assert!(trace.rule_skipped("later-loser"));
    }

    #[test]
    fn no_committing_rule_returns_none() {
        let engine = CommitRuleEngine::new(vec![
            Arc::new(NeverCommit { n: "a", p: 100 }),
            Arc::new(NeverCommit { n: "b", p: 200 }),
        ]);
        let (decision, trace) = engine.run(&ctx(), &[]);
        assert!(decision.is_none());
        assert_eq!(trace.fired.len(), 2);
    }
}
