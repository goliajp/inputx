//! Wubi engine — state machine over a Wubi 86 table.
//!
//! v1.5.2 WU-λ carve-out: pre-v1.5.2 this lived in the
//! `inputx-wubi-cement` crate as part of the v1.4.5 cement carve.
//! Per the v1.5 D11 taxonomy correction (cement = application source,
//! NOT a published crate), the stateful state machine + auto-commit
//! policy belong inside the Inputx application (inputx-core),
//! mirroring the v1.5.1 `crate::japanese` carve. Data + low-level
//! lookup helpers (table::lookup et al, `EMBEDDED_WUBI_IDF`,
//! `wubi_idf_reader`, `layer_from_idf_tag`) stay in
//! `inputx-wubi-cement` as those are stone-quality (no business
//! glue, no per-session state).
//!
//! Side-effect-free with respect to commit delivery: methods that
//! result in committed text return `Option<String>` directly. The
//! `Session` layer is the single owner of pending-commit state.

use inputx_wubi_data as cement;

/// Policy controlling when the engine auto-commits without explicit user
/// selection. Default: `OnFourCodesIfUnique`.
#[repr(u32)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum AutoCommitPolicy {
    Never = 0,
    OnFourCodes = 1,
    OnUniqueMatch = 2,
    #[default]
    OnFourCodesIfUnique = 3,
}

impl AutoCommitPolicy {
    pub fn from_u32(v: u32) -> Option<Self> {
        match v {
            0 => Some(Self::Never),
            1 => Some(Self::OnFourCodes),
            2 => Some(Self::OnUniqueMatch),
            3 => Some(Self::OnFourCodesIfUnique),
            _ => None,
        }
    }
}

const MAX_CODE_LEN: usize = 4;

pub struct WubiEngine {
    buffer: Vec<u8>,
    candidates: Vec<String>,
    policy: AutoCommitPolicy,
}

impl Default for WubiEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl WubiEngine {
    pub fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(MAX_CODE_LEN),
            candidates: Vec::new(),
            policy: AutoCommitPolicy::default(),
        }
    }

    pub fn policy(&self) -> AutoCommitPolicy {
        self.policy
    }

    pub fn set_policy(&mut self, p: AutoCommitPolicy) {
        self.policy = p;
    }

    pub fn is_composing(&self) -> bool {
        !self.buffer.is_empty()
    }

    pub fn buffer_str(&self) -> &str {
        // SAFETY: only ASCII alphabetic bytes are ever pushed.
        std::str::from_utf8(&self.buffer).expect("buffer is ASCII by construction")
    }

    pub fn candidates(&self) -> &[String] {
        &self.candidates
    }

    /// Scored candidates for the cross-engine merge. Re-queries the
    /// dict via `cement::lookup_with_scores` so the composite layer
    /// can sort uniformly across wubi / pinyin / jp. Score formula:
    /// `layer.base × pref + freq`, with the existing wubi-internal
    /// promote rules (single-char-beats-phrase at full code, L0 pin)
    /// folded in as score multipliers.
    pub fn candidates_with_scores(&self) -> Vec<(String, f64)> {
        if self.buffer.is_empty() {
            return Vec::new();
        }
        cement::lookup_with_scores(self.buffer_str())
    }

    /// Layer-aware scored candidates. Returns `(word, score, Layer)`
    /// so the composite dispatch can apply layer-specific ranking
    /// rules — notably demoting low-confidence Auto / Phrase entries
    /// at short pinyin-shaped buffers without touching high-confidence
    /// Jianma1/2/3 + Zigen simcodes (the 伙-rule).
    pub fn candidates_with_layer(&self) -> Vec<(String, f64, inputx_wubi::Layer)> {
        if self.buffer.is_empty() {
            return Vec::new();
        }
        cement::lookup_with_layer(self.buffer_str())
    }

    /// Candidates with raw frequency exposed (separate from layer.base ·
    /// pref). Caller composes the orthodox three-axis fill:
    ///   log_prior_q4 = Q4·ln(1 + freq)
    ///   log_likelihood_q4 = Q4·ln(layer.base() · pref · layer_demote
    ///                             · wubi_length_modifier · z_mult)
    /// for v1.4.7 sort-key cutover to probability-native ranking.
    /// (2026-06-03: `char_demote` multiplier retired — prominence is
    /// expressed as a tier shift in `composite/dispatch.rs`, not a
    /// within-tier likelihood factor.)
    pub fn candidates_with_freq_layer(&self) -> Vec<(String, inputx_wubi::Layer, u64)> {
        if self.buffer.is_empty() {
            return Vec::new();
        }
        cement::lookup_with_freq_layer(self.buffer_str())
    }

    /// User-pinned word for the *current* wubi buffer, if any. The
    /// composite dispatch layer re-applies the L0 pin × 1000 multiplier
    /// because `candidates_with_freq_layer` returns raw per-entry data
    /// without pin promotion baked in (pre-v1.10 the wubi facade's own
    /// `lookup_with_scores_into` baked it; cement-layer carve moved the
    /// per-engine business rules into composite/dispatch.rs but missed
    /// the pin re-apply — fixed in the same v1.10 cycle once symptoms
    /// surfaced).
    pub fn pinned_word_for_buffer(&self) -> Option<String> {
        if self.buffer.is_empty() {
            return None;
        }
        cement::pinned_word(self.buffer_str())
    }

    /// Prefix-prediction candidates for the current buffer:
    /// `(word, freq, code_len)` for every dict entry whose code
    /// strictly extends the buffer (exact-code matches excluded).
    /// Caller composes the final score via `scoring::predict_score`.
    /// Empty when the buffer is empty.
    pub fn prefix_predictions(&self) -> Vec<(String, u64, usize)> {
        if self.buffer.is_empty() {
            return Vec::new();
        }
        cement::prefix_predictions(self.buffer_str())
    }

    /// Prefix-prediction candidates for an arbitrary prefix (not bound
    /// to the engine's current buffer). Used by the composite Mixed-
    /// mode dispatcher's Phase I "full-code redundancy" check — at a
    /// 4-letter buffer we need to know which words are ALSO reachable
    /// via the 3-letter prefix's prediction list so we can suppress
    /// the full-code-only score boost when the wubi user already has
    /// a shorter path to the same word. Empty when the prefix is empty.
    pub fn prefix_predictions_for(prefix: &str) -> Vec<(String, u64, usize)> {
        if prefix.is_empty() {
            return Vec::new();
        }
        cement::prefix_predictions(prefix)
    }

    /// Feed one Wubi-relevant letter. Returns text to commit, if any
    /// (forced commit when buffer was already full, and/or auto-commit
    /// triggered by `AutoCommitPolicy`). May concatenate two commits
    /// in the rare 5th-letter-with-auto-commit case.
    ///
    /// Returns `None` if the input is not an ASCII alphabetic byte
    /// (engine took no action).
    pub fn handle_letter(&mut self, byte: u8) -> Option<String> {
        if !byte.is_ascii_alphabetic() {
            return None;
        }
        let mut out: Option<String> = None;

        if self.buffer.len() >= MAX_CODE_LEN {
            if let Some(top) = self.candidates.first().cloned() {
                cement::record_pick(self.buffer_str(), &top);
                push_str_into(&mut out, &top);
            }
            self.buffer.clear();
            self.candidates.clear();
        }

        self.buffer.push(byte.to_ascii_lowercase());
        self.refresh_candidates();
        if let Some(text) = self.maybe_auto_commit() {
            push_str_into(&mut out, &text);
        }
        out
    }

    /// Pop one buffered letter. Returns whether the keystroke was consumed.
    pub fn backspace(&mut self) -> bool {
        if self.buffer.is_empty() {
            return false;
        }
        self.buffer.pop();
        self.refresh_candidates();
        true
    }

    /// Discard composing state. Returns whether the keystroke was consumed.
    pub fn escape(&mut self) -> bool {
        if self.buffer.is_empty() {
            return false;
        }
        self.buffer.clear();
        self.candidates.clear();
        true
    }

    /// Manually commit candidate at `index`. Returns the text and resets
    /// composing state. Returns `None` if `index` is out of range.
    pub fn commit_index(&mut self, index: usize) -> Option<String> {
        let text = self.candidates.get(index)?.clone();
        cement::record_pick(self.buffer_str(), &text);
        self.buffer.clear();
        self.candidates.clear();
        Some(text)
    }

    pub fn clear_all(&mut self) {
        self.buffer.clear();
        self.candidates.clear();
    }

    fn refresh_candidates(&mut self) {
        self.candidates.clear();
        if self.buffer.is_empty() {
            return;
        }
        self.candidates = cement::lookup(self.buffer_str());
    }

    fn maybe_auto_commit(&mut self) -> Option<String> {
        let four = self.buffer.len() == MAX_CODE_LEN;
        let unique = self.candidates.len() == 1;
        let trigger = match self.policy {
            AutoCommitPolicy::Never => false,
            AutoCommitPolicy::OnFourCodes => four && !self.candidates.is_empty(),
            AutoCommitPolicy::OnUniqueMatch => unique,
            AutoCommitPolicy::OnFourCodesIfUnique => four && unique,
        };
        if !trigger {
            return None;
        }
        let text = self.candidates.first().cloned();
        if let Some(t) = text.as_deref() {
            cement::record_pick(self.buffer_str(), t);
        }
        self.buffer.clear();
        self.candidates.clear();
        text
    }
}

fn push_str_into(slot: &mut Option<String>, s: &str) {
    match slot {
        Some(existing) => existing.push_str(s),
        None => *slot = Some(s.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine_with(policy: AutoCommitPolicy) -> WubiEngine {
        let mut e = WubiEngine::new();
        e.set_policy(policy);
        e
    }

    #[test]
    fn one_letter_yi_ji_jian_ma_with_unique_match_auto_commits() {
        let mut e = engine_with(AutoCommitPolicy::OnUniqueMatch);
        assert_eq!(e.handle_letter(b'g').as_deref(), Some("一"));
        assert!(!e.is_composing());
    }

    #[test]
    fn full_4_letter_zigen_with_default_policy_auto_commits() {
        let mut e = WubiEngine::new();
        assert_eq!(e.handle_letter(b'i'), None);
        assert_eq!(e.handle_letter(b'p'), None);
        assert_eq!(e.handle_letter(b'b'), None);
        assert_eq!(e.handle_letter(b'f').as_deref(), Some("学"));
        assert!(!e.is_composing());
    }

    #[test]
    fn under_4_letters_does_not_auto_commit_under_if_unique() {
        let mut e = WubiEngine::new();
        assert_eq!(e.handle_letter(b'a'), None);
        assert!(e.is_composing());
        assert_eq!(e.buffer_str(), "a");
        assert!(!e.candidates().is_empty());
    }

    #[test]
    fn on_four_codes_commits_top_at_four_letters() {
        let mut e = engine_with(AutoCommitPolicy::OnFourCodes);
        assert_eq!(e.handle_letter(b'd'), None);
        assert_eq!(e.handle_letter(b'd'), None);
        assert_eq!(e.handle_letter(b'd'), None);
        assert_eq!(e.handle_letter(b'd').as_deref(), Some("大"));
    }

    #[test]
    fn never_policy_never_auto_commits() {
        let mut e = engine_with(AutoCommitPolicy::Never);
        for ch in b"ipbf" {
            assert_eq!(e.handle_letter(*ch), None);
        }
        assert!(e.is_composing());
        assert_eq!(e.commit_index(0).as_deref(), Some("学"));
        assert!(!e.is_composing());
    }

    #[test]
    fn fifth_letter_force_commits_top_then_starts_fresh() {
        let mut e = engine_with(AutoCommitPolicy::Never);
        for ch in b"ipbf" {
            e.handle_letter(*ch);
        }
        let out = e.handle_letter(b'd');
        assert_eq!(out.as_deref(), Some("学"));
        assert_eq!(e.buffer_str(), "d");
        assert_eq!(e.candidates().first().map(String::as_str), Some("在"));
    }

    #[test]
    fn backspace_pops_buffer_only_while_composing() {
        let mut e = engine_with(AutoCommitPolicy::Never);
        assert!(!e.backspace());
        e.handle_letter(b'g');
        e.handle_letter(b'g');
        assert!(e.backspace());
        assert_eq!(e.buffer_str(), "g");
        assert_eq!(e.candidates(), &["一".to_string()]);
        assert!(e.backspace());
        assert!(!e.is_composing());
        assert!(!e.backspace());
    }

    #[test]
    fn escape_clears_composing_state() {
        let mut e = engine_with(AutoCommitPolicy::Never);
        assert!(!e.escape());
        e.handle_letter(b'd');
        e.handle_letter(b'd');
        assert!(e.escape());
        assert!(!e.is_composing());
    }

    #[test]
    fn commit_index_picks_specific_candidate() {
        let mut e = engine_with(AutoCommitPolicy::Never);
        e.handle_letter(b'j');
        e.handle_letter(b'e');
        e.handle_letter(b'g');
        assert!(
            e.candidates().len() >= 2,
            "expected ≥2 candidates for `jeg`, got {:?}",
            e.candidates()
        );
        let second = e.candidates()[1].clone();
        assert_eq!(e.commit_index(1).as_deref(), Some(second.as_str()));
        assert!(!e.is_composing());
    }

    #[test]
    fn commit_out_of_range_returns_none_and_keeps_state() {
        let mut e = engine_with(AutoCommitPolicy::Never);
        e.handle_letter(b'd');
        e.handle_letter(b'd');
        assert!(e.commit_index(99).is_none());
        assert!(e.is_composing());
    }

    #[test]
    fn non_letter_byte_rejected() {
        let mut e = WubiEngine::new();
        assert_eq!(e.handle_letter(b'1'), None);
        assert_eq!(e.handle_letter(b' '), None);
        assert_eq!(e.handle_letter(b'!'), None);
        assert!(!e.is_composing());
    }
}
