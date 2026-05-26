//! Prediction-rule trait + engine (v3.0.1 scaffolding).
//!
//! `PredictionRule` is the next-word-prediction analog of CandidateRule.
//! Each rule may add / filter prediction candidates. Examples (v3.0.3
//! migration):
//!   - `TrigramContextLookup` — fires when prev_prev is Some, queries
//!     trigram FST, adds matches.
//!   - `BigramFallbackCold` — fires only when prev_prev is None.
//!   - `MinCountFilter` — strips predictions with count < threshold.
//!   - `IntraTokenExclude` — (in v1.3 lives at the DATA layer via FST
//!     inter/intra split; in v3.0.3 may move here as a runtime check
//!     for completeness).

use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Instant;

use super::{Context, ExecutionTrace, Rule, RuleEffect, TraceEntry};

/// A prediction is a candidate next word with a count score.
#[derive(Clone, Debug)]
pub struct PredictionCandidate {
    pub word: String,
    /// Raw count from bigram/trigram FST (NOT a normalized score —
    /// keeps the natural integer order across rules).
    pub count: u64,
    pub source: &'static str,
}

pub trait PredictionRule: Rule {
    fn applies(&self, _ctx: &Context) -> bool {
        true
    }
    fn apply(&self, ctx: &Context, predictions: &mut Vec<PredictionCandidate>) -> RuleEffect;
}

pub struct PredictionRuleEngine {
    rules: Vec<Arc<dyn PredictionRule>>,
}

impl PredictionRuleEngine {
    pub fn new(mut rules: Vec<Arc<dyn PredictionRule>>) -> Self {
        rules.sort_by_key(|r| r.priority());
        Self { rules }
    }

    pub fn run(
        &self,
        ctx: &Context,
        predictions: &mut Vec<PredictionCandidate>,
    ) -> ExecutionTrace {
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
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                rule.apply(ctx, predictions)
            }));
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

    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composite::Mode;

    struct ColdOnlyRule;
    impl Rule for ColdOnlyRule {
        fn name(&self) -> &'static str { "ColdOnly" }
        fn priority(&self) -> i32 { 100 }
    }
    impl PredictionRule for ColdOnlyRule {
        fn applies(&self, ctx: &Context) -> bool {
            ctx.second_prev_committed.is_none()
        }
        fn apply(&self, _ctx: &Context, preds: &mut Vec<PredictionCandidate>) -> RuleEffect {
            preds.push(PredictionCandidate {
                word: "FROM_COLD".into(),
                count: 100,
                source: "test",
            });
            RuleEffect::Added(1)
        }
    }

    #[test]
    fn applies_predicate_filters_by_context() {
        let engine = PredictionRuleEngine::new(vec![Arc::new(ColdOnlyRule)]);
        // Cold context: rule fires.
        let mut p = Vec::new();
        let trace = engine.run(&Context {
            mode: Mode::Mixed,
            buffer: String::new(),
            prev_committed: Some("好".into()),
            second_prev_committed: None,
            flags: super::super::ContextFlags::default(),
        }, &mut p);
        assert_eq!(p.len(), 1);
        assert!(trace.rule_fired("ColdOnly"));
        // Hot context (both prevs set): rule skipped.
        let mut p2 = Vec::new();
        let trace2 = engine.run(&Context {
            mode: Mode::Mixed,
            buffer: String::new(),
            prev_committed: Some("好".into()),
            second_prev_committed: Some("你".into()),
            flags: super::super::ContextFlags::default(),
        }, &mut p2);
        assert!(p2.is_empty());
        assert!(trace2.rule_skipped("ColdOnly"));
    }
}
