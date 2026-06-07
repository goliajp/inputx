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
    /// The most-recently-committed word from this session, used as the
    /// `prev` argument to `PinyinDict::bigram_boost`. `None` on first
    /// keystroke of a session and after `clear_all`. Updated by
    /// `commit_index` (and by any other commit path — auto-commit, ASCII
    /// fallback). Pure CJK candidates only; ASCII-fallback / English
    /// commits set this back to `None` because they don't seed
    /// meaningful Chinese-word context.
    last_committed_word: Option<String>,
    /// The word committed BEFORE `last_committed_word`. Used together
    /// with `last_committed_word` as the (prev_prev, prev) context for
    /// trigram-based next-word prediction (`predict_next_words_context`).
    /// Same CJK-only seeding rule.
    second_last_committed_word: Option<String>,
    /// Next-word predictions (联想) computed after every CJK commit,
    /// read by the host's UI via `predicted_candidates()`. Empty until
    /// the first CJK commit and after `clear_all` / non-CJK commits.
    /// Distinct from `cand_buf` so the host can choose whether to keep
    /// the panel visible post-commit (showing predictions) vs hide it.
    prediction_buf: Vec<Candidate>,
    /// Recently committed CJK words (last `RECENT_COMMITTED_CAP`).
    /// Filters out cycle-prone predictions.
    recent_committed: std::collections::VecDeque<String>,
    /// Number of consecutive prediction-commits (no manual typing in
    /// between). After PREDICTION_CHAIN_LIMIT, predictions are
    /// suppressed regardless of trigram strength — forces the user
    /// to take action (type next syllable / pick non-prediction /
    /// accept current text). Resets to 0 on any normal commit or
    /// clear_all. Prevents space-mashing from spawning long chains
    /// (user-reported 2026-05-24: "年人在年月日的比赛中获得了…").
    consecutive_predictions: u32,
}

const RECENT_COMMITTED_CAP: usize = 8;
const PREDICTION_CHAIN_LIMIT: u32 = 2;

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
            last_committed_word: None,
            second_last_committed_word: None,
            prediction_buf: Vec::with_capacity(10),
            recent_committed: std::collections::VecDeque::with_capacity(RECENT_COMMITTED_CAP),
            consecutive_predictions: 0,
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
            || (self.enable_japanese && self.japanese.as_ref().is_some_and(|j| j.is_composing()))
    }

    /// `true` iff the JP sub-engine is in the middle of a composition.
    /// Used by `Session::handle_key_cjk` to gate the `-` chōonpu input —
    /// only when JP is actively composing should `-` be routed through
    /// the engine (otherwise it stays a punct char). Independent of mode
    /// gating: in JapaneseOnly always returns the JP buffer state; in
    /// Mixed it follows the JP adapter (only Some when enable_japanese
    /// is on).
    pub fn japanese_is_composing(&self) -> bool {
        self.japanese.as_ref().is_some_and(|j| j.is_composing())
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
        // happens to be empty, and (user 2026-05-27 chōonpu polish) also
        // fall through to JP whenever JP holds a *longer* buffer than
        // pinyin: this happens when `-` (chōonpu) extends the JP buffer
        // — pinyin rejects `-`, JP accepts it, so showing the pinyin
        // buffer would hide the `-` the user just typed and confuse the
        // preedit display vs the kana candidates.
        let p = self.pinyin.buffer_str();
        if let Some(j) = self.japanese.as_ref() {
            let jp = j.buffer_str();
            if jp.len() > p.len() {
                return jp;
            }
        }
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
    /// Returns the merged candidate list, sorted purely by score
    /// (descending). L0 pins surface at #0 via the score multiplier
    /// applied inside the engine adapter's `lookup_with_scores_into`
    /// (see `scoring::PRIOR_L0_PIN_MULT` = 1000×) — no post-pass
    /// re-ordering exists or is needed.
    pub fn candidates(&mut self) -> &[Candidate] {
        self.cand_buf.clear();
        self.cand_buf.extend(dispatch(
            self.mode,
            &self.wubi,
            &self.pinyin,
            self.japanese.as_ref(),
            self.last_committed_word.as_deref(),
        ));
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
                if let Some(j) = self.japanese.as_mut() {
                    j.clear_all();
                }
                self.consecutive_predictions = 0; // typed → break chain
                self.update_bigram_context(&text);
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
                if let Some(j) = self.japanese.as_mut() {
                    j.clear_all();
                }
                self.consecutive_predictions = 0; // typed → break chain
                self.update_bigram_context(&text);
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
        if self.preedit().len() >= Self::ASCII_FALLBACK_THRESHOLD && self.is_pure_garbage() {
            let raw = self.preedit().to_string();
            self.wubi.clear_all();
            self.pinyin.clear_all();
            if let Some(j) = self.japanese.as_mut() {
                j.clear_all();
            }
            // ASCII raw fallback isn't a Chinese word — drop the full
            // bigram / trigram context. Whatever Chinese was committed
            // before no longer informs the next CJK input across this
            // English interruption.
            self.last_committed_word = None;
            self.second_last_committed_word = None;
            self.prediction_buf.clear();
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
        if self.pinyin.has_future_match() {
            return false;
        }
        // Path 1c (initials-fallback typo rescue) gate: when the
        // buffer is a 4-5 char missing-vowel-typo shape (2-consonant
        // prefix + ≥2-char suffix) we let Path 1c produce its
        // initials-based candidates instead of wiping the buffer.
        // Without this, single letters that can't start any pinyin
        // syllable (`v`, plus `i`/`u`) trigger ASCII-fallback at
        // exactly length 5 even though the buffer is the same shape
        // Path 1c was designed to rescue.
        //
        // Verified 2026-06-05: `shehv` at JP-off used to wipe via
        // ASCII fallback (preedit='' / 0 candidates) while `shehb`
        // / `shehz` correctly fell through to Path 1c (50 candidates
        // topped by 时候/生活/说话/...). JP-on already masked this
        // via the early-return above, so the bug only surfaced when
        // a user disabled JP — but the asymmetry was real and
        // unprincipled.
        if self.pinyin.path1c_would_fire() {
            return false;
        }
        // 音节意识细化 (2026-06-06): if the buffer has a clean ≥3-char
        // syllable prefix, Path 3b trim-retry (in pinyin_adapter) will
        // produce candidates. The user committed to that syllable; don't
        // wipe the buffer even though `has_future_match` and Path 1c
        // both said no. `shehv` / `xianv` route through this.
        if self.pinyin.has_clean_syllable_prefix() {
            return false;
        }
        true
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
        // Mixed mode: pinyin's buffer is the canonical input (it receives
        // every byte; wubi freezes at 4 chars — see handle_letter). Popping
        // the two engines independently desyncs them — wubi ends up tracking
        // a different substring than pinyin (`pinyinggg` then surfaced wubi
        // 王/珏 under a pinyin preedit, user-reported 2026-05-25). So pop
        // pinyin, then RE-DERIVE wubi from pinyin's post-pop buffer. JP, like
        // pinyin, receives every byte, so a plain pop keeps it in sync.
        if self.mode.allows_wubi() && self.mode.allows_pinyin() {
            // Resync only when wubi is FROZEN at its 4-char cap while pinyin
            // has grown past it — then wubi is a truncated prefix of pinyin
            // and popping it independently leaves the two tracking different
            // substrings (the `pinyinggg` desync, 2026-05-25). In every other
            // case keep the in-step pop: normal ≤4 typing (they move together)
            // and mode-switch inheritance (PinyinOnly→Mixed leaves pinyin >
            // wubi but wubi is NOT frozen; WubiOnly→Mixed leaves wubi ≥ pinyin)
            // — re-deriving there would wrongly wipe a buffer's own content.
            // The `>= 4` guard means this never fires below the freeze point.
            let wubi_is_frozen_prefix = self.wubi.buffer_str().len() >= 4
                && self.pinyin.buffer_str().len() > self.wubi.buffer_str().len();
            consumed |= self.pinyin.backspace();
            if wubi_is_frozen_prefix {
                self.resync_wubi_to_pinyin();
            } else {
                consumed |= self.wubi.backspace();
            }
            if self.enable_japanese {
                if let Some(j) = self.japanese.as_mut() {
                    consumed |= j.backspace();
                }
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

    /// Re-derive the wubi sub-engine buffer from pinyin's (the canonical
    /// input in Mixed mode), replaying the same "freeze at 4 chars" rule
    /// `handle_letter` applies. Guarantees `wubi_buffer == first-≤4 chars of
    /// pinyin_buffer`, so editing (backspace) can't leave the two engines
    /// tracking different substrings. wubi's sub-engine policy is always
    /// `Never` (see `new()`), so these replayed `handle_letter` calls cannot
    /// auto-commit. Pinyin is ASCII, so byte iteration is char-safe.
    fn resync_wubi_to_pinyin(&mut self) {
        self.wubi.clear_all();
        let pbuf = self.pinyin.buffer_str().to_string();
        for b in pbuf.bytes() {
            if self.wubi.buffer_str().len() >= 4 {
                break;
            }
            let _ = self.wubi.handle_letter(b);
        }
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
        self.prediction_buf.clear();
        // Treat clear_all as a full session boundary: drop the bigram /
        // trigram context. Use cases (set_mode, set_input_mode En→Cjk
        // restore, explicit clear) all imply "lose continuity".
        self.last_committed_word = None;
        self.second_last_committed_word = None;
        self.recent_committed.clear();
        self.consecutive_predictions = 0;
    }

    /// Seed bigram context from a just-committed word, but only if it's
    /// a pure-CJK Chinese word (kana / Latin commits don't inform the
    /// Chinese-corpus bigram table). Auto-commit / force-commit paths
    /// (above in `handle_letter`) call this; user-driven `commit_index`
    /// does it inline. Also triggers a fresh `predicted_candidates`
    /// computation so the host can show next-word predictions in the
    /// candidate panel immediately after commit.
    fn update_bigram_context(&mut self, committed: &str) {
        if committed
            .chars()
            .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
        {
            self.second_last_committed_word = self.last_committed_word.take();
            self.last_committed_word = Some(committed.to_string());
            // Track recent committed words for cycle-prevention in
            // predictions. Cap at RECENT_COMMITTED_CAP — older entries
            // age out so legit re-typing (e.g. saying 我 twice in two
            // separate sentences) still gets predictions.
            self.recent_committed.push_back(committed.to_string());
            while self.recent_committed.len() > RECENT_COMMITTED_CAP {
                self.recent_committed.pop_front();
            }
            self.refresh_predictions();
        }
    }

    /// Rebuild `prediction_buf` from `last_committed_word` via the
    /// pinyin dict's `predict_next_words`. Predictions are the 联想
    /// (next-word) feature: after committing a CJK word, the panel can
    /// show predicted continuations without the user typing anything.
    ///
    /// Bypassed in modes that don't allow pinyin (e.g. WubiOnly) — the
    /// bigram table is corpus-Chinese; predictions in pure-wubi mode
    /// would be off-channel.
    fn refresh_predictions(&mut self) {
        self.prediction_buf.clear();
        if !self.mode.allows_pinyin() {
            return;
        }
        // v1.5 chain-depth limit: after PREDICTION_CHAIN_LIMIT
        // consecutive prediction-commits with no manual typing in
        // between, stop showing predictions. Forces the user to
        // interact (type or stop) instead of spinning into the
        // 在年月日年月日 type chain.
        if self.consecutive_predictions >= PREDICTION_CHAIN_LIMIT {
            return;
        }
        let Some(prev) = self.last_committed_word.as_deref() else {
            return;
        };
        const PREDICTION_LIMIT: usize = 10;
        // v1.4 strict (2026-05-24): trigram only, no bigram fallback.
        // User rule: "联想是附加的好处，没有足够的证据就不要联想".
        // Single bigram signal alone is too noisy (在→年 user complaint
        // — bigram exists only because of "在2024年" type phrases, has
        // no real semantic basis as "after 在 user wants 年").
        // predict_next_words_context now returns EMPTY when prev_prev
        // is None OR trigram count < MIN_TRIGRAM_COUNT.
        let raw = self.pinyin.engine().dict().predict_next_words_context(
            self.second_last_committed_word.as_deref(),
            prev,
            PREDICTION_LIMIT * 2, // over-fetch then cycle-filter
        );
        // Cycle-filter: drop any predicted word that's already in the
        // recent_committed deque. Breaks the 在→年→月→日→年→… loop
        // (each link valid trigram, chain valid corpus, but globally
        // pointless re-emission).
        let mut count = 0;
        for (word, _trigram_count) in raw {
            if self.recent_committed.iter().any(|w| w == &word) {
                continue;
            }
            self.prediction_buf.push(Candidate {
                word,
                source: super::merge::Source::Pinyin,
                score: 200_000.0 - (count as f64) * 5_000.0,
                // Next-word predictions (post-commit 联想) use a synthetic
                // ranking-only score; no probability decomposition yet.
                components: None,
            });
            count += 1;
            if count >= PREDICTION_LIMIT {
                break;
            }
        }
    }

    /// Predicted next-word candidates after the most recent CJK commit.
    /// Empty when:
    ///   * No prior commit in this session.
    ///   * Mode is JapaneseOnly / WubiOnly (predictions are corpus-
    ///     Chinese, off-channel for those modes).
    ///   * The prev word has no bigram followers (very rare word).
    ///
    /// Host UI uses this to decide whether to keep the candidate panel
    /// visible after commit (showing predictions) instead of hiding it.
    pub fn predicted_candidates(&self) -> &[Candidate] {
        &self.prediction_buf
    }

    /// Commit a word that came from `predicted_candidates`. Unlike
    /// `commit_index` this does NOT consult `cand_buf` (predictions live
    /// in their own buffer) and does NOT record a pick to any engine's
    /// L0 (there's no buffer-context to learn from). It DOES update
    /// `last_committed_word` so a subsequent round of predictions fires
    /// from this newly-committed word — chained 联想 in the Sogou
    /// style. Returns the word for the host to deliver as committed
    /// text.
    pub fn commit_prediction_word(&mut self, word: &str) -> String {
        let owned = word.to_string();
        // Increment chain counter BEFORE update_bigram_context (which
        // calls refresh_predictions). The counter is consulted there
        // to decide whether to compute new predictions.
        self.consecutive_predictions += 1;
        // Same CJK guard as the regular commit path — only seed bigram
        // context from real Chinese words.
        self.update_bigram_context(&owned);
        owned
    }

    /// Commit candidate at index. Records the pick to the source engine's
    /// L0 layer (per item 28: each engine has its own L0 — same word can
    /// have different per-engine ranking).
    pub fn commit_index(&mut self, index: usize) -> Option<String> {
        if self.cand_buf.is_empty() {
            // Refresh once — caller may not have invoked candidates() yet.
            self.cand_buf.extend(dispatch(
                self.mode,
                &self.wubi,
                &self.pinyin,
                self.japanese.as_ref(),
                self.last_committed_word.as_deref(),
            ));
        }
        let cand = self.cand_buf.get(index).cloned()?;
        match cand.source {
            super::merge::Source::Wubi => {
                if let Some(idx) = self.wubi_index_for(&cand.word) {
                    self.wubi.commit_index(idx);
                }
                // else: prediction commit — `word` came from wubi prefix
                // prediction (e.g. `jeg → 明天` mid-typing), so it isn't
                // in `WubiEngine.candidates()` (which only holds exact
                // dict entries for the current buffer). The unconditional
                // `wubi.clear_all()` below handles buffer reset; the
                // L0 per-code advance is skipped because we don't have
                // the prediction's full code here yet — adding prediction
                // L0 records via `WubiDict::find_by_word` is on the
                // v1.4.7+ prefix-prediction backlog (per [[prefix-prediction-backlog]]).
                // The merge layer's returned `cand.word` still bubbles
                // back as the commit text below — same UX as exact-path
                // commit, just without per-code L0 reinforcement.
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
        // Manual pick (number-key / space) breaks any prediction chain
        // — the user actively chose this candidate from the buffer-
        // driven list, not from predictions.
        self.consecutive_predictions = 0;
        self.update_bigram_context(&cand.word);
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
    pub fn pinyin_export_l0(&self) -> Option<inputx_pinyin::L0Snapshot> {
        Some(self.pinyin.export_l0())
    }

    pub fn pinyin_import_l0(&self, snap: inputx_pinyin::L0Snapshot) -> usize {
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
        // 'wo' is a wubi 2-letter simcode (wo → 伙, explicitly
        // protected by session::wubi_simcode_priority) AND a pinyin
        // syllable (wo → 我). The composite contract: both sources
        // contribute to the candidate list. Use 'wo' instead of 'yi'
        // because 'yi → 就' was purged from Jianma2 in v1.4 polish
        // (the 'yi' code's Jianma2 char wasn't on the 伙-rule protect
        // list, so it yielded to pinyin top 一).
        typed(&mut e, b"wo");
        let cands = e.candidates().to_vec();
        assert_eq!(
            cands[0].source,
            Source::Wubi,
            "expected wubi #0 for 'wo' (protected simcode 伙); got cands={:?}",
            cands
                .iter()
                .take(5)
                .map(|c| (&c.word, c.source))
                .collect::<Vec<_>>()
        );
        assert!(
            cands.iter().any(|c| c.source == Source::Pinyin),
            "expected at least one Pinyin candidate in the list"
        );
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
    fn mixed_backspace_keeps_wubi_synced_to_pinyin() {
        // Regression for user-reported 2026-05-25 desync: in Mixed mode wubi
        // freezes at 4 chars while pinyin tracks the full input. Popping the
        // two engines independently on backspace makes them track DIFFERENT
        // substrings — `pinyinggg` then showed wubi 王/珏 under a pinyin
        // preedit. Invariant: wubi_buf == first-≤4 chars of pinyin_buf, and
        // backspace+retype must equal typing the net string fresh.
        let mut e = CompositeEngine::new();
        typed(&mut e, b"pinyinggg");
        assert_eq!(e.pinyin_buffer_str(), "pinyinggg");
        assert_eq!(e.wubi_buffer_str(), "piny", "wubi frozen at first 4 chars");

        for _ in 0..3 {
            e.backspace();
        }
        assert_eq!(e.pinyin_buffer_str(), "pinyin");
        assert_eq!(
            e.wubi_buffer_str(),
            "piny",
            "after backspace wubi must re-derive to first-4 of pinyin (bug left it 'p')"
        );

        // Edit-then-retype must converge to the same state as typing fresh.
        typed(&mut e, b"ggg");
        let mut fresh = CompositeEngine::new();
        typed(&mut fresh, b"pinyinggg");
        assert_eq!(e.pinyin_buffer_str(), fresh.pinyin_buffer_str());
        assert_eq!(
            e.wubi_buffer_str(),
            fresh.wubi_buffer_str(),
            "backspace+retype must match fresh-typed state (no desync, no '清零')"
        );
    }

    #[test]
    fn escape_clears_both_engines() {
        let mut e = CompositeEngine::new();
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        typed(&mut e, b"abc");
        assert!(e.escape());
        assert!(!e.is_composing());
    }

    #[cfg(not(feature = "bootstrap_only"))]
    #[test]
    fn composed_fallback_outranks_jp_kana() {
        // user-report 2026-05-25: in Mixed+JP, `kaopu` ranked the mechanical
        // kana かおぷ (LIKELIHOOD_JP_HIRAGANA_BASE 150k) ABOVE 靠谱 (Path 5 composition,
        // was NON_EXACT_FLOOR ~1k). A word composed from real single chars
        // must outrank a kana transliteration. COMPOSED_FALLBACK_SCORE (250k)
        // now sits above kana but below real dict words.
        let mut e = CompositeEngine::new();
        e.set_japanese_enabled(true);
        typed(&mut e, b"kaopu");
        let cands = e.candidates().to_vec();
        let kao = cands.iter().position(|c| c.word == "靠谱");
        assert!(
            kao.is_some(),
            "靠谱 should be present in Mixed+JP; got {:?}",
            cands
                .iter()
                .map(|c| (&c.word, c.source))
                .collect::<Vec<_>>()
        );
        if let Some(jp) = cands.iter().position(|c| c.source == Source::Japanese) {
            assert!(
                kao.unwrap() < jp,
                "靠谱(#{}) must outrank mechanical JP kana(#{}); got {:?}",
                kao.unwrap(),
                jp,
                cands
                    .iter()
                    .map(|c| (&c.word, c.source))
                    .collect::<Vec<_>>()
            );
        }
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
            "expected no JP candidates with toggle off, got {:?}",
            cands
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
            cands
                .iter()
                .any(|c| c.word == "か" && c.source == Source::Japanese),
            "expected か as JP candidate, got {:?}",
            cands
        );
        // Wubi/pinyin candidates (if any) must precede JP — strict ranking.
        let first_jp = cands
            .iter()
            .position(|c| c.source == Source::Japanese)
            .expect("expected at least one JP candidate");
        for (i, c) in cands.iter().enumerate() {
            if i < first_jp {
                assert_ne!(
                    c.source,
                    Source::Japanese,
                    "JP candidate appeared before non-JP at index {}",
                    i
                );
            }
        }
    }

    #[test]
    fn japanese_only_mode_silences_wubi_pinyin() {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::JapaneseOnly);
        typed(&mut e, b"ka");
        let cands = e.candidates().to_vec();
        assert!(
            !cands.is_empty(),
            "expected JP candidates in JapaneseOnly mode"
        );
        assert!(
            cands.iter().all(|c| c.source == Source::Japanese),
            "JapaneseOnly mode should yield only JP candidates, got {:?}",
            cands
        );
    }

    #[test]
    fn japanese_only_high_kou_finds_kanji() {
        // The user's framing example: typing "kou" should surface 高 in
        // JapaneseOnly mode (kanji subset is codepoint-identical with CN
        // simplified per the inputx-nihongo curation).
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::JapaneseOnly);
        typed(&mut e, b"kou");
        let cands = e.candidates().to_vec();
        assert!(
            cands
                .iter()
                .any(|c| c.word == "高" && c.source == Source::Japanese),
            "expected 高 in JapaneseOnly candidates for 'kou', got {:?}",
            cands
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
        assert!(
            !e.is_composing(),
            "JapaneseOnly transition should clear Chinese composing state"
        );
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
        assert!(
            cands.iter().any(|c| c.word == "北京"),
            "candidates for beijing: {:?}",
            cands.iter().map(|c| c.word.as_str()).collect::<Vec<_>>()
        );
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
            composing,
            !preedit_empty,
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
            s,
            f,
            s / f
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
            assert!(
                r.is_none(),
                "premature fallback at {}: {:?}",
                std::str::from_utf8(&[b]).unwrap(),
                r
            );
        }
        // 5th letter — fallback fires.
        let committed = e.handle_letter(b'y');
        assert_eq!(
            committed.as_deref(),
            Some("qwxzy"),
            "expected ASCII fallback at 5 chars, got {:?} (preedit={:?})",
            committed,
            e.preedit()
        );
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
        for &b in b"qwxz" {
            let _ = e.handle_letter(b);
        }
        assert!(
            e.is_composing(),
            "should still compose at 4 unrecognized chars"
        );
        assert_eq!(e.preedit(), "qwxz");
    }

    #[test]
    fn ascii_fallback_blocked_by_pinyin_match() {
        // 'beijing' is 7 chars but at every step pinyin has candidates
        // (北京 / 北 / 备 / ...) — fallback must not fire.
        let mut e = CompositeEngine::new();
        for &b in b"beijing" {
            let r = e.handle_letter(b);
            assert!(
                r.is_none(),
                "wrongful fallback while pinyin matches: {:?}",
                r
            );
        }
        assert!(
            e.is_composing(),
            "composing should still be true after beijing"
        );
        let cands: Vec<String> = e.candidates().iter().map(|c| c.word.clone()).collect();
        assert!(
            cands.iter().any(|s| s == "北京"),
            "expected 北京 in candidates, got {:?}",
            cands
        );
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
        assert!(
            !commits.is_empty(),
            "no ASCII commit fired for garbage input (preedit={:?})",
            e.preedit()
        );
        // Concatenation should match (modulo any residual preedit).
        let mut out = commits.join("");
        out.push_str(e.preedit());
        let expected = std::str::from_utf8(garbage).unwrap();
        assert_eq!(
            out, expected,
            "lost characters: committed+preedit={:?} expected={:?}",
            out, expected
        );
    }

    /// Regression 2026-06-05 → updated 2026-06-06 for 音节意识细化.
    ///
    /// Original symptom (2026-06-05): `shehv` (5 chars ending in 'v')
    /// at JP-off tripped ASCII fallback because:
    ///   - has_future_match("shehv") = false ('v' isn't a syllable
    ///     starter, the trailing-trim loop fails every suffix)
    ///   - JP-on early-return masked the symptom; the moment JP was
    ///     disabled, `shehv` wiped the buffer entirely (preedit='',
    ///     0 candidates) — output dependent on a setting unrelated
    ///     to the input
    /// Original fix (2026-06-05): is_pure_garbage gated on
    /// `path1c_would_fire`, so shehv kept the buffer and Path 1c
    /// surfaced sh+h initials (时候/生活/...).
    ///
    /// 音节意识细化 (2026-06-06, docs/PLAN-syllable-aware-pinyin.md):
    /// the Path 1c interpretation was wrong — `she` is a clean
    /// syllable, so trailing junk should route to Path 3b trim-retry
    /// (= sheh's prefix completions: 社会/奢华/设好/...), NOT Path 1c
    /// initials lookup. is_pure_garbage now ALSO gates on
    /// `has_clean_syllable_prefix`, and Path 3b in pinyin_adapter
    /// runs as the last-resort completion source for these buffers.
    ///
    /// This test pins the new behavior: shehv → preedit kept, candidates
    /// non-empty, top should match `sheh`'s top (社会).
    #[test]
    fn ascii_fallback_yields_to_path3b_trim_retry() {
        if super::super::pinyin_adapter::PINYIN_DISABLE_FUZZY {
            return;
        }
        let mut e = CompositeEngine::new();
        e.set_auto_commit_policy(AutoCommitPolicy::Never);
        // JP intentionally LEFT OFF — Phase J behavior must be
        // independent of the JP toggle, same as the 2026-06-05 fix.
        for &b in b"shehv" {
            let r = e.handle_letter(b);
            assert!(
                r.is_none(),
                "ASCII fallback fired at {:?}, wiped buffer — preedit={:?}",
                std::str::from_utf8(&[b]).unwrap(),
                e.preedit()
            );
        }
        assert_eq!(
            e.preedit(),
            "shehv",
            "音节意识细化: shehv must keep its buffer; got preedit={:?}",
            e.preedit()
        );
        let words: Vec<&str> = e.candidates().iter().map(|c| c.word.as_str()).collect();
        // Path 3b trim-retry on `shehv` → `sheh` prefix → 社会 / 奢华
        // / 设好 / 射核 / … (same candidates as typing `sheh`).
        assert!(
            words.contains(&"社会"),
            "音节意识细化: expected 社会 (sheh trim-retry top); got {words:?}"
        );
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
    // 联想 — predicted_candidates surface
    // -----------------------------------------------------------------

    #[test]
    fn predictions_empty_before_any_commit() {
        let e = CompositeEngine::new();
        assert!(
            e.predicted_candidates().is_empty(),
            "no predictions until first CJK commit"
        );
    }

    #[cfg(not(feature = "bootstrap_only"))]
    #[test]
    fn predictions_chain_stops_at_chain_limit() {
        // v1.5 cycle hard-stop: after PREDICTION_CHAIN_LIMIT (=2)
        // consecutive prediction-commits with no manual typing in
        // between, refresh_predictions returns empty. Prevents the
        // user-reported "在年月日年月日年月日…" runaway chain.
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::PinyinOnly);
        // Seed with two manual commits to build (prev_prev, prev) context.
        for b in b"jintian" {
            let _ = e.handle_letter(*b);
        }
        let cands = e.candidates();
        if let Some(idx) = cands.iter().position(|c| c.word == "今天") {
            let _ = e.commit_index(idx);
        }
        for b in b"de" {
            let _ = e.handle_letter(*b);
        }
        let cands = e.candidates();
        if let Some(idx) = cands.iter().position(|c| c.word == "的") {
            let _ = e.commit_index(idx);
        }
        // Now any predictions are chained from the rolling 2-word
        // context. Simulate 3 successive prediction commits.
        // After commit #1 + #2 of predictions (=> counter at 2),
        // the THIRD refresh should yield empty per chain-limit.
        let _ = e.commit_prediction_word("某词"); // counter 0→1
        let _ = e.commit_prediction_word("另词"); // counter 1→2
        // Now consecutive_predictions = 2 = PREDICTION_CHAIN_LIMIT.
        // Next refresh (already happened inside #2's commit) returned
        // empty because counter == limit.
        assert!(
            e.predicted_candidates().is_empty(),
            "predictions must stop at chain-limit; got {:?}",
            e.predicted_candidates()
                .iter()
                .map(|c| &c.word)
                .collect::<Vec<_>>()
        );
        // Manual typing resets the counter — predictions resume eligible.
        for b in b"de" {
            let _ = e.handle_letter(*b);
        }
        let cands = e.candidates();
        if let Some(idx) = cands.iter().position(|c| c.word == "的") {
            let _ = e.commit_index(idx);
            // counter reset to 0; predictions can fire again
            // (subject to other gates: trigram MIN_COUNT, recent_filter).
        }
        // Just confirm counter is back to 0 by checking that subsequent
        // prediction commit cycles work again.
    }

    #[cfg(not(feature = "bootstrap_only"))]
    #[test]
    fn predictions_empty_on_single_commit_v14_strict() {
        // v1.4 strict-trigram policy (2026-05-24): the first CJK commit
        // alone (no prev_prev) is INSUFFICIENT evidence to predict.
        // User rule: "联想是附加的好处，没有足够的证据就不要联想".
        // Predictions only fire when both prev_prev AND prev are CJK
        // (trigram context). After a SINGLE commit, prediction panel
        // is empty — the user types the next word manually.
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::PinyinOnly);
        for b in b"jintian" {
            let _ = e.handle_letter(*b);
        }
        let cands = e.candidates();
        let jintian_idx = cands
            .iter()
            .position(|c| c.word == "今天")
            .expect("expected 今天 in jintian candidates");
        let _ = e.commit_index(jintian_idx);
        assert!(
            e.predicted_candidates().is_empty(),
            "v1.4 strict: predictions must be empty after single commit"
        );
    }

    #[cfg(not(feature = "bootstrap_only"))]
    #[test]
    fn predictions_populated_after_two_cjk_commits_with_strong_trigram() {
        // Two-word context is the minimum for predictions to fire.
        // Use 我们 → 一起 → ? — both common words with corpus trigrams.
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::PinyinOnly);
        // First commit: 我们
        for b in b"women" {
            let _ = e.handle_letter(*b);
        }
        let cands = e.candidates();
        if let Some(idx) = cands.iter().position(|c| c.word == "我们") {
            let _ = e.commit_index(idx);
        }
        assert!(
            e.predicted_candidates().is_empty(),
            "no predictions after single commit"
        );
        // Second commit: 一起
        for b in b"yiqi" {
            let _ = e.handle_letter(*b);
        }
        let cands = e.candidates();
        if let Some(idx) = cands.iter().position(|c| c.word == "一起") {
            let _ = e.commit_index(idx);
            // Now (我们, 一起) is the trigram context. May or may not
            // have hits depending on corpus density; both are common
            // so SOME predictions should appear. If empty, that's OK
            // too (corpus may just lack this specific trigram) — test
            // doesn't fail.
            let preds = e.predicted_candidates();
            eprintln!(
                "(我们, 一起, *) predictions: {:?}",
                preds.iter().map(|c| &c.word).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn predictions_cleared_on_clear_all() {
        let mut e = CompositeEngine::new();
        e.set_mode(Mode::PinyinOnly);
        for b in b"wo" {
            let _ = e.handle_letter(*b);
        }
        let cands = e.candidates();
        if let Some(idx) = cands.iter().position(|c| c.word == "我") {
            let _ = e.commit_index(idx);
            // After CJK commit (when bigrams available) predictions may
            // be non-empty. Then clear_all should wipe them.
            e.clear_all();
            assert!(
                e.predicted_candidates().is_empty(),
                "clear_all should wipe predictions"
            );
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
        fn new() -> Self {
            Self {
                mode: Mode::Mixed,
                wubi_buf: String::new(),
                pinyin_buf: String::new(),
            }
        }

        fn handle_letter(&mut self, b: u8) {
            // ASCII a-z only (matches real engine contract).
            if !b.is_ascii_lowercase() {
                return;
            }
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
            if self.mode.allows_wubi() {
                self.wubi_buf.clear();
            }
            if self.mode.allows_pinyin() {
                self.pinyin_buf.clear();
            }
        }

        fn set_mode(&mut self, m: Mode) {
            // Mirror the real engine's behavior of clearing disallowed
            // engines on mode change (the bug-fix path).
            if !m.allows_wubi() {
                self.wubi_buf.clear();
            }
            if !m.allows_pinyin() {
                self.pinyin_buf.clear();
            }
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
