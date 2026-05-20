//! `PinyinEngine` — immutable assembly of dict + fuzzy config.
//!
//! Designed to be cheap to construct (the dict is from `include_bytes!` so
//! cloning the engine itself just re-validates the FST header) and shareable
//! across many concurrent [`Session`](crate::session::Session)s.

use crate::dict::PinyinDict;
use crate::fuzzy::FuzzyConfig;

/// Immutable bundle of the dict + the user's fuzzy preferences. One per
/// process; share by reference into [`Session`](crate::session::Session).
pub struct PinyinEngine {
    dict: PinyinDict,
    fuzzy: FuzzyConfig,
}

impl PinyinEngine {
    /// Build with the embedded dict and strict (no-fuzzy) defaults.
    pub fn new() -> Self {
        Self {
            dict: PinyinDict::embedded(),
            fuzzy: FuzzyConfig::strict(),
        }
    }

    /// Build with a custom fuzzy config.
    pub fn with_fuzzy(fuzzy: FuzzyConfig) -> Self {
        Self {
            dict: PinyinDict::embedded(),
            fuzzy,
        }
    }

    /// Borrowed view of the underlying dict — sessions and tests use this
    /// for direct lookups.
    pub fn dict(&self) -> &PinyinDict {
        &self.dict
    }

    /// Current fuzzy config.
    pub fn fuzzy(&self) -> FuzzyConfig {
        self.fuzzy
    }
}

impl Default for PinyinEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_loads_dict() {
        let e = PinyinEngine::new();
        assert!(e.dict().len() >= 50);
    }

    #[test]
    fn with_fuzzy_keeps_dict() {
        let e = PinyinEngine::with_fuzzy(FuzzyConfig::permissive());
        assert!(e.dict().len() >= 50);
        assert!(e.fuzzy().z_zh);
    }
}
