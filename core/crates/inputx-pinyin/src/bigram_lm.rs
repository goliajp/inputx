//! Phase 2 bigram language-model backend — pluggable trait + KenLM impl.
//!
//! Phase 2 of the pinyin climb plan wires a real bigram LM into the
//! Viterbi composition path. This module defines the abstract
//! [`LmBackend`] trait that callers depend on, plus two concrete
//! implementations:
//!
//! - [`NoopLm`] — always returns `0.0`, the historical (no-LM) behaviour.
//!   This is the default when the `kenlm` feature is off, so the rest of
//!   the engine compiles + runs identically to the pre-Phase-2 build.
//! - [`kenlm::KenLmBackend`] — under the optional `kenlm` Cargo feature.
//!   Wraps the [`kenlm-rs`] crate (LGPL-3.0-or-later), which builds the
//!   query-only KenLM C++ sources in-tree. Loads the Phase-2 trained
//!   `bigram.binary` (see `tools/scoring/09_bigram_lm/train.sh`).
//!
//! The feature gate is what keeps the LGPL footprint optional —
//! downstream `inputx-pinyin` consumers who build with default features
//! never link any KenLM code. The Inputx mac app turns the feature on
//! at bundle-build time; the data file (`bigram.binary`) ships
//! separately and is resolved at runtime, not compiled in.
//!
//! Acceptance reference (climb-plan CP-2.3 smoke test, 2026-06-15):
//!
//!   middle bigram log10 P("人" | "中国") = -1.8044
//!   middle bigram log10 P("嘎" | "中国") = -6.2038
//!
//! The integration test below checks the KenLM backend against those
//! C++ reference numbers (tolerance 0.01).

use core::fmt::Debug;

/// Pluggable bigram log-prob backend used by the Phase-2 Viterbi path.
///
/// Implementors must be `Send + Sync` because [`LmBackend`] is queried
/// from rayon worker threads inside the composition lattice.
pub trait LmBackend: Send + Sync + Debug {
    /// Return `log10 P(curr | prev)` for a bigram. Returns
    /// [`f32::NEG_INFINITY`] for genuine OOV pairs (or a finite back-off
    /// score, depending on smoothing — KenLM gives a finite KN back-off,
    /// the noop backend always returns 0.0).
    fn log_prob(&self, prev: &str, curr: &str) -> f32;

    /// Whether this backend actually scores. `false` means the caller
    /// can skip the LM term entirely and not pay the lookup cost.
    fn enabled(&self) -> bool {
        true
    }
}

/// Phase-0/1 fallback backend: always 0.0, never claims to be enabled.
///
/// Used both when the `kenlm` feature is off (no LM at all) and as the
/// safe default before [`kenlm::KenLmBackend::load`] succeeds at startup.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopLm;

impl LmBackend for NoopLm {
    fn log_prob(&self, _prev: &str, _curr: &str) -> f32 {
        0.0
    }
    fn enabled(&self) -> bool {
        false
    }
}

#[cfg(feature = "kenlm")]
pub mod kenlm_backend {
    //! Real KenLM bigram backend behind the optional `kenlm` feature.
    //!
    //! Named `kenlm_backend` (not `kenlm`) because the external
    //! `kenlm-rs` crate's library name is `kenlm`, and we want to
    //! refer to it as `::kenlm::Model` inside this module without
    //! colliding with our own module name.

    use super::LmBackend;
    use core::fmt;
    use std::path::Path;

    /// A loaded KenLM bigram model. Cheap to clone via the underlying
    /// kenlm-rs handle being internally reference-counted.
    pub struct KenLmBackend {
        model: kenlm::Model,
    }

    impl fmt::Debug for KenLmBackend {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("KenLmBackend")
                .field("order", &self.model.order())
                .finish()
        }
    }

    impl KenLmBackend {
        /// Load a KenLM binary or ARPA model from disk.
        ///
        /// Use the Phase-2 trained model at
        /// `tools/scoring/09_bigram_lm/data/bigram.binary` (or wherever
        /// the runtime ships it). For ARPA the function will still work
        /// but load is slower; prefer the trie binary for production.
        pub fn load(path: impl AsRef<Path>) -> Result<Self, kenlm::KenlmError> {
            let model = kenlm::Model::new(path)?;
            Ok(Self { model })
        }

        /// Underlying n-gram order. For the Phase-2 build this is 2.
        pub fn order(&self) -> u8 {
            self.model.order()
        }
    }

    impl LmBackend for KenLmBackend {
        fn log_prob(&self, prev: &str, curr: &str) -> f32 {
            // KenLM is stateful. Pipeline:
            //   ① start in null-context state (no history)
            //   ② feed `prev` → out-state now represents "context = prev";
            //      the score returned is log P(prev | nothing) which we
            //      discard — we only want the conditioning side-effect.
            //   ③ feed `curr` from the prev-context state → returned
            //      score is log P(curr | prev), the bigram we want.
            //
            // We need three distinct State buffers because KenLM's
            // base_score requires in/out to be different allocations.
            let state_null = self.model.null_context_state();
            let mut state_after_prev = self.model.null_context_state();
            let mut state_after_curr = self.model.null_context_state();

            let prev_idx = match self.model.index(prev) {
                Ok(idx) => idx,
                Err(_) => return f32::NEG_INFINITY,
            };
            if self
                .model
                .base_score(&state_null, prev_idx, &mut state_after_prev)
                .is_err()
            {
                return f32::NEG_INFINITY;
            }

            let curr_idx = match self.model.index(curr) {
                Ok(idx) => idx,
                Err(_) => return f32::NEG_INFINITY,
            };
            self.model
                .base_score(&state_after_prev, curr_idx, &mut state_after_curr)
                .unwrap_or(f32::NEG_INFINITY)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_returns_zero_and_is_disabled() {
        let lm = NoopLm;
        assert_eq!(lm.log_prob("foo", "bar"), 0.0);
        assert!(!lm.enabled());
    }

    /// Cross-checks the Rust KenLM binding against KenLM C++ query.
    ///
    /// Requires the Phase-2 trained `bigram.binary` to exist at the
    /// canonical path. The file is gitignored (regenerable via
    /// `tools/scoring/09_bigram_lm/train.sh`), so this test is marked
    /// `#[ignore]` for stock `cargo test` runs and only fires on
    /// `cargo test -- --ignored` or under the Phase-2 CI lane.
    ///
    /// Reference numbers come from `query bigram.binary` smoke test
    /// in CP-2.3 (2026-06-15):
    ///   ("中国", "人")  bigram log10 P = -1.8043799   (well-formed)
    ///   ("中国", "嘎")  bigram log10 P = -6.2038174   (malformed)
    ///   ("我",   "是")  bigram log10 P ≈ ???          (3rd fixture)
    #[cfg(feature = "kenlm")]
    #[test]
    #[ignore = "needs tools/scoring/09_bigram_lm/data/bigram.binary; run with --ignored"]
    fn kenlm_bigram_log_prob_matches_cpp_reference() {
        use kenlm_backend::KenLmBackend;

        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../tools/scoring/09_bigram_lm/data/bigram.binary");
        let lm = KenLmBackend::load(&path).expect("load bigram.binary");
        assert_eq!(lm.order(), 2, "Phase-2 trained model is a bigram");

        // Fixture 1: well-formed bigram (well-known phrase)
        let good = lm.log_prob("中国", "人");
        assert!(
            (good - (-1.8044)).abs() < 0.01,
            "P(人|中国) Rust binding diverged from C++ reference: got {good}, expected ~-1.8044"
        );

        // Fixture 2: malformed bigram (semantically wrong successor)
        let bad = lm.log_prob("中国", "嘎");
        assert!(
            (bad - (-6.2038)).abs() < 0.01,
            "P(嘎|中国) Rust binding diverged from C++ reference: got {bad}, expected ~-6.2038"
        );

        // Fixture 3: another common phrase. We don't pin the numeric
        // value (it was not in the CP-2.3 smoke log), but assert it is
        // (a) finite — both tokens are in vocab, (b) higher than the
        // malformed bigram above — "我 是" is a far more common pair
        // than "中国 嘎".
        let normal = lm.log_prob("我", "是");
        assert!(
            normal.is_finite(),
            "P(是|我) returned non-finite {normal}"
        );
        assert!(
            normal > bad,
            "P(是|我)={normal} should be higher than the malformed P(嘎|中国)={bad}"
        );
    }

    /// CP-2.4 acceptance: cold-load time < 100 ms (mmap).
    #[cfg(feature = "kenlm")]
    #[test]
    #[ignore = "needs tools/scoring/09_bigram_lm/data/bigram.binary; run with --ignored"]
    fn kenlm_load_under_100ms_via_mmap() {
        use kenlm_backend::KenLmBackend;
        use std::time::Instant;

        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../tools/scoring/09_bigram_lm/data/bigram.binary");
        let t = Instant::now();
        let _ = KenLmBackend::load(&path).expect("load bigram.binary");
        let ms = t.elapsed().as_millis();
        assert!(
            ms < 100,
            "cold-load took {ms} ms (acceptance gate: < 100 ms via mmap)"
        );
    }
}
