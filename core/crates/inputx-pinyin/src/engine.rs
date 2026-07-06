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
            dict: PinyinDict::embedded().with_lm_from_env(),
            fuzzy: FuzzyConfig::strict(),
        }
    }

    /// Build with a custom fuzzy config.
    pub fn with_fuzzy(fuzzy: FuzzyConfig) -> Self {
        Self {
            dict: PinyinDict::embedded().with_lm_from_env(),
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

    /// v1.15 hot-reload entry point. Replace `self.dict` with a fresh
    /// [`PinyinDict`] built from `map_bytes` (the raw bytes of a
    /// freshly-baked `pinyin.dict`), carrying over the per-session L0
    /// pins, cell-dict layer, and LM backend so a polish round doesn't
    /// destroy the user's typing memory.
    ///
    /// On parse failure the engine's dict is left untouched and the
    /// FST error is returned; callers should log and continue with
    /// the old dict rather than falling back to embedded (which would
    /// silently undo any polish already applied).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn reload_dict_from_bytes(
        &mut self,
        map_bytes: Vec<u8>,
    ) -> Result<(), inputx_fsa::FsaError> {
        let fresh = self.dict.reload_map_preserving(map_bytes)?;
        self.dict = fresh;
        Ok(())
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
