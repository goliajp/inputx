//! Session — top-level state visible to the host. Wraps a `CompositeEngine`
//! (wubi + pinyin dual-engine) per Phase 4 of the iOS commercial-grade
//! roadmap. The public API + FFI surface stays compatible with the v0.1
//! wubi-only shape; new dual-engine knobs (`mode`, `candidate_source`,
//! `export_l0_json`) are additive.

use crate::composite::{Candidate, CompositeEngine, Mode, Source, l0_json};
use crate::input_mode::InputMode;
use crate::locale::punct::SmartQuoteState;
use crate::wubi::{self, AutoCommitPolicy, L0Snapshot};

const MOD_CTRL: u32 = 1 << 1;
const MOD_CMD: u32 = 1 << 3;

const CP_BACKSPACE: u32 = 0x08;
const CP_DEL_FORWARD: u32 = 0x7F;
const CP_ESCAPE: u32 = 0x1B;
const CP_SPACE: u32 = b' ' as u32;
const CP_RETURN_CR: u32 = 0x0D;
const CP_RETURN_LF: u32 = 0x0A;

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
    /// Top-level input mode (CJK vs EN). Orthogonal to engine `Mode`
    /// (which is `WubiOnly`/`PinyinOnly`/`Mixed` *within* Cjk).
    input_mode: InputMode,
    /// ASCII preedit buffer used only in `InputMode::En`. Letters /
    /// digits / printable punct push; `return` commits (no \n sent);
    /// `space` commits with trailing " "; backspace pops; escape clears.
    en_preedit: String,
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
            input_mode: InputMode::Cjk,
            en_preedit: String::new(),
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

    /// Current top-level input mode (Cjk / En).
    pub fn input_mode(&self) -> InputMode {
        self.input_mode
    }

    /// Switch top-level input mode. Asymmetric transition rules:
    /// - **Cjk → En**: in-flight CJK composing is *dropped* (escape, not
    ///   committed) — user toggling to EN implies the wubi/pinyin code
    ///   they were typing is no longer wanted.
    /// - **En → Cjk**: in-flight `en_preedit` is *committed* — the
    ///   typed English so far is kept (it's already valid output).
    /// No-op when target equals current mode.
    pub fn set_input_mode(&mut self, m: InputMode) {
        if m == self.input_mode {
            return;
        }
        match self.input_mode {
            InputMode::Cjk => {
                self.composite.escape();
                self.refresh_caches();
            }
            InputMode::En => {
                if !self.en_preedit.is_empty() {
                    let text = std::mem::take(&mut self.en_preedit);
                    self.append_pending(text);
                }
            }
        }
        self.input_mode = m;
    }

    /// Process a keystroke. Returns `true` if the IME consumed it.
    pub fn handle_key(&mut self, codepoint: u32, modifiers: u32) -> bool {
        match self.input_mode {
            InputMode::Cjk => self.handle_key_cjk(codepoint, modifiers),
            InputMode::En => self.handle_key_en(codepoint, modifiers),
        }
    }

    fn handle_key_cjk(&mut self, codepoint: u32, modifiers: u32) -> bool {
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

    /// EN-mode keystroke handler. Pure ASCII preedit pipeline; no engine
    /// lookup, no candidates. See `set_input_mode` doc for transition
    /// rules at mode boundary.
    fn handle_key_en(&mut self, codepoint: u32, modifiers: u32) -> bool {
        if modifiers & (MOD_CTRL | MOD_CMD) != 0 {
            return false;
        }

        match codepoint {
            CP_RETURN_CR | CP_RETURN_LF => {
                if self.en_preedit.is_empty() {
                    return false;
                }
                let text = std::mem::take(&mut self.en_preedit);
                self.append_pending(text);
                true
            }
            CP_SPACE => {
                if self.en_preedit.is_empty() {
                    return false;
                }
                let mut text = std::mem::take(&mut self.en_preedit);
                text.push(' ');
                self.append_pending(text);
                true
            }
            CP_BACKSPACE | CP_DEL_FORWARD => {
                if self.en_preedit.is_empty() {
                    return false;
                }
                self.en_preedit.pop();
                true
            }
            CP_ESCAPE => {
                if self.en_preedit.is_empty() {
                    return false;
                }
                self.en_preedit.clear();
                true
            }
            cp => {
                if let Some(c) = char::from_u32(cp)
                    && is_en_input_char(c)
                {
                    self.en_preedit.push(c);
                    return true;
                }
                // Unhandled key: if preedit non-empty, commit before
                // letting the host receive the keystroke (so the typed
                // English doesn't get stranded behind the new char).
                if !self.en_preedit.is_empty() {
                    let text = std::mem::take(&mut self.en_preedit);
                    self.append_pending(text);
                }
                false
            }
        }
    }

    pub fn preedit(&self) -> &str {
        match self.input_mode {
            InputMode::Cjk => self.composite.preedit(),
            InputMode::En => &self.en_preedit,
        }
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
        // Reset EN preedit + return to default CJK mode.
        self.en_preedit.clear();
        self.input_mode = InputMode::Cjk;
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

/// Predicate for EN-mode preedit acceptance: printable ASCII that isn't
/// whitespace or control. Matches letters, digits, and visible punctuation.
/// Space / return / backspace / escape are routed by codepoint in
/// `handle_key_en` and never reach this check.
fn is_en_input_char(c: char) -> bool {
    c.is_ascii() && !c.is_ascii_whitespace() && !c.is_ascii_control()
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

    // ------------------------------------------------------------------
    // v1.1.0 InputMode (Cjk / En) + EN preedit pipeline
    // ------------------------------------------------------------------

    const CP_RETURN: u32 = 0x0D;

    #[test]
    fn input_mode_default_is_cjk() {
        let sess = s();
        assert_eq!(sess.input_mode(), InputMode::Cjk);
    }

    #[test]
    fn set_input_mode_toggles() {
        let mut sess = s();
        sess.set_input_mode(InputMode::En);
        assert_eq!(sess.input_mode(), InputMode::En);
        sess.set_input_mode(InputMode::Cjk);
        assert_eq!(sess.input_mode(), InputMode::Cjk);
    }

    #[test]
    fn set_input_mode_same_is_noop() {
        let mut sess = s();
        sess.set_input_mode(InputMode::Cjk);
        assert_eq!(sess.input_mode(), InputMode::Cjk);
        assert!(sess.take_pending_commit().is_none());
    }

    #[test]
    fn en_mode_letter_accumulates_in_preedit() {
        let mut sess = s();
        sess.set_input_mode(InputMode::En);
        for cp in b"hello" {
            assert!(sess.handle_key(*cp as u32, 0));
        }
        assert_eq!(sess.preedit(), "hello");
        assert!(sess.take_pending_commit().is_none());
    }

    #[test]
    fn en_mode_digit_accumulates() {
        let mut sess = s();
        sess.set_input_mode(InputMode::En);
        for cp in b"abc123" {
            assert!(sess.handle_key(*cp as u32, 0));
        }
        assert_eq!(sess.preedit(), "abc123");
    }

    #[test]
    fn en_mode_punct_accumulates() {
        let mut sess = s();
        sess.set_input_mode(InputMode::En);
        for cp in b"a.b,c!" {
            assert!(sess.handle_key(*cp as u32, 0));
        }
        assert_eq!(sess.preedit(), "a.b,c!");
    }

    #[test]
    fn en_mode_return_commits_without_newline() {
        let mut sess = s();
        sess.set_input_mode(InputMode::En);
        for cp in b"hello" {
            sess.handle_key(*cp as u32, 0);
        }
        assert!(sess.handle_key(CP_RETURN, 0));
        assert_eq!(sess.take_pending_commit().as_deref(), Some("hello"));
        assert!(sess.preedit().is_empty());
    }

    #[test]
    fn en_mode_return_when_empty_passthrough() {
        let mut sess = s();
        sess.set_input_mode(InputMode::En);
        assert!(!sess.handle_key(CP_RETURN, 0));
    }

    #[test]
    fn en_mode_space_commits_with_trailing_space() {
        let mut sess = s();
        sess.set_input_mode(InputMode::En);
        for cp in b"world" {
            sess.handle_key(*cp as u32, 0);
        }
        assert!(sess.handle_key(CP_SPACE, 0));
        assert_eq!(sess.take_pending_commit().as_deref(), Some("world "));
        assert!(sess.preedit().is_empty());
    }

    #[test]
    fn en_mode_space_when_empty_passthrough() {
        let mut sess = s();
        sess.set_input_mode(InputMode::En);
        assert!(!sess.handle_key(CP_SPACE, 0));
    }

    #[test]
    fn en_mode_backspace_pops() {
        let mut sess = s();
        sess.set_input_mode(InputMode::En);
        for cp in b"abc" {
            sess.handle_key(*cp as u32, 0);
        }
        assert!(sess.handle_key(CP_BACKSPACE, 0));
        assert_eq!(sess.preedit(), "ab");
        assert!(sess.handle_key(CP_BACKSPACE, 0));
        assert!(sess.handle_key(CP_BACKSPACE, 0));
        assert!(sess.preedit().is_empty());
        // empty → passthrough
        assert!(!sess.handle_key(CP_BACKSPACE, 0));
    }

    #[test]
    fn en_mode_escape_clears() {
        let mut sess = s();
        sess.set_input_mode(InputMode::En);
        for cp in b"abc" {
            sess.handle_key(*cp as u32, 0);
        }
        assert!(sess.handle_key(CP_ESCAPE, 0));
        assert!(sess.preedit().is_empty());
        // empty → passthrough
        assert!(!sess.handle_key(CP_ESCAPE, 0));
    }

    #[test]
    fn en_mode_ctrl_cmd_passthrough() {
        let mut sess = s();
        sess.set_input_mode(InputMode::En);
        assert!(!sess.handle_key(b'c' as u32, MOD_CMD));
        assert!(!sess.handle_key(b'a' as u32, MOD_CTRL));
    }

    #[test]
    fn cjk_to_en_drops_composing_does_not_commit() {
        // User types "jeg" in CJK (mid-composition), then toggles to EN.
        // Expectation: jeg is dropped (escape), not committed.
        let mut sess = s();
        sess.set_auto_commit_policy(AutoCommitPolicy::Never);
        sess.handle_key(b'j' as u32, 0);
        sess.handle_key(b'e' as u32, 0);
        sess.handle_key(b'g' as u32, 0);
        assert!(!sess.preedit().is_empty());
        sess.set_input_mode(InputMode::En);
        assert!(sess.take_pending_commit().is_none());
        assert!(sess.preedit().is_empty());
        assert_eq!(sess.candidate_count(), 0);
    }

    #[test]
    fn en_to_cjk_commits_en_preedit() {
        // User types "hel" in EN, then toggles to CJK.
        // Expectation: "hel" is committed (not lost).
        let mut sess = s();
        sess.set_input_mode(InputMode::En);
        for cp in b"hel" {
            sess.handle_key(*cp as u32, 0);
        }
        sess.set_input_mode(InputMode::Cjk);
        assert_eq!(sess.take_pending_commit().as_deref(), Some("hel"));
        assert!(sess.preedit().is_empty());
        assert_eq!(sess.input_mode(), InputMode::Cjk);
    }

    #[test]
    fn clear_resets_input_mode_and_en_preedit() {
        let mut sess = s();
        sess.set_input_mode(InputMode::En);
        for cp in b"abc" {
            sess.handle_key(*cp as u32, 0);
        }
        assert_eq!(sess.preedit(), "abc");
        sess.clear();
        assert_eq!(sess.input_mode(), InputMode::Cjk);
        assert!(sess.preedit().is_empty());
        assert!(sess.take_pending_commit().is_none());
    }

    #[test]
    fn en_mode_cjk_engine_dormant_no_candidates() {
        // Letters that would produce wubi candidates in CJK should not in EN.
        let mut sess = s();
        sess.set_input_mode(InputMode::En);
        for cp in b"khlg" {
            sess.handle_key(*cp as u32, 0);
        }
        assert_eq!(sess.candidate_count(), 0);
        assert_eq!(sess.preedit(), "khlg");
    }

    // ------------------------------------------------------------------
    // Proptest: InputMode round-trips never leak buffers
    // ------------------------------------------------------------------
    //
    // Two properties:
    //   - any op sequence followed by clear() leaves a default session
    //   - any op sequence followed by 2 full mode-toggle cycles drains
    //     both CJK and EN buffers (no preedit, no cand, idle in Cjk)
    //
    // The second one is the actual mode-switch-doesn't-leak invariant
    // ([[proptest-engine-invariants]] caught the analogous bug at the
    // composite/engine layer — this is the Session-layer counterpart).

    use proptest::prelude::*;

    #[derive(Clone, Debug)]
    enum SessionOp {
        Letter(u8),
        Digit(u8),
        Space,
        Return,
        Backspace,
        Escape,
        SetCjk,
        SetEn,
    }

    fn op_strategy() -> impl Strategy<Value = SessionOp> {
        prop_oneof![
            10 => (b'a'..=b'z').prop_map(SessionOp::Letter),
            2 => (b'0'..=b'9').prop_map(SessionOp::Digit),
            2 => Just(SessionOp::Space),
            2 => Just(SessionOp::Return),
            1 => Just(SessionOp::Backspace),
            1 => Just(SessionOp::Escape),
            2 => Just(SessionOp::SetCjk),
            2 => Just(SessionOp::SetEn),
        ]
    }

    fn apply(sess: &mut Session, op: &SessionOp) {
        match op {
            SessionOp::Letter(b) | SessionOp::Digit(b) => {
                sess.handle_key(*b as u32, 0);
            }
            SessionOp::Space => {
                sess.handle_key(CP_SPACE, 0);
            }
            SessionOp::Return => {
                sess.handle_key(CP_RETURN_CR, 0);
            }
            SessionOp::Backspace => {
                sess.handle_key(CP_BACKSPACE, 0);
            }
            SessionOp::Escape => {
                sess.handle_key(CP_ESCAPE, 0);
            }
            SessionOp::SetCjk => sess.set_input_mode(InputMode::Cjk),
            SessionOp::SetEn => sess.set_input_mode(InputMode::En),
        }
        let _ = sess.take_pending_commit();
    }

    fn check_invariants(sess: &Session, after: &str) {
        // INV-A: EN mode has no candidates (engine dormant).
        if sess.input_mode() == InputMode::En {
            assert_eq!(
                sess.candidate_count(),
                0,
                "INV-A violated after {after}: EN has {} candidates",
                sess.candidate_count()
            );
        }
        // INV-B: CJK preedit is ASCII lowercase wubi codes only.
        if sess.input_mode() == InputMode::Cjk {
            for b in sess.preedit().bytes() {
                assert!(
                    b.is_ascii_lowercase(),
                    "INV-B violated after {after}: CJK preedit byte {b:#x}"
                );
            }
        }
        // INV-C: EN preedit is printable ASCII (no control / whitespace).
        if sess.input_mode() == InputMode::En {
            for b in sess.preedit().bytes() {
                assert!(
                    b.is_ascii() && !(b as char).is_ascii_whitespace()
                        && !(b as char).is_ascii_control(),
                    "INV-C violated after {after}: EN preedit byte {b:#x}"
                );
            }
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 128,
            .. ProptestConfig::default()
        })]

        #[test]
        fn fuzz_session_invariants_hold_then_clear_resets(
            ops in proptest::collection::vec(op_strategy(), 1..64)
        ) {
            let mut sess = Session::new();
            for (i, op) in ops.iter().enumerate() {
                apply(&mut sess, op);
                check_invariants(&sess, &format!("op#{i}={:?}", op));
            }
            sess.clear();
            prop_assert_eq!(sess.input_mode(), InputMode::Cjk);
            prop_assert!(sess.preedit().is_empty(),
                "post-clear preedit leaked: {:?}", sess.preedit());
            prop_assert!(sess.take_pending_commit().is_none());
            prop_assert_eq!(sess.candidate_count(), 0);
        }

        #[test]
        fn fuzz_input_mode_double_toggle_drains_buffers(
            ops in proptest::collection::vec(op_strategy(), 0..32)
        ) {
            let mut sess = Session::new();
            for op in &ops {
                apply(&mut sess, op);
            }
            // Two full mode-cycles. After this, both CJK composing buffer
            // (escape()d on each Cjk→En) and en_preedit (drained on each
            // En→Cjk) must be empty.
            sess.set_input_mode(InputMode::En);
            sess.set_input_mode(InputMode::Cjk);
            sess.set_input_mode(InputMode::En);
            sess.set_input_mode(InputMode::Cjk);
            let _ = sess.take_pending_commit();
            prop_assert_eq!(sess.input_mode(), InputMode::Cjk);
            prop_assert!(sess.preedit().is_empty(),
                "preedit leaked after double-toggle: {:?}", sess.preedit());
            prop_assert_eq!(sess.candidate_count(), 0);
        }
    }
}
