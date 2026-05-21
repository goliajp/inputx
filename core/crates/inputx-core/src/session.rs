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
const CP_TAB: u32 = 0x09;

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
    /// (which is `WubiOnly`/`PinyinOnly`/`Mixed` *within* Cjk). In EN
    /// mode `handle_key` returns false unconditionally so the host
    /// receives ASCII directly — no IME-side preedit / buffering.
    input_mode: InputMode,
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

    /// Current engine mode (Mixed / WubiOnly / PinyinOnly / JapaneseOnly).
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

    /// JP plugin "enhancement" toggle. Independent of `mode`:
    /// - In Mixed / WubiOnly / PinyinOnly: when on, JP candidates are
    ///   appended after the Chinese candidates.
    /// - In JapaneseOnly: this flag is implicitly true and the toggle
    ///   here is a no-op (mode forces JP to run).
    pub fn japanese_enabled(&self) -> bool {
        self.composite.japanese_enabled()
    }

    pub fn set_japanese_enabled(&mut self, on: bool) {
        self.composite.set_japanese_enabled(on);
        self.refresh_caches();
    }

    /// Current top-level input mode (Cjk / En).
    pub fn input_mode(&self) -> InputMode {
        self.input_mode
    }

    /// Switch top-level input mode. On **Cjk → En** any in-flight CJK
    /// preedit is committed as **raw ASCII** (the wubi/pinyin letters the
    /// user typed) — user toggling to EN with codes still composing is
    /// signaling "this wasn't supposed to be CJK, ship it as English".
    /// Same semantic as pressing return in CJK with a non-empty preedit.
    /// **En → Cjk** has nothing to drain (EN mode doesn't buffer — see
    /// `handle_key`). No-op when target equals current mode.
    pub fn set_input_mode(&mut self, m: InputMode) {
        if m == self.input_mode {
            return;
        }
        if self.input_mode == InputMode::Cjk && self.composite.is_composing() {
            let raw = self.composite.preedit().to_string();
            self.composite.escape();
            self.refresh_caches();
            if !raw.is_empty() {
                self.append_pending(raw);
            }
        }
        self.input_mode = m;
    }

    /// Process a keystroke. Returns `true` if the IME consumed it. In
    /// `InputMode::En` always returns `false` — the IME steps aside and
    /// the host receives the raw ASCII keystroke. Host adapters may
    /// short-circuit this call entirely when in EN mode for efficiency;
    /// the returned `false` is the canonical answer either way.
    pub fn handle_key(&mut self, codepoint: u32, modifiers: u32) -> bool {
        match self.input_mode {
            InputMode::Cjk => self.handle_key_cjk(codepoint, modifiers),
            InputMode::En => false,
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
            CP_RETURN_CR | CP_RETURN_LF => {
                // Return while composing = "I didn't want this to be CJK".
                // Commit the raw ASCII codes the user typed, clear preedit,
                // swallow the event (no \n to host — the user only wanted
                // to escape composing). Return when not composing falls
                // through to passthrough so plain Enter still produces \n.
                if !self.composite.is_composing() {
                    return false;
                }
                let raw = self.composite.preedit().to_string();
                self.composite.escape();
                self.refresh_caches();
                if !raw.is_empty() {
                    self.append_pending(raw);
                }
                true
            }
            CP_TAB => {
                // Tab is reserved for future candidate page navigation.
                // While composing: swallow silently — no state change, no
                // commit, no \t to host. The default branch would otherwise
                // force-commit the top candidate and pass \t through, which
                // is the wrong UX (user expects tab to be a no-op or page
                // candidates, never a "commit + tab" combo). Not composing:
                // passthrough so plain tab still inserts a tab character.
                self.composite.is_composing()
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
        match self.input_mode {
            InputMode::Cjk => self.composite.preedit(),
            // EN mode has no IME-side preedit — host owns the text.
            InputMode::En => "",
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
        // Return to default CJK mode.
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
    /// `engine_kind`: 0 = Wubi, 1 = Pinyin, 2 = Japanese.
    /// Returns `None` for unrecognized engine_kind, OR for Japanese
    /// (the JP plugin doesn't ship per-user L0 in v0.1 — no pin counters,
    /// no freq learning, so there's nothing to persist).
    pub fn export_l0_json(&self, engine_kind: u8) -> Option<String> {
        match Source::from_u8(engine_kind)? {
            Source::Wubi => Some(l0_json::wubi_to_json(&wubi::export_l0())),
            Source::Pinyin => self
                .composite
                .pinyin_export_l0()
                .map(|snap| l0_json::pinyin_to_json(&snap)),
            Source::Japanese => None,
        }
    }

    /// Restore L0 from JSON for the given engine. Returns count of accepted
    /// pins, or 0 on parse error / unrecognized engine. Japanese always
    /// returns 0 — no L0 to restore (see `export_l0_json` doc).
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
            Source::Japanese => 0,
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
    fn wcng_top_candidate_is_phrase_gongsi_not_rare_single_char() {
        // Regression for the dual of gmww — at full code, when the
        // single-char's freq is LOWER than the phrase's, the phrase
        // wins. Pre-fix, the absolute-`freq > 0` rule promoted ANY
        // single-char with corpus presence (鹟 freq 5961) above the
        // phrase, even when the phrase had far higher actual frequency
        // (公司 freq 42817). The corrected rule compares freqs.
        let mut sess = s();
        sess.set_auto_commit_policy(AutoCommitPolicy::Never);
        for cp in b"wcng" {
            sess.handle_key(*cp as u32, 0);
        }
        let cands = sess.candidates();
        assert!(!cands.is_empty());
        assert_eq!(
            cands.first().map(String::as_str),
            Some("公司"),
            "wcng top candidate should be 公司, got {:?}",
            cands.first()
        );
    }

    #[test]
    fn gmww_top_candidate_is_single_char_liang_not_phrase() {
        // Regression: at a fully-typed 4-letter wubi code, the canonical
        // single-char answer must rank above any phrase sharing the code.
        // Pre-fix, 两 (Auto layer, base ~100k) lost to 两败俱伤 (Phrase
        // layer, base ~400k) at gmww. The full-code single-char-wins
        // rule in `inputx_wubi::dict::lookup_into` corrects this.
        let mut sess = s();
        sess.set_auto_commit_policy(AutoCommitPolicy::Never);
        for cp in b"gmww" {
            sess.handle_key(*cp as u32, 0);
        }
        let cands = sess.candidates();
        assert!(!cands.is_empty(), "expected gmww candidates");
        assert_eq!(
            cands.first().map(String::as_str),
            Some("两"),
            "gmww top candidate should be 两 (single char), got {:?}",
            cands.first()
        );
        // 两败俱伤 should still appear, just not at #1.
        assert!(
            cands.iter().any(|w| w == "两败俱伤"),
            "phrase 两败俱伤 should still appear in gmww candidates"
        );
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
    // v1.1.0 InputMode (Cjk / En) — EN mode is pure passthrough
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
    fn en_mode_consumes_no_keys() {
        // Every keystroke variety should pass straight through to the host:
        // letters, digits, punct, space, return, backspace, escape, ctrl/cmd.
        let mut sess = s();
        sess.set_input_mode(InputMode::En);
        for cp in b"hello world! 12,3.45" {
            assert!(!sess.handle_key(*cp as u32, 0), "letter/digit/punct should passthrough");
        }
        assert!(!sess.handle_key(CP_RETURN, 0));
        assert!(!sess.handle_key(CP_BACKSPACE, 0));
        assert!(!sess.handle_key(CP_ESCAPE, 0));
        assert!(!sess.handle_key(b'c' as u32, MOD_CMD));
        assert!(!sess.handle_key(b'a' as u32, MOD_CTRL));
    }

    #[test]
    fn en_mode_has_no_preedit_or_candidates() {
        // Engine never runs in EN — preedit empty, no candidates, no commits.
        let mut sess = s();
        sess.set_input_mode(InputMode::En);
        for cp in b"khlg" {
            sess.handle_key(*cp as u32, 0);
        }
        assert!(sess.preedit().is_empty());
        assert_eq!(sess.candidate_count(), 0);
        assert!(sess.take_pending_commit().is_none());
    }

    #[test]
    fn cjk_to_en_with_preedit_commits_raw_ascii() {
        // User types "jeg" in CJK, then toggles to EN. The preedit
        // letters ship as English (user signaled "not CJK after all").
        let mut sess = s();
        sess.set_auto_commit_policy(AutoCommitPolicy::Never);
        sess.handle_key(b'j' as u32, 0);
        sess.handle_key(b'e' as u32, 0);
        sess.handle_key(b'g' as u32, 0);
        assert_eq!(sess.preedit(), "jeg");
        sess.set_input_mode(InputMode::En);
        assert_eq!(sess.take_pending_commit().as_deref(), Some("jeg"));
        assert!(sess.preedit().is_empty());
        assert_eq!(sess.candidate_count(), 0);
        assert_eq!(sess.input_mode(), InputMode::En);
    }

    #[test]
    fn cjk_to_en_without_preedit_just_switches() {
        // No in-flight composing → toggle is a pure state flip.
        let mut sess = s();
        sess.set_input_mode(InputMode::En);
        assert!(sess.take_pending_commit().is_none());
        assert_eq!(sess.input_mode(), InputMode::En);
    }

    #[test]
    fn cjk_return_with_preedit_commits_raw_ascii() {
        // Return in CJK while composing ships the raw codes as ASCII
        // and swallows the event (no \n to host).
        let mut sess = s();
        sess.set_auto_commit_policy(AutoCommitPolicy::Never);
        sess.handle_key(b'h' as u32, 0);
        sess.handle_key(b'u' as u32, 0);
        sess.handle_key(b'i' as u32, 0);
        sess.handle_key(b'l' as u32, 0);
        assert_eq!(sess.preedit(), "huil");
        assert!(sess.handle_key(CP_RETURN, 0));
        assert_eq!(sess.take_pending_commit().as_deref(), Some("huil"));
        assert!(sess.preedit().is_empty());
        assert_eq!(sess.candidate_count(), 0);
    }

    #[test]
    fn cjk_return_without_preedit_passes_through() {
        // Plain return with no composing → engine doesn't consume it
        // so the host receives the \n normally.
        let mut sess = s();
        assert!(!sess.handle_key(CP_RETURN, 0));
        assert!(sess.take_pending_commit().is_none());
    }

    #[test]
    fn cjk_tab_with_preedit_is_swallowed() {
        // Tab while composing must not leak to the host (no \t in the
        // text field) and must not disturb preedit. Reserved for future
        // candidate page navigation.
        let mut sess = s();
        sess.set_auto_commit_policy(AutoCommitPolicy::Never);
        for cp in b"jeg" {
            sess.handle_key(*cp as u32, 0);
        }
        let preedit_before = sess.preedit().to_string();
        let cands_before = sess.candidate_count();
        assert!(sess.handle_key(0x09, 0));
        assert!(sess.take_pending_commit().is_none());
        assert_eq!(sess.preedit(), preedit_before);
        assert_eq!(sess.candidate_count(), cands_before);
    }

    #[test]
    fn cjk_tab_without_preedit_passes_through() {
        // Plain tab with no composing → engine doesn't consume so the
        // host receives \t as a tab character.
        let mut sess = s();
        assert!(!sess.handle_key(0x09, 0));
        assert!(sess.take_pending_commit().is_none());
    }

    #[test]
    fn en_to_cjk_is_pure_state_flip_no_commit() {
        // EN→Cjk has nothing to drain (EN doesn't buffer). Just flips state.
        let mut sess = s();
        sess.set_input_mode(InputMode::En);
        for cp in b"hel" {
            sess.handle_key(*cp as u32, 0);
        }
        sess.set_input_mode(InputMode::Cjk);
        assert!(sess.take_pending_commit().is_none());
        assert!(sess.preedit().is_empty());
        assert_eq!(sess.input_mode(), InputMode::Cjk);
    }

    #[test]
    fn clear_resets_input_mode() {
        let mut sess = s();
        sess.set_input_mode(InputMode::En);
        sess.clear();
        assert_eq!(sess.input_mode(), InputMode::Cjk);
        assert!(sess.preedit().is_empty());
        assert!(sess.take_pending_commit().is_none());
    }

    // ------------------------------------------------------------------
    // Proptest: arbitrary op sequences keep session invariants
    // ------------------------------------------------------------------
    //
    //   - any op sequence followed by clear() leaves a default session
    //   - any op sequence followed by 2 full mode-toggle cycles ends in
    //     a clean idle CJK session (no preedit, no cand, no commit)
    //
    // The second one is the mode-switch-doesn't-leak invariant
    // ([[proptest-engine-invariants]] caught the analogous bug at the
    // composite/engine layer — this is the Session-layer counterpart).
    // EN mode is pure passthrough (`handle_key` returns false), so EN
    // keystrokes can't leak buffers — but the Cjk→En transition still
    // has to escape an in-flight composing buffer.

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
                sess.handle_key(CP_RETURN, 0);
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
        // INV-A: EN mode is dormant — no candidates, no preedit.
        if sess.input_mode() == InputMode::En {
            assert_eq!(
                sess.candidate_count(),
                0,
                "INV-A violated after {after}: EN has {} candidates",
                sess.candidate_count()
            );
            assert!(
                sess.preedit().is_empty(),
                "INV-A violated after {after}: EN preedit is non-empty: {:?}",
                sess.preedit()
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
            // Two full mode-cycles. After this, the CJK composing buffer
            // (escape()d on each Cjk→En) must be empty. EN→Cjk has nothing
            // to drain since EN doesn't buffer.
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
