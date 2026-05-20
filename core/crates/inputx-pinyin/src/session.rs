//! Per-input mutable state — accumulates the user's typing buffer and exposes
//! candidates / commit semantics.
//!
//! Mirrors the [`inputx-wubi`](https://crates.io/crates/inputx-wubi) session
//! surface so a composite engine (e.g. the Inputx IME's `inputx-core`) can
//! dispatch between both engines uniformly.

use crate::dict::PinyinDict;
use crate::engine::PinyinEngine;
use crate::fuzzy::FuzzyConfig;

/// One typing session: holds the partial pinyin string the user has typed
/// so far, exposes candidates, and commits on selection. Borrows the
/// [`PinyinEngine`] for the dict + fuzzy config.
pub struct Session<'e> {
    engine: &'e PinyinEngine,
    input: String,
    /// Reused candidate buffer to avoid per-keystroke allocs.
    cand_buf: Vec<String>,
}

impl<'e> Session<'e> {
    /// Open a session against `engine`. The session holds a borrow for its
    /// lifetime; the engine itself is `Send + Sync`-compatible (FST is
    /// `'static`, fuzzy is `Copy`).
    pub fn new(engine: &'e PinyinEngine) -> Self {
        Self {
            engine,
            input: String::new(),
            cand_buf: Vec::with_capacity(16),
        }
    }

    /// Append one ASCII character to the input buffer. Non-ASCII or
    /// non-letter characters are silently ignored — the IME shell is
    /// responsible for filtering at the keyboard layer.
    pub fn input_char(&mut self, c: char) {
        if c.is_ascii_alphabetic() {
            self.input.push(c.to_ascii_lowercase());
        }
    }

    /// Drop the last input character, if any. Returns whether anything was
    /// removed.
    pub fn backspace(&mut self) -> bool {
        self.input.pop().is_some()
    }

    /// Clear the entire input buffer (e.g., on Esc).
    pub fn reset(&mut self) {
        self.input.clear();
    }

    /// The raw input string typed so far.
    pub fn input(&self) -> &str {
        &self.input
    }

    /// Candidates for the current input, considering fuzzy expansion. Result
    /// is borrowed from the session's reused buffer; subsequent calls
    /// invalidate the previous slice.
    ///
    /// Empty input yields an empty slice.
    pub fn candidates(&mut self) -> &[String] {
        if self.input.is_empty() {
            self.cand_buf.clear();
            return &self.cand_buf;
        }
        let input = self.input.clone();
        Self::lookup_with_fuzzy(self.engine, &input, &mut self.cand_buf);
        &self.cand_buf
    }

    /// Same as [`Self::candidates`] but writes into a caller-owned buffer.
    /// Useful for FFI callers that own the buffer's lifetime independently
    /// of the session.
    pub fn lookup_into(&self, out: &mut Vec<String>) {
        if self.input.is_empty() {
            out.clear();
            return;
        }
        Self::lookup_with_fuzzy(self.engine, &self.input, out);
    }

    fn lookup_with_fuzzy(engine: &PinyinEngine, input: &str, out: &mut Vec<String>) {
        out.clear();
        let dict = engine.dict();
        let fuzzy = engine.fuzzy();
        // For v0.1: fuzzy applies to the WHOLE input (treating it as one
        // syllable). v0.2 will integrate the segmenter so fuzzy works
        // per-syllable in multi-syllable inputs. The current behavior is
        // correct for short/single-syllable inputs and a no-op when fuzzy
        // is strict — which is the default.
        let alternates = expand_full_input(fuzzy, input);
        let mut local = Vec::with_capacity(8);
        for variant in alternates {
            dict.lookup_into(&variant, &mut local);
            for w in local.drain(..) {
                if !out.contains(&w) {
                    out.push(w);
                }
            }
        }
    }

    /// Commit the candidate at index `idx`, returning the committed word and
    /// resetting the input buffer. Out-of-range indices yield `None` and
    /// leave the session untouched.
    ///
    /// Side effect: records the pick in the engine's L0 layer (item 28).
    /// Three consecutive picks of the same `(input, word)` auto-pin it to
    /// position 0 for that input. Pinyin v0.2 ignores the fuzzy expansion
    /// for L0 attribution — record uses the literal user input string,
    /// matching what `dict.exists_in_l1` will accept.
    pub fn commit(&mut self, idx: usize) -> Option<String> {
        let cands = self.candidates();
        let word = cands.get(idx).cloned()?;
        // Record before clearing so we still have the input string.
        self.engine.dict().record_pick(&self.input, &word);
        self.input.clear();
        self.cand_buf.clear();
        Some(word)
    }
}

fn expand_full_input(fuzzy: FuzzyConfig, input: &str) -> Vec<String> {
    // Skip the wrapper Vec when fuzzy is fully strict — the common case.
    if matches!(
        fuzzy,
        FuzzyConfig {
            z_zh: false,
            c_ch: false,
            s_sh: false,
            n_l: false,
            f_h: false,
            r_l: false,
            in_ing: false,
            en_eng: false,
            an_ang: false
        }
    ) {
        return vec![input.to_string()];
    }
    fuzzy.expand(input)
}

// Internal helper for tests that want a quick `lookup` without managing the
// session lifecycle.
#[allow(dead_code)]
fn quick_lookup(dict: &PinyinDict, pinyin: &str) -> Vec<String> {
    dict.lookup(pinyin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_and_lookup_zhongguo() {
        let engine = PinyinEngine::new();
        let mut session = Session::new(&engine);
        for c in "zhongguo".chars() {
            session.input_char(c);
        }
        let cands = session.candidates();
        assert_eq!(cands.first().map(String::as_str), Some("中国"));
    }

    #[test]
    fn backspace_shrinks_input() {
        let engine = PinyinEngine::new();
        let mut session = Session::new(&engine);
        for c in "abc".chars() {
            session.input_char(c);
        }
        assert_eq!(session.input(), "abc");
        assert!(session.backspace());
        assert_eq!(session.input(), "ab");
        assert!(session.backspace());
        assert!(session.backspace());
        assert!(!session.backspace()); // empty now
    }

    #[test]
    fn commit_returns_word_and_resets() {
        let engine = PinyinEngine::new();
        let mut session = Session::new(&engine);
        for c in "wo".chars() {
            session.input_char(c);
        }
        let committed = session.commit(0);
        assert_eq!(committed.as_deref(), Some("我"));
        assert_eq!(session.input(), "");
        assert!(session.candidates().is_empty());
    }

    #[test]
    fn commit_out_of_range_is_noop() {
        let engine = PinyinEngine::new();
        let mut session = Session::new(&engine);
        for c in "wo".chars() {
            session.input_char(c);
        }
        // Use clearly-out-of-range index — full v0.2 dict has 100+ candidates
        // for some single-syllable inputs, so the bootstrap-era `99` was no
        // longer "obviously past the end".
        assert!(session.commit(999_999).is_none());
        assert_eq!(session.input(), "wo");
    }

    #[test]
    fn input_char_filters_non_ascii() {
        let engine = PinyinEngine::new();
        let mut session = Session::new(&engine);
        session.input_char('z');
        session.input_char('中'); // ignored
        session.input_char('h');
        assert_eq!(session.input(), "zh");
    }

    #[test]
    fn fuzzy_expands_lookup() {
        // With z↔zh fuzzy on, typing `zong` should also match `zhong`.
        let engine = PinyinEngine::with_fuzzy(FuzzyConfig {
            z_zh: true,
            ..FuzzyConfig::default()
        });
        let mut session = Session::new(&engine);
        for c in "zong".chars() {
            session.input_char(c);
        }
        // `zong` itself isn't in bootstrap; expansion picks up `zhong` instead.
        let cands = session.candidates();
        assert!(
            cands.iter().any(|w| w == "中"),
            "expected fuzzy z→zh to find 中, got {cands:?}"
        );
    }

    #[test]
    fn empty_input_no_candidates() {
        let engine = PinyinEngine::new();
        let mut session = Session::new(&engine);
        assert!(session.candidates().is_empty());
    }

    #[test]
    fn reset_clears_input() {
        let engine = PinyinEngine::new();
        let mut session = Session::new(&engine);
        for c in "abc".chars() {
            session.input_char(c);
        }
        session.reset();
        assert_eq!(session.input(), "");
    }

    /// Item 28 — committing through Session feeds the engine's L0. After
    /// PROMOTE_THRESHOLD repeats, the picked candidate is auto-pinned.
    /// Gated to default features: bootstrap dict has too few candidates
    /// per input to make pin-vs-default observable.
    #[cfg(not(feature = "bootstrap_only"))]
    #[test]
    fn commit_feeds_l0_pin_promotion() {
        use crate::ranking::PROMOTE_THRESHOLD;
        let engine = PinyinEngine::new();

        // Pick a non-default candidate for "shi" via a fresh session each time.
        // 时 isn't first in v0.2 (是 dominates), so promoting it via picks
        // visibly changes the lookup ordering.
        let target = "时";
        for _ in 0..PROMOTE_THRESHOLD {
            let mut s = Session::new(&engine);
            for c in "shi".chars() {
                s.input_char(c);
            }
            let cands = s.candidates();
            let idx = cands
                .iter()
                .position(|w| w == target)
                .expect("时 should be a 'shi' candidate");
            assert_eq!(s.commit(idx).as_deref(), Some(target));
        }

        // After threshold picks, 时 should now be at position 0 for "shi".
        let mut probe = Session::new(&engine);
        for c in "shi".chars() {
            probe.input_char(c);
        }
        assert_eq!(
            probe.candidates().first().map(String::as_str),
            Some(target),
            "expected L0 to pin 时 after {PROMOTE_THRESHOLD} picks via Session::commit"
        );
    }
}
