//! Session — top-level state visible to the host. Wraps a `CompositeEngine`
//! (wubi + pinyin dual-engine) per Phase 4 of the iOS commercial-grade
//! roadmap. The public API + FFI surface stays compatible with the v0.1
//! wubi-only shape; new dual-engine knobs (`mode`, `candidate_source`,
//! `export_l0_json`) are additive.

use crate::composite::{Candidate, CompositeEngine, Mode, Source, l0_json};
use crate::locale::punct::SmartQuoteState;
use crate::wubi::{self, AutoCommitPolicy, L0Snapshot};

const MOD_CTRL: u32 = 1 << 1;
const MOD_CMD: u32 = 1 << 3;

const CP_BACKSPACE: u32 = 0x08;
const CP_DEL_FORWARD: u32 = 0x7F;
const CP_ESCAPE: u32 = 0x1B;
const CP_SPACE: u32 = b' ' as u32;

pub struct Session {
    composite: CompositeEngine,
    /// Cached candidate words from the most recent composite::candidates()
    /// call. Kept as `Vec<String>` so the legacy `candidates() -> &[String]`
    /// surface still works (FFI returns these one-by-one). Parallel to
    /// `source_cache` for the W/P indicator.
    cand_cache: Vec<String>,
    source_cache: Vec<Source>,
    /// Text the engine has decided should be committed but the host hasn't
    /// drained yet. Drained via `take_pending_commit`. Multiple commits
    /// within one keystroke get concatenated.
    pending_commit: Option<String>,
    /// Per-session smart-quote alternator (item 82). Each `"` / `'`
    /// flips between opening and closing CJK quote glyphs. Reset on
    /// `clear()` so a fresh document context starts assuming the next
    /// quote is opening.
    smart_quote: SmartQuoteState,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    pub fn new() -> Self {
        Self {
            composite: CompositeEngine::new(),
            cand_cache: Vec::with_capacity(16),
            source_cache: Vec::with_capacity(16),
            pending_commit: None,
            smart_quote: SmartQuoteState::new(),
        }
    }

    /// Map an ASCII quote char to its smart-CJK form, advancing state.
    /// Non-quote chars return `None`.
    pub fn smart_quote(&mut self, c: char) -> Option<char> {
        self.smart_quote.map(c)
    }

    /// Reset the smart-quote alternator (e.g., after `clear()` or when
    /// switching contexts).
    pub fn smart_quote_reset(&mut self) {
        self.smart_quote.reset();
    }

    pub fn set_auto_commit_policy(&mut self, p: AutoCommitPolicy) {
        self.composite.set_auto_commit_policy(p);
    }

    /// Pay every cold-init cost in the engine stack now, so the user's first
    /// keystroke never stalls on dict init / FST page fault / INITIALS_INDEX
    /// build. Safe to call from any thread once `Session` is constructed
    /// (the host wrapper serializes session access). Idempotent and cheap
    /// after the first call (~1ms on hot caches).
    ///
    /// Typical use: host front-end (Inputx iOS keyboard extension, future Mac
    /// IMK adapter, web wasm shell) dispatches this on a background thread
    /// at keyboard-load time so the main thread stays responsive.
    pub fn warmup(&mut self) {
        self.composite.warmup();
    }

    pub fn auto_commit_policy(&self) -> AutoCommitPolicy {
        self.composite.auto_commit_policy()
    }

    /// Current engine mode (Mixed / WubiOnly / PinyinOnly).
    pub fn mode(&self) -> Mode {
        self.composite.mode()
    }

    /// Switch engine mode. Composing state isn't cleared — mid-input
    /// switches "work" but candidates flicker. The iOS UX is to switch
    /// mode at app-level only; mid-input switch isn't reachable from the
    /// keyboard UI in v1.
    pub fn set_mode(&mut self, m: Mode) {
        self.composite.set_mode(m);
        self.refresh_caches();
    }

    /// Process a keystroke. Returns `true` if the IME consumed it.
    pub fn handle_key(&mut self, codepoint: u32, modifiers: u32) -> bool {
        if modifiers & (MOD_CTRL | MOD_CMD) != 0 {
            return false;
        }

        if let Some(c) = char::from_u32(codepoint)
            && c.is_ascii_alphabetic()
        {
            if let Some(text) = self.composite.handle_letter(c as u8) {
                self.append_pending(text);
            }
            self.refresh_caches();
            return true;
        }

        match codepoint {
            CP_SPACE => {
                if !self.composite.is_composing() {
                    return false;
                }
                if let Some(text) = self.composite.commit_index(0) {
                    self.append_pending(text);
                }
                self.refresh_caches();
                true
            }
            CP_BACKSPACE | CP_DEL_FORWARD => {
                let consumed = self.composite.backspace();
                if consumed {
                    self.refresh_caches();
                }
                consumed
            }
            CP_ESCAPE => {
                let consumed = self.composite.escape();
                if consumed {
                    self.refresh_caches();
                }
                consumed
            }
            cp if (b'0' as u32..=b'9' as u32).contains(&cp) => {
                if !self.composite.is_composing() {
                    return false;
                }
                let raw = (cp as u8 - b'0') as usize;
                let idx = if raw == 0 { 9 } else { raw - 1 };
                if let Some(text) = self.composite.commit_index(idx) {
                    self.append_pending(text);
                }
                self.refresh_caches();
                // Swallow digits while composing even if no candidate at idx,
                // to avoid leaking digits into the client mid-composition.
                true
            }
            _ => {
                // Other characters (punctuation etc.): if composing, force-
                // commit top and pass the punctuation through.
                if self.composite.is_composing() {
                    if !self.cand_cache.is_empty() {
                        let top = self.cand_cache[0].clone();
                        self.append_pending(top);
                    }
                    self.composite.escape();
                    self.refresh_caches();
                }
                false
            }
        }
    }

    pub fn preedit(&self) -> &str {
        self.composite.preedit()
    }

    pub fn candidates(&self) -> &[String] {
        &self.cand_cache
    }

    pub fn candidate_count(&self) -> usize {
        self.cand_cache.len()
    }

    /// Source byte for the candidate at `index` — 0 = Wubi, 1 = Pinyin.
    /// Returns `None` if the index is out of range.
    pub fn candidate_source(&self, index: usize) -> Option<u8> {
        self.source_cache.get(index).map(|s| s.as_u8())
    }

    pub fn commit_index(&mut self, index: usize) -> Option<String> {
        let r = self.composite.commit_index(index);
        self.refresh_caches();
        r
    }

    pub fn take_pending_commit(&mut self) -> Option<String> {
        self.pending_commit.take()
    }

    pub fn clear(&mut self) {
        self.composite.clear_all();
        self.cand_cache.clear();
        self.source_cache.clear();
        self.pending_commit = None;
        // Reset smart-quote alternator — fresh context.
        self.smart_quote.reset();
    }

    /// Snapshot the WUBI dictionary's L0 layer (process-global; pins +
    /// pending pick counters + layer prefs). Backward-compat surface from
    /// the v0.1 wubi-only era. For dual-engine persistence, use
    /// `export_l0_json(engine_kind)`.
    pub fn export_l0(&self) -> L0Snapshot {
        wubi::export_l0()
    }

    /// Restore a previously-exported wubi L0 snapshot. Returns count of
    /// accepted pins.
    pub fn import_l0(&self, snap: L0Snapshot) -> usize {
        wubi::import_l0(snap)
    }

    /// JSON export per-engine for App-Group persistence (item 45).
    /// `engine_kind`: 0 = Wubi, 1 = Pinyin.
    /// Returns `None` for unrecognized engine_kind.
    pub fn export_l0_json(&self, engine_kind: u8) -> Option<String> {
        match Source::from_u8(engine_kind)? {
            Source::Wubi => Some(l0_json::wubi_to_json(&wubi::export_l0())),
            Source::Pinyin => self
                .composite
                .pinyin_export_l0()
                .map(|snap| l0_json::pinyin_to_json(&snap)),
        }
    }

    /// Restore L0 from JSON for the given engine. Returns count of accepted
    /// pins, or 0 on parse error / unrecognized engine.
    pub fn import_l0_json(&self, engine_kind: u8, json: &str) -> usize {
        let Some(kind) = Source::from_u8(engine_kind) else {
            return 0;
        };
        match kind {
            Source::Wubi => {
                if let Some(snap) = l0_json::wubi_from_json(json) {
                    wubi::import_l0(snap)
                } else {
                    0
                }
            }
            Source::Pinyin => {
                if let Some(snap) = l0_json::pinyin_from_json(json) {
                    self.composite.pinyin_import_l0(snap)
                } else {
                    0
                }
            }
        }
    }

    fn append_pending(&mut self, text: String) {
        match &mut self.pending_commit {
            Some(p) => p.push_str(&text),
            None => self.pending_commit = Some(text),
        }
    }

    fn refresh_caches(&mut self) {
        let cands: Vec<Candidate> = self.composite.candidates().to_vec();
        self.cand_cache.clear();
        self.source_cache.clear();
        for c in cands {
            self.cand_cache.push(c.word);
            self.source_cache.push(c.source);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s() -> Session {
        Session::new()
    }

    #[test]
    fn typing_g_with_unique_policy_auto_commits_yi() {
        let mut sess = s();
        sess.set_auto_commit_policy(AutoCommitPolicy::OnUniqueMatch);
        assert!(sess.handle_key(b'g' as u32, 0));
        assert_eq!(sess.take_pending_commit().as_deref(), Some("一"));
        assert!(sess.preedit().is_empty());
    }

    #[test]
    fn typing_jeg_then_space_commits_first_candidate() {
        let mut sess = s();
        sess.set_auto_commit_policy(AutoCommitPolicy::Never);
        sess.handle_key(b'j' as u32, 0);
        sess.handle_key(b'e' as u32, 0);
        sess.handle_key(b'g' as u32, 0);
        assert!(sess.candidate_count() >= 2);
        let first = sess.candidates()[0].clone();
        assert!(sess.handle_key(CP_SPACE, 0));
        assert_eq!(sess.take_pending_commit().as_deref(), Some(first.as_str()));
        assert!(sess.preedit().is_empty());
    }

    #[test]
    fn typing_jeg_then_digit_2_commits_second_candidate() {
        let mut sess = s();
        sess.set_auto_commit_policy(AutoCommitPolicy::Never);
        sess.handle_key(b'j' as u32, 0);
        sess.handle_key(b'e' as u32, 0);
        sess.handle_key(b'g' as u32, 0);
        assert!(sess.candidate_count() >= 2);
        let second = sess.candidates()[1].clone();
        assert!(sess.handle_key(b'2' as u32, 0));
        assert_eq!(sess.take_pending_commit().as_deref(), Some(second.as_str()));
    }

    #[test]
    fn digit_when_not_composing_passes_through() {
        let mut sess = s();
        assert!(!sess.handle_key(b'5' as u32, 0));
    }

    #[test]
    fn cmd_combo_passes_through() {
        let mut sess = s();
        assert!(!sess.handle_key(b'c' as u32, 1 << 3));
        assert!(!sess.handle_key(b'a' as u32, 1 << 1));
    }

    #[test]
    fn space_when_not_composing_passes_through() {
        let mut sess = s();
        assert!(!sess.handle_key(CP_SPACE, 0));
    }

    #[test]
    fn backspace_pops_then_passes_through_when_empty() {
        let mut sess = s();
        sess.set_auto_commit_policy(AutoCommitPolicy::Never);
        sess.handle_key(b'g' as u32, 0);
        sess.handle_key(b'g' as u32, 0);
        assert!(sess.handle_key(CP_BACKSPACE, 0));
        assert_eq!(sess.preedit(), "g");
        assert!(sess.handle_key(CP_BACKSPACE, 0));
        assert!(sess.preedit().is_empty());
        assert!(!sess.handle_key(CP_BACKSPACE, 0));
    }

    #[test]
    fn escape_clears_composition() {
        let mut sess = s();
        sess.set_auto_commit_policy(AutoCommitPolicy::Never);
        sess.handle_key(b'd' as u32, 0);
        sess.handle_key(b'd' as u32, 0);
        assert!(sess.handle_key(CP_ESCAPE, 0));
        assert!(sess.preedit().is_empty());
        assert_eq!(sess.candidate_count(), 0);
    }

    #[test]
    fn punctuation_mid_composition_commits_top_and_passes_through() {
        let mut sess = s();
        sess.set_auto_commit_policy(AutoCommitPolicy::Never);
        sess.handle_key(b'g' as u32, 0);
        let consumed = sess.handle_key(b',' as u32, 0);
        assert!(!consumed);
        assert_eq!(sess.take_pending_commit().as_deref(), Some("一"));
        assert!(!sess.composite.is_composing());
    }

    #[test]
    fn export_l0_returns_snapshot_with_layer_prefs() {
        let sess = s();
        let snap = sess.export_l0();
        assert_eq!(snap.layer_prefs.len(), 6);
        let restored = sess.import_l0(snap.clone());
        assert_eq!(restored, snap.pins.len());
    }

    #[test]
    fn typing_ipbf_with_default_auto_commits_xue() {
        let mut sess = s();
        for cp in b"ipbf" {
            sess.handle_key(*cp as u32, 0);
        }
        assert_eq!(sess.take_pending_commit().as_deref(), Some("学"));
    }

    // ------------------------------------------------------------------
    // Phase 4 dual-engine tests (items 44 + 45 + 47 manual probe)
    // ------------------------------------------------------------------

    #[test]
    fn item_47_mixed_wubi_input_gives_wubi_candidate() {
        // Manual probe per ROADMAP item 47: `khlg` → 中国 (Source::Wubi).
        // (The original ROADMAP wording said `wgkf → 国` — wrong wubi code;
        // 国 is `lgyi`, 中国 phrase is `khlg`. khlg is the canonical
        // multi-candidate test code in the wubi crate's own tests.)
        let mut sess = s();
        sess.set_auto_commit_policy(AutoCommitPolicy::Never);
        for cp in b"khlg" {
            sess.handle_key(*cp as u32, 0);
        }
        let cands = sess.candidates();
        assert!(!cands.is_empty(), "expected khlg candidates");
        assert_eq!(sess.candidate_source(0), Some(0)); // 0 = Source::Wubi
        // 中国 dominates khlg via real corpus weights (data-v2).
        assert_eq!(cands.first().map(String::as_str), Some("中国"));
    }

    #[test]
    fn item_47_mixed_pinyin_input_gives_pinyin_candidate() {
        // Manual probe per ROADMAP item 47: `zhongguo` → 中国 (Source::Pinyin).
        let mut sess = s();
        sess.set_auto_commit_policy(AutoCommitPolicy::Never);
        for cp in b"zhongguo" {
            sess.handle_key(*cp as u32, 0);
        }
        let cands = sess.candidates();
        assert_eq!(cands.first().map(String::as_str), Some("中国"));
        assert_eq!(sess.candidate_source(0), Some(1)); // 1 = Source::Pinyin
    }

    #[test]
    fn item_44_mode_switch_round_trip() {
        let mut sess = s();
        assert_eq!(sess.mode(), Mode::Mixed);
        sess.set_mode(Mode::WubiOnly);
        assert_eq!(sess.mode(), Mode::WubiOnly);
        sess.set_mode(Mode::PinyinOnly);
        assert_eq!(sess.mode(), Mode::PinyinOnly);
    }

    #[test]
    fn item_44_pinyin_only_mode_skips_wubi() {
        let mut sess = s();
        sess.set_mode(Mode::PinyinOnly);
        sess.set_auto_commit_policy(AutoCommitPolicy::Never);
        for cp in b"women" {
            sess.handle_key(*cp as u32, 0);
        }
        let cands = sess.candidates();
        assert_eq!(cands.first().map(String::as_str), Some("我们"));
        assert!(
            (0..cands.len()).all(|i| sess.candidate_source(i) == Some(1)),
            "expected all pinyin-sourced in PinyinOnly mode"
        );
    }

    #[test]
    fn item_45_wubi_l0_json_round_trip() {
        let sess = s();
        let json = sess.export_l0_json(0).expect("wubi export");
        assert!(json.contains("\"engine\":\"wubi\""));
        let n = sess.import_l0_json(0, &json);
        // n = pins.len() of the exported snapshot.
        let snap = sess.export_l0();
        assert_eq!(n, snap.pins.len());
    }

    #[test]
    fn item_45_pinyin_l0_json_round_trip() {
        let sess = s();
        let json = sess.export_l0_json(1).expect("pinyin export");
        assert!(json.contains("\"engine\":\"pinyin\""));
        let n = sess.import_l0_json(1, &json);
        // Fresh session: 0 pins.
        assert_eq!(n, 0);
    }

    #[test]
    fn item_45_unknown_engine_kind_returns_none_and_zero() {
        let sess = s();
        assert!(sess.export_l0_json(99).is_none());
        assert_eq!(sess.import_l0_json(99, "{}"), 0);
    }

    #[test]
    fn item_45_malformed_json_returns_zero() {
        let sess = s();
        assert_eq!(sess.import_l0_json(0, "not json"), 0);
        assert_eq!(sess.import_l0_json(1, "{also not"), 0);
    }
}
