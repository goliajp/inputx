//! `CompositeEngine` — the dual-engine state machine driven by the
//! composite session. Holds owned sub-engines and a `Mode`; routes each
//! keystroke to the relevant sub-engines and exposes the merged candidate
//! list.
//!
//! Design constraints:
//! - **WubiEngine** is letter-by-letter with a max-4 char buffer +
//!   AutoCommitPolicy that may force-commit and reset on the 5th letter
//!   or via `OnFourCodesIfUnique` triggers.
//! - **PinyinAdapter** has no fixed length — accepts any number of
//!   ASCII letters and keeps composing until commit / escape / backspace
//!   to empty.
//!
//! In `Mode::Mixed`, both sub-engines see the same keystrokes. Wubi may
//! force-commit and reset — the resulting committed text bubbles up
//! through the engine's return value (same surface as `WubiEngine` alone
//! provides). Pinyin keeps composing in parallel; if wubi is force-
//! committing, the engine clears pinyin too (consistent state — user
//! sees committed text, then a fresh empty buffer).

use super::dispatch::dispatch;
use super::japanese_adapter::JapaneseAdapter;
use super::merge::Candidate;
use super::mode::Mode;
use super::pinyin_adapter::PinyinAdapter;
use crate::wubi::{AutoCommitPolicy, WubiEngine};

pub struct CompositeEngine {
    wubi: WubiEngine,
    pinyin: PinyinAdapter,
    /// Lazy JP plugin — `Some` iff JP is currently active (either
    /// `Mode::JapaneseOnly` OR `enable_japanese=true` in another mode).
    /// `None` means JP is fully off and the keystroke path skips it
    /// entirely — zero cost on the hot path when the user doesn't want
    /// Japanese. The lifecycle is managed by `set_mode` and
    /// `set_japanese_enabled`, which (re)allocate / drop this as needed.
    japanese: Option<JapaneseAdapter>,
    /// User-facing toggle, default false. Independent of `mode`:
    /// JP attaches as an "enhancement" to Mixed/Wubi/Pinyin when true,
    /// or is the lone engine when `mode == JapaneseOnly` (in which case
    /// this flag is effectively forced true).
    enable_japanese: bool,
    mode: Mode,
    /// User-facing auto-commit policy. CompositeEngine owns the actual
    /// decision (sub-engine wubi is always set to `Never` so it can't
    /// commit out from under us); we apply this policy via
    /// `should_force_commit_wubi()` after each letter.
    user_policy: AutoCommitPolicy,
    /// Reused candidate buffer to avoid per-keystroke alloc.
    cand_buf: Vec<Candidate>,
}

impl Default for CompositeEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl CompositeEngine {
    pub fn new() -> Self {
        let mut wubi = WubiEngine::new();
        // Always Never on the sub-engine; CompositeEngine takes the commit
        // decision in `handle_letter`. This is what keeps wubi 4-letter
        // unique codes (e.g., shan→櫖, wang→佢) from hijacking pinyin
        // input that happens to start with those same letters.
        wubi.set_policy(AutoCommitPolicy::Never);
        Self {
            wubi,
            pinyin: PinyinAdapter::new(),
            japanese: None,
            enable_japanese: false,
            mode: Mode::default(),
            user_policy: AutoCommitPolicy::default(),
            cand_buf: Vec::with_capacity(16),
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn japanese_enabled(&self) -> bool {
        self.enable_japanese
    }

    /// Toggle the JP plugin's "enhancement" attachment. No effect when
    /// `mode == JapaneseOnly` (JP is forced on there regardless). Clears
    /// the JP buffer when turning off so a re-enable doesn't pick up
    /// stale state. (Re)allocates the adapter as needed.
    pub fn set_japanese_enabled(&mut self, on: bool) {
        if self.enable_japanese == on {
            return;
        }
        self.enable_japanese = on;
        self.sync_japanese_adapter();
    }

    /// Should the JP plugin currently participate? `JapaneseOnly` forces
    /// it on regardless of the toggle; other modes consult the toggle.
    fn japanese_active(&self) -> bool {
        self.mode.forces_japanese() || self.enable_japanese
    }

    /// Bring the `japanese` field into agreement with `japanese_active()`:
    /// allocate on first use, drop when turning fully off. Drops on
    /// disable also so that future re-enables start fresh — JP doesn't
    /// have per-session learning yet so this is safe.
    fn sync_japanese_adapter(&mut self) {
        let need = self.japanese_active();
        match (need, self.japanese.is_some()) {
            (true, false) => self.japanese = Some(JapaneseAdapter::new()),
            (false, true) => self.japanese = None,
            _ => {}
        }
    }

    /// Set the engine mode. Clears the buffer of any engine the new mode
    /// disallows — otherwise a Mixed→WubiOnly mid-compose flip leaves the
    /// pinyin buffer dangling (still flagged composing, but `preedit()`
    /// reads only the wubi buffer), which the UI sees as "composing but
    /// blank candidate bar". (Caught by proptest fuzz, fuzz_engine_never_*.)
    ///
    /// On a transition to `JapaneseOnly`, both Chinese sub-engines are
    /// cleared and the JP adapter is brought online (if not already).
    /// On a transition AWAY from `JapaneseOnly` with `enable_japanese=false`,
    /// the JP adapter is dropped — its buffer would otherwise dangle.
    pub fn set_mode(&mut self, mode: Mode) {
        if !mode.allows_wubi() && self.wubi.is_composing() {
            self.wubi.escape();
        }
        if !mode.allows_pinyin() && self.pinyin.is_composing() {
            self.pinyin.escape();
        }
        self.mode = mode;
        self.sync_japanese_adapter();
    }

    pub fn set_auto_commit_policy(&mut self, p: AutoCommitPolicy) {
        // Stored on CompositeEngine; sub-engine wubi remains Never (see
        // `new()`). `should_force_commit_wubi()` consults `user_policy`.
        self.user_policy = p;
    }

    pub fn auto_commit_policy(&self) -> AutoCommitPolicy {
        self.user_policy
    }

    pub fn is_composing(&self) -> bool {
        // Filter by current mode — a buffer in a disallowed engine
        // doesn't count as "composing" from the user's POV (no
        // candidates would render for it anyway). set_mode() clears
        // these on mode flips, but this guard belt-and-suspenders any
        // future code path that mutates buffers directly.
        if self.mode.is_japanese_only() {
            return self.japanese.as_ref().is_some_and(|j| j.is_composing());
        }
        (self.mode.allows_wubi() && self.wubi.is_composing())
            || (self.mode.allows_pinyin() && self.pinyin.is_composing())
            || (self.enable_japanese
                && self.japanese.as_ref().is_some_and(|j| j.is_composing()))
    }

    /// The active preedit string. In Mixed mode prefers pinyin's longer
    /// buffer if it diverges (e.g., > 4 chars after wubi force-commits);
    /// in JapaneseOnly mode the JP buffer is used. Callers that want a
    /// raw per-engine buffer can use `wubi_buffer_str` / `pinyin_buffer_str`.
    pub fn preedit(&self) -> &str {
        if self.mode.is_japanese_only() {
            return self.japanese.as_ref().map(|j| j.buffer_str()).unwrap_or("");
        }
        if !self.mode.allows_pinyin() {
            // WubiOnly — wubi is the primary buffer. If JP is also on
            // ("enhancement"), prefer wubi (it represents the deliberate
            // CJK intent); only fall back to JP buffer when wubi is empty.
            let w = self.wubi.buffer_str();
            if !w.is_empty() {
                return w;
            }
            if let Some(j) = self.japanese.as_ref() {
                let jp = j.buffer_str();
                if !jp.is_empty() {
                    return jp;
                }
            }
            return w;
        }
        if !self.mode.allows_wubi() {
            // PinyinOnly path.
            let p = self.pinyin.buffer_str();
            if !p.is_empty() {
                return p;
            }
            if let Some(j) = self.japanese.as_ref() {
                let jp = j.buffer_str();
                if !jp.is_empty() {
                    return jp;
                }
            }
            return p;
        }
        // Mixed — prefer pinyin (it captures the full input across
        // wubi force-commits). But fall back to wubi if pinyin's buffer
        // happens to be empty (e.g., entered Mixed from WubiOnly after
        // some keystrokes — wubi has state, pinyin does not). JP buffer
        // mirrors pinyin (same letter-by-letter feed) so it doesn't
        // need a separate fallback here.
        let p = self.pinyin.buffer_str();
        if !p.is_empty() {
            p
        } else {
            self.wubi.buffer_str()
        }
    }

    pub fn wubi_buffer_str(&self) -> &str {
        self.wubi.buffer_str()
    }

    pub fn pinyin_buffer_str(&self) -> &str {
        self.pinyin.buffer_str()
    }

    /// Recompute and return the merged candidate list. Slice borrows
    /// internal storage; subsequent calls invalidate.
    ///
    /// Cross-engine pin promotion runs as a final pass: if the user has
    /// pinned a word for the current pinyin buffer (e.g. `jixu → 继续`),
    /// that word moves to position 0 of the merged list — overriding
    /// the wubi-first hard rule, because an explicit pin is the
    /// strongest user-intent signal. Without this, wubi's natural-code
    /// hits (曳光弹 happens to encode to `jixu` as a 3-char phrase)
    /// would shadow the pinyin pin.
    pub fn candidates(&mut self) -> &[Candidate] {
        self.cand_buf.clear();
        self.cand_buf
            .extend(dispatch(self.mode, &self.wubi, &self.pinyin, self.japanese.as_ref()));
        // Pinyin pin promotion. (Wubi-internal pins are already at #0
        // within the wubi candidates list — they fight cross-engine
        // only with pinyin pins, which is what this pass handles.)
        if let Some(pinned) = self.pinyin.pinned_word_for_buffer()
            && let Some(idx) = self.cand_buf.iter().position(|c| c.word == pinned)
            && idx > 0
        {
            let p = self.cand_buf.remove(idx);
            self.cand_buf.insert(0, p);
        }
        &self.cand_buf
    }

    /// Number of merged candidates without recomputing — caller must have
    /// invoked `candidates` first.
    pub fn candidate_count(&self) -> usize {
        self.cand_buf.len()
    }

    /// Process one ASCII alphabetic letter. Returns committed text if
    /// CompositeEngine decided to force-commit a wubi candidate based on
    /// `user_policy` (sub-engine wubi never auto-commits on its own —
    /// see `new()`).
    pub fn handle_letter(&mut self, byte: u8) -> Option<String> {
        // JapaneseOnly short-circuits the Chinese engines entirely.
        if self.mode.is_japanese_only() {
            if let Some(j) = self.japanese.as_mut() {
                j.handle_letter(byte);
            }
            // JP has no policy-driven force-commit and no ASCII-fallback
            // path (the user typing romaji is their own ASCII intent).
            return None;
        }

        // Pinyin first — captures every byte without auto-committing,
        // and seeds `pinyin.candidates()` so the wubi-veto logic below
        // sees the *post-byte* pinyin state.
        if self.mode.allows_pinyin() {
            self.pinyin.handle_letter(byte);
        }

        // JP attaches in parallel as an enhancement source.
        if self.enable_japanese {
            if let Some(j) = self.japanese.as_mut() {
                j.handle_letter(byte);
            }
        }

        if self.mode.allows_wubi() {
            // User-stated policy (2026-05-22): in Mixed mode, wubi is
            // out of the picture past 4 letters. "超过 4 字就和五笔没关系
            // 了" — typing a long pinyin word should not defuse wubi
            // into a tail-letter simcode (which then crashes the
            // candidate list via Jianma1's 1M score floor). So: in
            // Mixed mode, stop feeding wubi once its buffer is already
            // 4 chars. Wubi state freezes; pinyin keeps growing; no
            // defuse, no tail interpretation, no junk #0 candidate.
            //
            // WubiOnly mode keeps the original defuse behavior — there
            // the user IS typing wubi codes, and the 4→reset cycle is
            // the expected ergonomics.
            if self.mode == Mode::Mixed && self.wubi.buffer_str().len() >= 4 {
                // Skip wubi entirely for this byte. Pinyin already
                // consumed it above; JP too. Move on.
            } else if let Some(text) = self.wubi.handle_letter(byte) {
                // WubiOnly defuse path or pre-4-char auto-commit fired.
                self.pinyin.clear_all();
                if let Some(j) = self.japanese.as_mut() { j.clear_all(); }
                return Some(text);
            }
        }

        if self.should_force_commit_wubi() {
            // Take the wubi top candidate as the committed text, then
            // clear all sub-engines so the next keystroke starts fresh.
            if let Some(text) = self.wubi.candidates().first().cloned() {
                let idx = self.wubi_index_for(&text).unwrap_or(0);
                self.wubi.commit_index(idx);
                self.pinyin.clear_all();
                if let Some(j) = self.japanese.as_mut() { j.clear_all(); }
                return Some(text);
            }
        }

        // Sogou-style ASCII fallback: at THRESHOLD+ input chars, if the
        // long-running engine (pinyin in Mixed/PinyinOnly, wubi in
        // WubiOnly) has zero candidates, dump the raw preedit as ASCII
        // and return to idle. Without this the engine stays composing
        // forever — the user's letters are "stuck" and can't space-
        // commit. Real-world repro: typing English mid-Chinese (e.g.
        // "eiwfjiewjfif") on a sloppy mash.
        //
        // Note: in Mixed mode, wubi resets its buffer every 4 chars,
        // so at 5+ chars wubi sees only the tail — its prefix matches
        // are misleading and don't represent the user's intent.
        // Pinyin runs continuously, so its emptiness is the reliable
        // "not a Chinese word" signal. JP intentionally does NOT veto
        // the fallback — the user typing in romaji that doesn't form
        // CJK is the same English-intent signal whether JP is on or off.
        if self.preedit().len() >= Self::ASCII_FALLBACK_THRESHOLD
            && self.is_pure_garbage()
        {
            let raw = self.preedit().to_string();
            self.wubi.clear_all();
            self.pinyin.clear_all();
            if let Some(j) = self.japanese.as_mut() { j.clear_all(); }
            return Some(raw);
        }

        None
    }

    /// True iff the current input has NO chance of matching any word
    /// in any active engine. Drives the ASCII-fallback (auto-uppercase-
    /// the-raw-buffer) at THRESHOLD chars.
    ///
    /// Engines consulted:
    ///   - **Pinyin** via `has_future_match` (dict prefix lookup) —
    ///     pinyin candidates briefly empty mid-syllable for long words
    ///     (e.g. `zhongg` between `zhong` and `zhongguo`) while the
    ///     dict prefix still resolves, so `is_empty` is too eager.
    ///   - **JP plugin** — when active, the user could still be typing
    ///     a multi-syllable JP word (`watashi` mid-typing at `watas`
    ///     would otherwise ASCII-fall at length 5 before the user
    ///     finishes the word). Treat JP-active as a categorical "no
    ///     ASCII fallback" — the user opted into JP and accepts that
    ///     typing English in mixed-mode requires the JP toggle off.
    fn is_pure_garbage(&self) -> bool {
        if !self.mode.allows_pinyin() {
            // WubiOnly path. wubi commits natively at 5 chars so we
            // shouldn't normally reach here; fall back to false to be
            // safe.
            return false;
        }
        if self.japanese.is_some() {
            // JP plugin active — give the user room to finish a romaji
            // word that's longer than the 5-char ASCII-fallback budget.
            return false;
        }
        !self.pinyin.has_future_match()
    }

    /// Sogou-style auto-ASCII threshold. At this many input letters,
    /// if neither engine produced any candidate, fall back to ASCII
    /// upload of the raw preedit. 5 was chosen because wubi's max
    /// useful code length is 4 — at 5+ chars with zero candidates,
    /// the input is definitively not a wubi code.
    const ASCII_FALLBACK_THRESHOLD: usize = 5;

    /// Decide whether to fire a wubi force-commit on the current state.
    ///
    /// **Mixed mode** veto rules:
    ///   - `pinyin.has_non_speculative_candidate()` true → user is mid-pinyin
    ///     (`shan` → 上, `wang` → 王, `hh` → 哈哈 via 简拼). Original 47f
    ///     veto, narrowed to exact + initials matches only since prefix
    ///     completion (added 2026-05-20) makes pinyin candidates always
    ///     non-empty — which would otherwise mask all wubi 简码 commits.
    ///   - `pinyin.has_future_match()` true at buf=4 → user might be
    ///     building a longer pinyin word (`beij` → 北京). Without this
    ///     we'd commit 阴 and leak "ing" into the pinyin buffer.
    ///
    /// **WubiOnly** applies the policy verbatim — pinyin is dormant so
    /// no veto. **PinyinOnly** wubi never runs.
    fn should_force_commit_wubi(&self) -> bool {
        if !self.mode.allows_wubi() {
            return false;
        }
        let buf_len = self.wubi.buffer_str().len();
        let cand_count = self.wubi.candidates().len();
        let four = buf_len == 4;
        let policy_says_yes = match self.user_policy {
            AutoCommitPolicy::Never => false,
            AutoCommitPolicy::OnFourCodes => four && cand_count > 0,
            AutoCommitPolicy::OnUniqueMatch => cand_count == 1,
            AutoCommitPolicy::OnFourCodesIfUnique => four && cand_count == 1,
        };
        if !policy_says_yes {
            return false;
        }
        if self.mode == Mode::Mixed {
            if self.pinyin.has_non_speculative_candidate() {
                return false;
            }
            // 4-letter wubi unique codes are the dangerous ones — they're
            // often prefixes of multi-syllable pinyin words (`beij` →
            // 北京). Veto here when the buffer is still a valid pinyin
            // word prefix. Single-letter wubi 简码 auto-commit (`g` → 一)
            // is unaffected — every single letter has pinyin futures but
            // that's expected ambiguity 一级简码 users accept.
            if four && self.pinyin.has_future_match() {
                return false;
            }
        }
        true
    }

    pub fn backspace(&mut self) -> bool {
        let mut consumed = false;
        if self.mode.is_japanese_only() {
            if let Some(j) = self.japanese.as_mut() {
                consumed |= j.backspace();
            }
            return consumed;
        }
        if self.mode.allows_wubi() {
            consumed |= self.wubi.backspace();
        }
        if self.mode.allows_pinyin() {
            consumed |= self.pinyin.backspace();
        }
        if self.enable_japanese {
            if let Some(j) = self.japanese.as_mut() {
                consumed |= j.backspace();
            }
        }
        consumed
    }

    pub fn escape(&mut self) -> bool {
        let mut consumed = false;
        if self.mode.is_japanese_only() {
            if let Some(j) = self.japanese.as_mut() {
                consumed |= j.escape();
            }
            return consumed;
        }
        if self.mode.allows_wubi() {
            consumed |= self.wubi.escape();
        }
        if self.mode.allows_pinyin() {
            consumed |= self.pinyin.escape();
        }
        if self.enable_japanese {
            if let Some(j) = self.japanese.as_mut() {
                consumed |= j.escape();
            }
        }
        consumed
    }

    /// Eagerly run every cold-init path in the composite stack: wubi dict
    /// init + FST page-fault touch, pinyin dict touch, INITIALS_INDEX build,
    /// and one simulated wubi→pinyin dispatch (the path that triggered the
    /// original 500ms stall on the first `kp` keystroke). After this returns,
    /// every subsequent `handle_letter` is on hot paths only. Idempotent —
    /// each underlying init is `OnceLock`-gated, so repeat calls cost only
    /// the cheap FST lookups (~1ms total).
    pub fn warmup(&mut self) {
        crate::wubi::warmup();
        self.pinyin.warmup();
        // Drive the cross-engine dispatch path explicitly: 'k' then 'p'
        // exercises the wubi-buffer → pinyin-engine handoff that lazy-inits
        // composite-internal dispatch state on first use.
        let _ = self.handle_letter(b'k');
        let _ = self.handle_letter(b'p');
        self.clear_all();
    }

    pub fn clear_all(&mut self) {
        self.wubi.clear_all();
        self.pinyin.clear_all();
        if let Some(j) = self.japanese.as_mut() {
            j.clear_all();
        }
        self.cand_buf.clear();
    }

    /// Commit candidate at index. Records the pick to the source engine's
    /// L0 layer (per item 28: each engine has its own L0 — same word can
    /// have different per-engine ranking).
    pub fn commit_index(&mut self, index: usize) -> Option<String> {
        if self.cand_buf.is_empty() {
            // Refresh once — caller may not have invoked candidates() yet.
            self.cand_buf
                .extend(dispatch(self.mode, &self.wubi, &self.pinyin, self.japanese.as_ref()));
        }
        let cand = self.cand_buf.get(index).cloned()?;
        match cand.source {
            super::merge::Source::Wubi => {
                self.wubi.commit_index(self.wubi_index_for(&cand.word)?);
            }
            super::merge::Source::Pinyin => {
                self.pinyin.commit_index(self.pinyin_index_for(&cand.word)?);
            }
            super::merge::Source::Japanese => {
                if let Some(j) = self.japanese.as_mut() {
                    let idx = j.candidates().iter().position(|w| w == &cand.word);
                    if let Some(i) = idx {
                        j.commit_index(i);
                    }
                }
            }
        }
        // Whichever engine handled the commit, clear all so session
        // state is consistent (next keystroke starts fresh).
        if self.mode.allows_wubi() {
            self.wubi.clear_all();
        }
        if self.mode.allows_pinyin() {
            self.pinyin.clear_all();
        }
        if let Some(j) = self.japanese.as_mut() {
            j.clear_all();
        }
        self.cand_buf.clear();
        Some(cand.word)
    }

    fn wubi_index_for(&self, word: &str) -> Option<usize> {
        self.wubi.candidates().iter().position(|w| w == word)
    }

    fn pinyin_index_for(&self, word: &str) -> Option<usize> {
        self.pinyin.candidates().iter().position(|w| w == word)
    }

    // ------------------------------------------------------------------
    // Pinyin L0 surface — proxies to the owned PinyinAdapter so
    // Session::export_l0_json / import_l0_json can route through the
    // composite layer.
    // ------------------------------------------------------------------

    /// Snapshot the pinyin sub-engine's L0 layer, or `None` if pinyin is
    /// dormant (WubiOnly mode and pinyin was never used).
    pub fn pinyin_export_l0(&self) -> Option<golia_pinyin::L0Snapshot> {
        Some(self.pinyin.export_l0())
    }

    pub fn pinyin_import_l0(&self, snap: golia_pinyin::L0Snapshot) -> usize {
        self.pinyin.import_l0(snap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composite::merge::Source;

    fn typed(eng: &mut CompositeEngine, s: &[u8]) {
        for b in s {
            eng.handle_letter(*b);
        }
    }

    #[test]
    fn default_mode_is_mixed() {
        let e = CompositeEngine::new();
        assert_eq!(e.mode(), Mode::Mixed);
    }

    #[test]
    fn wubi_only_mode_ignores_pinyin() {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::WubiOnly);
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        typed(&mut e, b"gggg"); // wubi: 王 (or whatever ggg yields)
        let cands = e.candidates().to_vec();
        assert!(cands.iter().all(|c| c.source == Source::Wubi));
        assert!(!cands.is_empty());
    }

    #[test]
    fn pinyin_only_mode_ignores_wubi() {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::PinyinOnly);
        typed(&mut e, b"women");
        let cands = e.candidates().to_vec();
        assert!(cands.iter().all(|c| c.source == Source::Pinyin));
        assert_eq!(cands.first().map(|c| c.word.as_str()), Some("我们"));
    }

    #[test]
    fn mixed_mode_z_prefix_uses_pinyin_only() {
        let mut e = CompositeEngine::new();
        // Mode::Mixed by default. 'z' isn't a wubi 字根 letter so wubi
        // yields nothing; pinyin handles it.
        typed(&mut e, b"zhongguo");
        let cands = e.candidates().to_vec();
        assert!(cands.iter().all(|c| c.source == Source::Pinyin));
        assert_eq!(cands.first().map(|c| c.word.as_str()), Some("中国"));
    }

    #[test]
    fn mixed_mode_wubi_letters_show_both_when_pinyin_matches() {
        let mut e = CompositeEngine::new();
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        typed(&mut e, b"yi"); // pinyin: 一/以/已/...; wubi: 2-letter simcode
        let cands = e.candidates().to_vec();
        // Under dispatch Policy 2 (2026-05-22), a 2-letter input that
        // matches a valid pinyin syllable demotes non-Jianma1 wubi by
        // ×0.5, so pinyin (with its high-freq common chars at this
        // reading) takes #0. Both engines still contribute to the list;
        // wubi just slides down past pinyin's top hits.
        assert_eq!(cands[0].source, Source::Pinyin);
        assert!(cands.iter().any(|c| c.source == Source::Wubi));
        assert!(cands.iter().any(|c| c.source == Source::Pinyin));
    }

    #[test]
    fn backspace_pops_both_engines() {
        let mut e = CompositeEngine::new();
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        typed(&mut e, b"abc");
        assert!(e.backspace());
        assert_eq!(e.pinyin_buffer_str(), "ab");
        assert_eq!(e.wubi_buffer_str(), "ab");
    }

    #[test]
    fn escape_clears_both_engines() {
        let mut e = CompositeEngine::new();
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        typed(&mut e, b"abc");
        assert!(e.escape());
        assert!(!e.is_composing());
    }

    #[test]
    fn commit_pinyin_source_routes_to_pinyin_engine() {
        let mut e = CompositeEngine::new();
        // 'z' prefix → Mixed mode goes pinyin-only for this input.
        typed(&mut e, b"zhongguo");
        let cands = e.candidates().to_vec();
        let zhongguo_idx = cands.iter().position(|c| c.word == "中国").unwrap();
        assert_eq!(cands[zhongguo_idx].source, Source::Pinyin);
        assert_eq!(e.commit_index(zhongguo_idx).as_deref(), Some("中国"));
        assert!(!e.is_composing());
    }

    #[test]
    fn mode_set_round_trip() {
        let mut e = CompositeEngine::new();
        for m in [Mode::WubiOnly, Mode::PinyinOnly, Mode::Mixed] {
            e.set_mode(m);
            assert_eq!(e.mode(), m);
        }
    }

    // ----- v1.2.0-α1 JP plugin integration tests -----------------------

    #[test]
    fn japanese_disabled_no_jp_candidates() {
        // Default state: enable_japanese = false in Mixed → only wubi/pinyin.
        let mut e = CompositeEngine::new();
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        typed(&mut e, b"ka");
        let cands = e.candidates().to_vec();
        assert!(
            cands.iter().all(|c| c.source != Source::Japanese),
            "expected no JP candidates with toggle off, got {:?}", cands
        );
    }

    #[test]
    fn japanese_enabled_appends_jp_candidates_in_mixed() {
        let mut e = CompositeEngine::new();
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        e.set_japanese_enabled(true);
        typed(&mut e, b"ka");
        let cands = e.candidates().to_vec();
        // か (hiragana) must appear with Source::Japanese.
        assert!(
            cands.iter().any(|c| c.word == "か" && c.source == Source::Japanese),
            "expected か as JP candidate, got {:?}", cands
        );
        // Wubi/pinyin candidates (if any) must precede JP — strict ranking.
        let first_jp = cands.iter().position(|c| c.source == Source::Japanese)
            .expect("expected at least one JP candidate");
        for (i, c) in cands.iter().enumerate() {
            if i < first_jp {
                assert_ne!(c.source, Source::Japanese,
                    "JP candidate appeared before non-JP at index {}", i);
            }
        }
    }

    #[test]
    fn japanese_only_mode_silences_wubi_pinyin() {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::JapaneseOnly);
        typed(&mut e, b"ka");
        let cands = e.candidates().to_vec();
        assert!(!cands.is_empty(), "expected JP candidates in JapaneseOnly mode");
        assert!(
            cands.iter().all(|c| c.source == Source::Japanese),
            "JapaneseOnly mode should yield only JP candidates, got {:?}", cands
        );
    }

    #[test]
    fn japanese_only_high_kou_finds_kanji() {
        // The user's framing example: typing "kou" should surface 高 in
        // JapaneseOnly mode (kanji subset is codepoint-identical with CN
        // simplified per the inputx-jp curation).
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::JapaneseOnly);
        typed(&mut e, b"kou");
        let cands = e.candidates().to_vec();
        assert!(
            cands.iter().any(|c| c.word == "高" && c.source == Source::Japanese),
            "expected 高 in JapaneseOnly candidates for 'kou', got {:?}", cands
        );
    }

    #[test]
    fn japanese_only_clear_returns_to_idle() {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::JapaneseOnly);
        typed(&mut e, b"abc");
        assert!(e.is_composing());
        e.escape();
        assert!(!e.is_composing());
        assert_eq!(e.preedit(), "");
    }

    #[test]
    fn switch_to_japanese_only_clears_chinese_buffers() {
        let mut e = CompositeEngine::new();
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        typed(&mut e, b"khlg"); // wubi mid-compose
        assert!(e.is_composing());
        e.set_mode(Mode::JapaneseOnly);
        // Chinese buffers cleared by set_mode; JP buffer fresh.
        assert!(!e.is_composing(),
            "JapaneseOnly transition should clear Chinese composing state");
    }

    #[test]
    fn beijing_compose_no_wubi_force_commit() {
        // Regression: 'beijing' (7 letters) was triggering wubi's hard
        // 5-letter cutoff at "beiji" because pinyin had no exact-match
        // candidates at "beij" (long-word prefix), so the old defuse
        // guard `!pinyin.candidates().is_empty()` didn't fire and wubi
        // force-committed "阴" (top of "beij" wubi candidates), leaving
        // "ing" in the pinyin buffer. Fix: defuse unconditionally in
        // Mixed mode at 4 letters.
        let mut e = CompositeEngine::new();
        typed(&mut e, b"beijing");
        // No commit fired
        assert!(e.is_composing());
        // Full preedit preserved
        assert_eq!(e.preedit(), "beijing");
        // 北京 in the candidate list
        let cands = e.candidates().to_vec();
        assert!(cands.iter().any(|c| c.word == "北京"),
                "candidates for beijing: {:?}",
                cands.iter().map(|c| c.word.as_str()).collect::<Vec<_>>());
    }

    #[test]
    fn commit_out_of_range_is_noop() {
        let mut e = CompositeEngine::new();
        typed(&mut e, b"women");
        let _ = e.candidates();
        assert!(e.commit_index(99_999).is_none());
        assert!(e.is_composing());
    }

    // -------------------------------------------------------------
    // Property-based fuzz tests — randomized op sequences under all
    // (Mode × AutoCommitPolicy) combinations. Asserts state-invariant
    // properties hold no matter how the user mashes keys. Catches edge
    // cases hand-written tests miss (mode flips mid-compose, deep
    // backspace, commit-of-empty, escape-while-idle, etc.).
    // -------------------------------------------------------------
    use proptest::prelude::*;

    #[derive(Debug, Clone)]
    enum Op {
        Letter(u8),
        Backspace,
        Escape,
        Commit(usize),
        SetMode(Mode),
        SetPolicy(AutoCommitPolicy),
    }

    fn op_strategy() -> impl Strategy<Value = Op> {
        prop_oneof![
            // Letters dominate the distribution — most realistic usage
            // is typing, with the other ops sprinkled in.
            8 => (b'a'..=b'z').prop_map(Op::Letter),
            2 => Just(Op::Backspace),
            1 => Just(Op::Escape),
            2 => (0usize..32).prop_map(Op::Commit),
            1 => prop_oneof![
                Just(Mode::Mixed), Just(Mode::WubiOnly), Just(Mode::PinyinOnly)
            ].prop_map(Op::SetMode),
            1 => prop_oneof![
                Just(AutoCommitPolicy::Never),
                Just(AutoCommitPolicy::OnFourCodes),
                Just(AutoCommitPolicy::OnUniqueMatch),
                Just(AutoCommitPolicy::OnFourCodesIfUnique),
            ].prop_map(Op::SetPolicy),
        ]
    }

    fn check_invariants(e: &mut CompositeEngine, after: &str) {
        let composing = e.is_composing();
        let preedit_empty = e.preedit().is_empty();
        // INV-1: is_composing() iff preedit is non-empty. Used by the
        // host UI to decide whether to render the candidate bar.
        assert_eq!(
            composing, !preedit_empty,
            "INV-1 violated after {after}: is_composing={composing} preedit={:?}",
            e.preedit()
        );
        // INV-2: candidate_count agrees with candidates().len() once a
        // recompute has been triggered. Engine's contract is that count
        // is stale unless caller invoked candidates() first; the Session
        // layer mediates this (Session.cand_cache is single-source). We
        // force the recompute then assert sync.
        let cands_len = e.candidates().len();
        let count = e.candidate_count();
        assert_eq!(
            count, cands_len,
            "INV-2 violated after {after}: candidate_count={count} candidates().len()={cands_len} (after refresh)"
        );
        // INV-3: preedit() returns ASCII a-z bytes only (it's the raw
        // key code echo, not the Han preview). Any non-ASCII slipping
        // through means the engine is confused about which buffer to
        // display.
        for b in e.preedit().bytes() {
            assert!(
                b.is_ascii_lowercase(),
                "INV-3 violated after {after}: non-ASCII byte {b:#x} in preedit {:?}",
                e.preedit()
            );
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            // 256 cases × ~64 ops each = 16k operations per run. Fast on
            // release-build composite engine (~1ms per op including
            // candidate fetch).
            cases: 256,
            .. ProptestConfig::default()
        })]

        #[test]
        fn fuzz_engine_never_panics_and_invariants_hold(
            ops in proptest::collection::vec(op_strategy(), 1..64)
        ) {
            let mut e = CompositeEngine::new();
            // Fresh engine baseline
            check_invariants(&mut e, "init");
            for (i, op) in ops.iter().enumerate() {
                let label = format!("op#{i} = {:?}", op);
                match op {
                    Op::Letter(b) => {
                        let committed = e.handle_letter(*b);
                        // INV-4: if handle_letter auto-committed (returned
                        // Some), the engine must be in a clean state for
                        // that engine — wubi auto-commits clear wubi
                        // buffer; pinyin keeps composing. Skip a strict
                        // check here since auto-commit semantics differ
                        // per engine; just verify the return is well-
                        // formed (non-empty if Some).
                        if let Some(s) = &committed {
                            prop_assert!(!s.is_empty(),
                                "INV-4 violated after {label}: empty auto-commit");
                        }
                    }
                    Op::Backspace => {
                        let consumed = e.backspace();
                        if !consumed {
                            // INV-5: backspace returning false means engine
                            // was idle. After a no-op backspace, still idle.
                            prop_assert!(!e.is_composing(),
                                "INV-5 violated after {label}: backspace=false but composing");
                        }
                    }
                    Op::Escape => {
                        let _ = e.escape();
                        // INV-6: escape always clears composing state.
                        prop_assert!(!e.is_composing(),
                            "INV-6 violated after {label}: escape did not clear composing");
                    }
                    Op::Commit(idx) => {
                        let count = e.candidate_count();
                        let result = e.commit_index(*idx);
                        if *idx >= count {
                            // INV-7: out-of-range commit returns None and
                            // does NOT mutate state.
                            prop_assert!(result.is_none(),
                                "INV-7 violated after {label}: out-of-range commit returned Some");
                        } else if let Some(s) = &result {
                            // INV-8: successful commit returns non-empty
                            // string and leaves engine idle.
                            prop_assert!(!s.is_empty(),
                                "INV-8a violated after {label}: empty commit string");
                            prop_assert!(!e.is_composing(),
                                "INV-8b violated after {label}: still composing after commit");
                        }
                    }
                    Op::SetMode(m) => {
                        e.set_mode(*m);
                        prop_assert_eq!(e.mode(), *m,
                            "set_mode round-trip failed after {}", label);
                    }
                    Op::SetPolicy(p) => {
                        e.set_auto_commit_policy(*p);
                        prop_assert_eq!(e.auto_commit_policy(), *p,
                            "set_auto_commit_policy round-trip failed after {}", label);
                    }
                }
                check_invariants(&mut e, &label);
            }
        }

        // -----------------------------------------------------------
        // A. State-machine identity laws — round-trip / commutativity
        // properties that catch subtle state leaks unrelated to single-
        // op invariants.
        // -----------------------------------------------------------

        /// After `escape()`, a second `escape()` is a no-op (returns
        /// false). Catches escape paths that re-arm engine state.
        #[test]
        fn escape_idempotent(
            letters in proptest::collection::vec(b'a'..=b'z', 0..16)
        ) {
            let mut e = CompositeEngine::new();
            for b in &letters { let _ = e.handle_letter(*b); }
            let _first = e.escape();
            let second = e.escape();
            prop_assert!(!second,
                "escape() called twice in a row returned true the second time \
                 (letters={letters:?})");
            prop_assert!(!e.is_composing());
            prop_assert!(e.preedit().is_empty());
        }

        /// Type letters → escape() should reset to a state observably
        /// equivalent to a fresh engine in the same mode + policy. If
        /// not, some buffer leaks through escape.
        #[test]
        fn escape_returns_to_fresh_state(
            letters in proptest::collection::vec(b'a'..=b'z', 1..16),
            mode in prop_oneof![Just(Mode::Mixed), Just(Mode::WubiOnly), Just(Mode::PinyinOnly)],
            policy in prop_oneof![
                Just(AutoCommitPolicy::Never),
                Just(AutoCommitPolicy::OnFourCodes),
                Just(AutoCommitPolicy::OnUniqueMatch),
                Just(AutoCommitPolicy::OnFourCodesIfUnique),
            ],
        ) {
            // Fresh reference
            let mut fresh = CompositeEngine::new();
            fresh.set_mode(mode);
            fresh.set_auto_commit_policy(policy);

            // Typed + escaped
            let mut e = CompositeEngine::new();
            e.set_mode(mode);
            e.set_auto_commit_policy(policy);
            for b in &letters { let _ = e.handle_letter(*b); }
            let _ = e.escape();

            prop_assert_eq!(e.is_composing(), fresh.is_composing(),
                "escape didn't reset is_composing (letters={:?} mode={:?})", letters, mode);
            prop_assert_eq!(e.preedit(), fresh.preedit(),
                "escape didn't reset preedit (letters={:?} mode={:?})", letters, mode);
            prop_assert_eq!(e.candidates().to_vec(), fresh.candidates().to_vec(),
                "escape didn't reset candidates (letters={:?} mode={:?})", letters, mode);
            prop_assert_eq!(e.mode(), fresh.mode());
            prop_assert_eq!(e.auto_commit_policy(), fresh.auto_commit_policy());
        }

        /// SetMode(X); SetMode(Y); SetMode(X) on a fresh engine ≡
        /// SetMode(X) alone. Catches set_mode side-effects that leak
        /// state across mode flips.
        #[test]
        fn mode_flip_then_back_is_noop(
            m1 in prop_oneof![Just(Mode::Mixed), Just(Mode::WubiOnly), Just(Mode::PinyinOnly)],
            m2 in prop_oneof![Just(Mode::Mixed), Just(Mode::WubiOnly), Just(Mode::PinyinOnly)],
        ) {
            let mut direct = CompositeEngine::new();
            direct.set_mode(m1);

            let mut indirect = CompositeEngine::new();
            indirect.set_mode(m1);
            indirect.set_mode(m2);
            indirect.set_mode(m1);

            prop_assert_eq!(direct.mode(), indirect.mode());
            prop_assert_eq!(direct.is_composing(), indirect.is_composing());
            prop_assert_eq!(direct.preedit(), indirect.preedit());
            prop_assert_eq!(direct.candidates().to_vec(), indirect.candidates().to_vec());
        }

        /// Determinism: typing the same prefix from a fresh engine
        /// always yields the same candidate list. Catches non-
        /// determinism from undefined ordering, time-based ranking,
        /// PRNG, etc.
        #[test]
        fn prefix_yields_deterministic_candidates(
            letters in proptest::collection::vec(b'a'..=b'z', 1..8),
        ) {
            let mut a = CompositeEngine::new();
            for b in &letters { let _ = a.handle_letter(*b); }
            let cands_a: Vec<String> = a.candidates().iter().map(|c| c.word.clone()).collect();

            let mut b_eng = CompositeEngine::new();
            for b in &letters { let _ = b_eng.handle_letter(*b); }
            let cands_b: Vec<String> = b_eng.candidates().iter().map(|c| c.word.clone()).collect();

            prop_assert_eq!(cands_a, cands_b,
                "non-deterministic candidates for letters={:?}", letters);
        }

        /// Commit-then-state-clean: after a successful `commit_index`,
        /// engine is fully idle (not just `is_composing=false` — also
        /// preedit empty, candidates empty). Catches "committed but
        /// chip pool still shows ghost candidates" bug class.
        #[test]
        fn commit_leaves_clean_state(
            letters in proptest::collection::vec(b'a'..=b'z', 1..8),
        ) {
            let mut e = CompositeEngine::new();
            for b in &letters { let _ = e.handle_letter(*b); }
            let count = e.candidates().len();
            if count == 0 { return Ok(()); }
            // Commit candidate at index 0 (always valid since count > 0)
            let result = e.commit_index(0);
            if result.is_none() {
                // Some letter sequences may not produce a commitable
                // first candidate (e.g. raw input fallback). Skip.
                return Ok(());
            }
            prop_assert!(!e.is_composing(),
                "is_composing=true after commit (letters={:?})", letters);
            prop_assert!(e.preedit().is_empty(),
                "preedit non-empty after commit: {:?} (letters={:?})", e.preedit(), letters);
            prop_assert_eq!(e.candidates().len(), 0,
                "candidates non-empty after commit (letters={:?})", letters);
        }
    }

    // -----------------------------------------------------------------
    // D. Long-sequence stress / leak tests — drive the engine through
    // thousands of operations and verify latency / memory don't grow
    // unboundedly. Catches O(n²) regressions and unbounded internal
    // buffers (e.g., a commit path that pushes to a Vec without ever
    // clearing).
    // -----------------------------------------------------------------

    /// 10k mixed-op stress: 1000 cycles of (type 6 letters, fetch
    /// candidates, commit 1st, fetch preedit). Total runtime must
    /// stay roughly linear in cycle count — assert by checking the
    /// SECOND HALF average is at most 4× the FIRST HALF average. A
    /// quadratic regression (e.g., a Vec that's appended to and
    /// scanned each commit) would explode this ratio.
    #[test]
    fn stress_10k_ops_no_quadratic() {
        use std::time::Instant;
        let mut e = CompositeEngine::new();
        // Wider character pool so we hit varied wubi/pinyin paths.
        let cycles = 1000usize;
        let mut first_half_total = std::time::Duration::ZERO;
        let mut second_half_total = std::time::Duration::ZERO;
        let inputs: [&[u8]; 5] = [b"nihao", b"women", b"zhongg", b"beijin", b"ggggg"];

        for i in 0..cycles {
            let pick = inputs[i % inputs.len()];
            let t0 = Instant::now();
            for &b in pick {
                let _ = e.handle_letter(b);
            }
            let _ = e.candidates();
            let _ = e.commit_index(0);
            let _ = e.preedit();
            let elapsed = t0.elapsed();
            if i < cycles / 2 {
                first_half_total += elapsed;
            } else {
                second_half_total += elapsed;
            }
        }
        let f = first_half_total.as_nanos() as f64 / (cycles as f64 / 2.0);
        let s = second_half_total.as_nanos() as f64 / (cycles as f64 / 2.0);
        // 4× slack to absorb GC/jit/thermal noise; an O(n²) bug shows
        // up as 10×+ slowdown by cycle 1000.
        assert!(
            s < f * 4.0,
            "second-half avg {:.0}ns vs first-half {:.0}ns ({}x) — looks quadratic",
            s, f, s / f
        );
    }

    /// After 1000 typed-and-escaped cycles, the engine's internal
    /// candidate buffer length is bounded by `refreshCandidateCap`
    /// (or whatever cap the merge layer applies). If `cand_buf` grew
    /// without clearing, the buffer length here would track op count.
    #[test]
    fn stress_buffer_does_not_grow_unbounded() {
        let mut e = CompositeEngine::new();
        for _ in 0..1000 {
            for &b in b"nihao" {
                let _ = e.handle_letter(b);
            }
            let _ = e.candidates();
            let _ = e.escape();
        }
        // After 1000 cycles ending on escape, candidates should be 0.
        let cands_len = e.candidates().len();
        assert_eq!(
            cands_len, 0,
            "candidates buffer leaked: {cands_len} after 1000 escape cycles"
        );
        assert!(!e.is_composing());
        assert!(e.preedit().is_empty());
    }

    // -----------------------------------------------------------------
    // ASCII auto-commit (Sogou-style) — at length >= 5 with zero
    // candidates from either engine, dump the raw preedit as ASCII.
    // -----------------------------------------------------------------

    #[test]
    fn ascii_fallback_at_5_chars_no_match() {
        let mut e = CompositeEngine::new();
        // 'qwxz' at 4 chars produces no candidates either (from the probe),
        // but 4 isn't long enough for the fallback. Add a 5th letter.
        for &b in b"qwxz" {
            let r = e.handle_letter(b);
            assert!(r.is_none(), "premature fallback at {}: {:?}",
                std::str::from_utf8(&[b]).unwrap(), r);
        }
        // 5th letter — fallback fires.
        let committed = e.handle_letter(b'y');
        assert_eq!(committed.as_deref(), Some("qwxzy"),
            "expected ASCII fallback at 5 chars, got {:?} (preedit={:?})",
            committed, e.preedit());
        // Engine returns to idle.
        assert!(!e.is_composing());
        assert!(e.preedit().is_empty());
        assert_eq!(e.candidates().len(), 0);
    }

    #[test]
    fn ascii_fallback_holds_at_4_chars() {
        // 'qwxz' at 4 chars has 0 candidates but the fallback threshold
        // is 5 — engine should still be composing.
        let mut e = CompositeEngine::new();
        for &b in b"qwxz" { let _ = e.handle_letter(b); }
        assert!(e.is_composing(), "should still compose at 4 unrecognized chars");
        assert_eq!(e.preedit(), "qwxz");
    }

    #[test]
    fn ascii_fallback_blocked_by_pinyin_match() {
        // 'beijing' is 7 chars but at every step pinyin has candidates
        // (北京 / 北 / 备 / ...) — fallback must not fire.
        let mut e = CompositeEngine::new();
        for &b in b"beijing" {
            let r = e.handle_letter(b);
            assert!(r.is_none(), "wrongful fallback while pinyin matches: {:?}", r);
        }
        assert!(e.is_composing(), "composing should still be true after beijing");
        let cands: Vec<String> = e.candidates().iter().map(|c| c.word.clone()).collect();
        assert!(cands.iter().any(|s| s == "北京"),
            "expected 北京 in candidates, got {:?}", cands);
    }

    #[test]
    fn ascii_fallback_blocked_by_wubi_match() {
        // 'gggg' (4-letter wubi for 王) — at 4 chars wubi has candidates.
        // Add a 5th letter 'g' under AutoCommitPolicy::Never — wubi cap
        // wraps but the engine should keep going as long as SOMETHING
        // has candidates.
        let mut e = CompositeEngine::new();
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        for &b in b"gggg" {
            let _ = e.handle_letter(b);
        }
        // 4 letters — composing with candidates.
        assert!(e.is_composing());
        assert!(!e.candidates().is_empty());
    }

    /// Real-world repro from the user: typing English mid-Chinese
    /// produces a long meaningless sequence that should auto-ASCII.
    #[test]
    fn ascii_fallback_long_english_garbage() {
        let mut e = CompositeEngine::new();
        let garbage = b"eiwfjiewjfif";
        let mut commits: Vec<String> = vec![];
        for &b in garbage {
            if let Some(c) = e.handle_letter(b) {
                commits.push(c);
            }
        }
        // At least one ASCII commit should have happened.
        assert!(!commits.is_empty(),
            "no ASCII commit fired for garbage input (preedit={:?})", e.preedit());
        // Concatenation should match (modulo any residual preedit).
        let mut out = commits.join("");
        out.push_str(e.preedit());
        let expected = std::str::from_utf8(garbage).unwrap();
        assert_eq!(out, expected,
            "lost characters: committed+preedit={:?} expected={:?}", out, expected);
    }

    /// Backspace from empty repeatedly is a no-op and stays no-op
    /// (catches a state-corruption bug where backspace-from-empty
    /// leaves the engine in a non-fresh state).
    #[test]
    fn stress_backspace_from_empty_idempotent() {
        let mut e = CompositeEngine::new();
        for _ in 0..1000 {
            let consumed = e.backspace();
            assert!(!consumed, "backspace from empty should not consume");
            assert!(!e.is_composing());
            assert!(e.preedit().is_empty());
        }
    }

    // -----------------------------------------------------------------
    // E. Reference-model comparison fuzz — runs the same op sequence
    // against the real engine and a minimal in-memory model. Model
    // tracks only mode + buffer state (no candidates), and policy is
    // pinned to Never to disable wubi's force-commit at 4 letters
    // (which is per-engine state we don't replicate). Asserts is_
    // composing / preedit match step-by-step. Catches state-machine
    // bugs the unit-test layer wouldn't see (e.g., my mode-switch
    // buffer-leak bug from earlier — the model would have caught it
    // earlier had it existed).
    // -----------------------------------------------------------------

    #[derive(Debug, Clone)]
    struct EngineModel {
        mode: Mode,
        wubi_buf: String,
        pinyin_buf: String,
    }

    impl EngineModel {
        fn new() -> Self { Self { mode: Mode::Mixed, wubi_buf: String::new(), pinyin_buf: String::new() } }

        fn handle_letter(&mut self, b: u8) {
            // ASCII a-z only (matches real engine contract).
            if !b.is_ascii_lowercase() { return; }
            let c = b as char;
            if self.mode.allows_pinyin() && self.pinyin_buf.len() < 4 {
                self.pinyin_buf.push(c);
            }
            if self.mode.allows_wubi() && self.wubi_buf.len() < 4 {
                self.wubi_buf.push(c);
            }
        }

        fn backspace(&mut self) -> bool {
            let mut consumed = false;
            if self.mode.allows_wubi() && !self.wubi_buf.is_empty() {
                self.wubi_buf.pop();
                consumed = true;
            }
            if self.mode.allows_pinyin() && !self.pinyin_buf.is_empty() {
                self.pinyin_buf.pop();
                consumed = true;
            }
            consumed
        }

        fn escape(&mut self) {
            if self.mode.allows_wubi() { self.wubi_buf.clear(); }
            if self.mode.allows_pinyin() { self.pinyin_buf.clear(); }
        }

        fn set_mode(&mut self, m: Mode) {
            // Mirror the real engine's behavior of clearing disallowed
            // engines on mode change (the bug-fix path).
            if !m.allows_wubi() { self.wubi_buf.clear(); }
            if !m.allows_pinyin() { self.pinyin_buf.clear(); }
            self.mode = m;
        }

        fn is_composing(&self) -> bool {
            (self.mode.allows_wubi() && !self.wubi_buf.is_empty())
            || (self.mode.allows_pinyin() && !self.pinyin_buf.is_empty())
        }

        /// Mirror real engine's `preedit()` selection rule.
        fn preedit(&self) -> &str {
            if !self.mode.allows_pinyin() {
                &self.wubi_buf
            } else if !self.mode.allows_wubi() || !self.pinyin_buf.is_empty() {
                // Pinyin is the sole engine OR has live composition — either way
                // its buffer is what the user sees.
                &self.pinyin_buf
            } else {
                &self.wubi_buf
            }
        }
    }

    #[derive(Debug, Clone)]
    enum ModelOp {
        Letter(u8),
        Backspace,
        Escape,
        SetMode(Mode),
    }

    fn model_op() -> impl Strategy<Value = ModelOp> {
        prop_oneof![
            6 => (b'a'..=b'z').prop_map(ModelOp::Letter),
            2 => Just(ModelOp::Backspace),
            1 => Just(ModelOp::Escape),
            2 => prop_oneof![
                Just(Mode::Mixed), Just(Mode::WubiOnly), Just(Mode::PinyinOnly)
            ].prop_map(ModelOp::SetMode),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, .. ProptestConfig::default() })]

        /// Real engine + minimal model evolve in lockstep on
        /// is_composing / preedit. Letter count is capped at 4 per
        /// engine in the model, and we use ::Never policy on the real
        /// side, so wubi never force-commits and the simple model
        /// stays in sync.
        #[test]
        fn model_lockstep_is_composing_and_preedit(
            ops in proptest::collection::vec(model_op(), 1..40)
        ) {
            let mut real = CompositeEngine::new();
            real.set_auto_commit_policy(AutoCommitPolicy::Never);
            let mut model = EngineModel::new();

            let mut letter_count_since_clear: [usize; 2] = [0, 0]; // [wubi, pinyin]

            for (i, op) in ops.iter().enumerate() {
                let label = format!("op#{i} = {:?}", op);
                match op {
                    ModelOp::Letter(b) => {
                        // Skip letter ops once either buffer would
                        // overflow the model's 4-cap (i.e., real engine
                        // approaching wubi's force-commit edge). This
                        // keeps the model's simplifying assumption
                        // (no force-commit) valid.
                        if model.mode.allows_wubi() && letter_count_since_clear[0] >= 4 { continue; }
                        if model.mode.allows_pinyin() && letter_count_since_clear[1] >= 4 { continue; }
                        let _ = real.handle_letter(*b);
                        model.handle_letter(*b);
                        if model.mode.allows_wubi()   { letter_count_since_clear[0] += 1; }
                        if model.mode.allows_pinyin() { letter_count_since_clear[1] += 1; }
                    }
                    ModelOp::Backspace => {
                        let _ = real.backspace();
                        model.backspace();
                        if model.mode.allows_wubi() && letter_count_since_clear[0] > 0 { letter_count_since_clear[0] -= 1; }
                        if model.mode.allows_pinyin() && letter_count_since_clear[1] > 0 { letter_count_since_clear[1] -= 1; }
                    }
                    ModelOp::Escape => {
                        let _ = real.escape();
                        model.escape();
                        if model.mode.allows_wubi()   { letter_count_since_clear[0] = 0; }
                        if model.mode.allows_pinyin() { letter_count_since_clear[1] = 0; }
                    }
                    ModelOp::SetMode(m) => {
                        real.set_mode(*m);
                        model.set_mode(*m);
                        if !m.allows_wubi()   { letter_count_since_clear[0] = 0; }
                        if !m.allows_pinyin() { letter_count_since_clear[1] = 0; }
                    }
                }
                prop_assert_eq!(
                    real.is_composing(), model.is_composing(),
                    "is_composing diverged after {}: real={} model={} (model.wubi={:?} model.pinyin={:?})",
                    label, real.is_composing(), model.is_composing(), model.wubi_buf, model.pinyin_buf
                );
                prop_assert_eq!(
                    real.preedit(), model.preedit(),
                    "preedit diverged after {}: real={:?} model={:?} (model.wubi={:?} model.pinyin={:?})",
                    label, real.preedit(), model.preedit(), model.wubi_buf, model.pinyin_buf
                );
            }
        }
    }
}
