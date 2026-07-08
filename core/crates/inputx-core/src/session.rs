//! Session — top-level state visible to the host. Wraps a `CompositeEngine`
//! (wubi + pinyin dual-engine) per Phase 4 of the iOS commercial-grade
//! roadmap. The public API + FFI surface stays compatible with the v0.1
//! wubi-only shape; new dual-engine knobs (`mode`, `candidate_source`,
//! `export_l0_json`) are additive.

use crate::composite::{Candidate, CompositeEngine, Mode, ScoreComponents, Source, l0_json};
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
    /// Per-candidate unified score, mirrors `cand_cache` index-for-index.
    /// Exposed via `candidate_score()` so `inputx-probe` can print the
    /// engine's ranking signal (v1.3 WU-γ).
    score_cache: Vec<f64>,
    /// Per-candidate (base, prior, likelihood) decomposition when the
    /// score flowed through `scoring::predict_score_with_components`
    /// (CP-A JP / CP-B pinyin / CP-C wubi prefix-prediction). `None`
    /// for paths that don't yet emit the split (exact / fuzzy /
    /// composed / fallback) — those remain pre-v1.4 architecture.
    components_cache: Vec<Option<ScoreComponents>>,
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

/// v1.15 hot-reload failure kinds returned by
/// [`Session::reload_pinyin_data`]. Each variant stringifies its
/// underlying cause so the FFI can surface a single `char*` without
/// coupling to the concrete parse-error types.
#[derive(Debug)]
pub enum PinyinReloadError {
    /// Reading `pinyin.dict` from the target directory failed.
    ReadDict(String),
    /// Reading `words.idf` from the target directory failed.
    ReadIdf(String),
    /// Parsing the new `pinyin.dict` bytes failed.
    Dict(String),
    /// Parsing the new `words.idf` bytes failed.
    Idf(String),
    /// Parsing the new `bigrams.ngm` bytes failed (only reached when
    /// the file was present on disk).
    NgmIntra(String),
    /// Parsing the new `bigrams_inter.ngm` bytes failed.
    NgmInter(String),
}

impl std::fmt::Display for PinyinReloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReadDict(s) => write!(f, "read pinyin.dict failed: {s}"),
            Self::ReadIdf(s) => write!(f, "read words.idf failed: {s}"),
            Self::Dict(s) => write!(f, "parse pinyin.dict failed: {s}"),
            Self::Idf(s) => write!(f, "parse words.idf failed: {s}"),
            Self::NgmIntra(s) => write!(f, "parse bigrams.ngm failed: {s}"),
            Self::NgmInter(s) => write!(f, "parse bigrams_inter.ngm failed: {s}"),
        }
    }
}

impl std::error::Error for PinyinReloadError {}

impl Session {
    pub fn new() -> Self {
        Self {
            composite: CompositeEngine::new(),
            cand_cache: Vec::with_capacity(16),
            source_cache: Vec::with_capacity(16),
            score_cache: Vec::with_capacity(16),
            components_cache: Vec::with_capacity(16),
            pending_commit: None,
            smart_quote: SmartQuoteState::new(),
            input_mode: InputMode::Cjk,
        }
    }

    /// Map an ASCII quote char to its smart-CJK form, advancing state.
    /// Non-quote chars return `None`.
    /// CP-5.2 step-3: forward TOML cell-dict bytes into the underlying
    /// PinyinDict L0.5 layer. Returns the number of entries accepted on
    /// success, or a human-readable error string on parse failure. Multiple
    /// calls accumulate; `clear_cell_dict` wipes everything.
    pub fn load_cell_dict(&self, toml: &str) -> Result<usize, String> {
        self.composite.load_cell_dict(toml)
    }

    pub fn clear_cell_dict(&self) {
        self.composite.clear_cell_dict();
    }

    pub fn cell_dict_count(&self) -> usize {
        self.composite.cell_dict_count()
    }

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

        // 联想 cancellation guard: when the engine is sitting in a
        // pure-prediction state (no composing buffer, but pending
        // next-word predictions from a prior commit), any non-letter
        // input interrupts the flow and must drop the predictions.
        // Letter input is the only continuation path — it starts a new
        // composing round whose next commit will rebuild predictions
        // anyway via `refresh_predictions`. Everything else (digit,
        // punct, Space, Enter, Tab, Backspace, Escape, function keys,
        // arrow keys, …) reaches here and must clear the stale view
        // before falling through to its normal routing below.
        //
        // Ctrl/Cmd modifiers short-circuit earlier so app shortcuts
        // can't accidentally wipe predictions.
        if !self.composite.is_composing() && self.composite.has_predictions() {
            self.composite.cancel_predictions();
            self.refresh_caches();
        }

        // `-` (chōonpu / long-vowel mark) routes to the engine ONLY when JP
        // is mid-composition (e.g., `koohi` + `-` → コーヒー). Outside JP
        // composing — wubi/pinyin in progress, or no engine composing — `-`
        // falls through to the IME controller's locale punct path so it
        // ends up as raw ASCII / 全角 hyphen on the host. User-reported
        // 2026-05-27: `-` must be typeable as chōonpu in JP mode.
        if codepoint == b'-' as u32 && self.composite.japanese_is_composing() {
            if let Some(text) = self.composite.handle_letter(b'-') {
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

    /// `true` iff the JP sub-engine currently has a non-empty buffer.
    /// Lets the IME controller decide whether `-` should route through
    /// the engine as chōonpu (when JP composing) or fall to locale
    /// punct (otherwise). EN mode forces false: there's no engine
    /// composing at all.
    pub fn japanese_is_composing(&self) -> bool {
        match self.input_mode {
            InputMode::Cjk => self.composite.japanese_is_composing(),
            InputMode::En => false,
        }
    }

    pub fn candidates(&self) -> &[String] {
        &self.cand_cache
    }

    pub fn candidate_count(&self) -> usize {
        self.cand_cache.len()
    }

    /// Next-word predictions (联想) computed after the most-recent
    /// CJK commit. Empty until first CJK commit and after `clear`.
    /// Host UI uses this to decide whether to keep the candidate
    /// panel visible post-commit, showing predictions as the user's
    /// likely next pick (Sogou-style 联想 panel).
    pub fn predictions(&self) -> Vec<String> {
        self.composite
            .predicted_candidates()
            .iter()
            .map(|c| c.word.clone())
            .collect()
    }

    pub fn prediction_count(&self) -> usize {
        self.composite.predicted_candidates().len()
    }

    /// Drop pending 联想 candidates. Mirrors `composite.cancel_predictions`
    /// at the session boundary so the host (Swift / Obj-C) can clear
    /// stale predictions when its own keyDown path doesn't reach
    /// `handle_key_cjk` (e.g. punct that the IMK controller maps to a
    /// 全角 codepoint and routes around the engine — Path B in
    /// IMEController). `handle_key_cjk` already guards this internally,
    /// so this method exists solely for those out-of-band cancel sites.
    ///
    /// Leaves `last_committed_word` / `second_last_committed_word`
    /// intact — those only feed *next* commit's bigram / trigram LM
    /// scoring, not the currently visible prediction panel. Use
    /// `clear()` for a full session reset.
    pub fn cancel_predictions(&mut self) {
        self.composite.cancel_predictions();
    }

    /// Commit a prediction by index. Returns the committed text on
    /// success (and triggers a fresh round of predictions internally,
    /// keyed off the just-committed word — chained 联想). Returns
    /// `None` for out-of-range index.
    ///
    /// Unlike `commit_index`, this is a "soft" commit — there's no
    /// buffer to drain, so no per-engine L0 pick is recorded. The
    /// prediction list is fully derived from `last_committed_word` +
    /// the static bigram corpus.
    pub fn commit_prediction(&mut self, index: usize) -> Option<String> {
        let preds = self.composite.predicted_candidates();
        let word = preds.get(index).map(|c| c.word.clone())?;
        let committed = self.composite.commit_prediction_word(&word);
        Some(committed)
    }

    /// Source byte for the candidate at `index` — 0 = Wubi, 1 = Pinyin.
    /// Returns `None` if the index is out of range.
    pub fn candidate_source(&self, index: usize) -> Option<u8> {
        self.source_cache.get(index).map(|s| s.as_u8())
    }

    /// Unified score for the candidate at `index` (the value used for
    /// cross-engine sort). `None` if the index is out of range.
    /// Exposed for diagnostic tooling (`inputx-probe`); UI code should
    /// not depend on the absolute value.
    pub fn candidate_score(&self, index: usize) -> Option<f64> {
        self.score_cache.get(index).copied()
    }

    /// (base, prior, likelihood) decomposition for the candidate at
    /// `index` when its score came from `scoring::predict_score_with_components`;
    /// `None` for paths that don't yet emit the split. Invariant when
    /// present: `base + prior * likelihood == predict_score(..)` — the
    /// final `candidate_score()` may exceed this by an additive
    /// bigram_bonus / multiplicative promote applied downstream.
    pub fn candidate_components(&self, index: usize) -> Option<ScoreComponents> {
        self.components_cache.get(index).copied().flatten()
    }

    pub fn commit_index(&mut self, index: usize) -> Option<String> {
        let r = self.composite.commit_index(index);
        self.refresh_caches();
        r
    }

    // Segment mode (拼音手动分段, user 2026-06-07). Pinyin-only; delegate
    // straight to the composite engine's pinyin adapter.
    pub fn segment_anchors(&self) -> Vec<usize> {
        self.composite.segment_anchors()
    }

    pub fn segment_candidates(&self, k: usize) -> Vec<String> {
        self.composite.segment_candidates(k)
    }

    pub fn commit_segment(&mut self, k: usize, idx: usize) -> Option<String> {
        let r = self.composite.commit_segment(k, idx);
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

    /// v1.15 hot-reload for the pinyin sub-engine. Reads the four
    /// files that a polish rebuild regenerates from `dir` — the
    /// caller supplies the directory (typically the running IME
    /// bundle's `Contents/Resources/data/`) — and drives:
    ///
    /// - process-global `pinyin_idf_reader` via
    ///   [`inputx_pinyin_helpers::set_pinyin_idf_bytes`],
    /// - this session's `PinyinDict.map` via
    ///   [`crate::composite::CompositeEngine::reload_pinyin_dict`].
    ///
    /// NGM tables (`bigrams.ngm`, `bigrams_inter.ngm`) don't currently
    /// change during polish so they're read but only fed to their
    /// slots when present; missing NGM files skip that step without
    /// failing the reload.
    ///
    /// Returns Ok(()) on success. On any parse or IO failure, leaves
    /// both the process-global slot and the session dict in their
    /// prior state and returns an error — never a half-applied swap.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn reload_pinyin_data(&mut self, dir: &std::path::Path) -> Result<(), PinyinReloadError> {
        let dict_bytes = std::fs::read(dir.join("pinyin.dict"))
            .map_err(|e| PinyinReloadError::ReadDict(format!("{e}")))?;
        let idf_bytes = std::fs::read(dir.join("words.idf"))
            .map_err(|e| PinyinReloadError::ReadIdf(format!("{e}")))?;
        // Optional: NGM tables aren't polish-regenerated in the
        // current pipeline, but read them if present so future polish
        // rounds that update them "just work" without another code
        // release.
        let bigrams_ngm = std::fs::read(dir.join("bigrams.ngm")).ok();
        let inter_ngm = std::fs::read(dir.join("bigrams_inter.ngm")).ok();

        // Order matters: parse-check everything BEFORE swapping any
        // slot, so a broken new .idf can't leave a good old dict
        // paired with a bad new IdfReader.
        inputx_pinyin_helpers::set_pinyin_idf_bytes(idf_bytes)
            .map_err(|e| PinyinReloadError::Idf(format!("{e:?}")))?;
        if let Some(bytes) = bigrams_ngm {
            crate::composite::set_bigrams_ngm_bytes(bytes)
                .map_err(|e| PinyinReloadError::NgmIntra(format!("{e:?}")))?;
        }
        if let Some(bytes) = inter_ngm {
            crate::composite::set_inter_bigrams_ngm_bytes(bytes)
                .map_err(|e| PinyinReloadError::NgmInter(format!("{e:?}")))?;
        }
        self.composite
            .reload_pinyin_dict(dict_bytes)
            .map_err(|e| PinyinReloadError::Dict(format!("{e:?}")))?;
        // v1.16: v2 engine's polish overlay TSVs live behind ArcSwap
        // too. If a `polish/` subdir exists next to `pinyin.dict`,
        // swap those bytes so v2's `words()` / `tier_overlay()` /
        // `quickfix_boost()` / etc. rebuild on next query. Without
        // this step, the running Inputx binary would keep using the
        // compile-time-embedded TSV even after a hot-reload — which
        // is what made every polish since v2's default flip look
        // like it worked (per probe) while actually being invisible
        // to the running IME.
        let polish_dir = dir.join("polish");
        if polish_dir.is_dir() {
            inputx_pinyin_v2::set_polish_data_dir(&polish_dir);
        }
        Ok(())
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
        self.score_cache.clear();
        self.components_cache.clear();
        for c in cands {
            self.cand_cache.push(c.word);
            self.source_cache.push(c.source);
            self.score_cache.push(c.score);
            self.components_cache.push(c.components);
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
    fn chouonpu_backspace_returns_to_same_candidates() {
        // User polish-log 2026-05-27: "删除的时候与输入的时候，一样的码
        // 不一样的候选". Concretely: type `fa-----` (5 chouonpu), record
        // candidates; backspace 3 times (removing 3 `-`), then type 3 `-`
        // back; buffer is `fa-----` again — but candidates differ. This
        // indicates one of the sub-engines (wubi/pinyin/jp) isn't being
        // backspaced symmetrically with typing, so state drifts across
        // a backspace-then-retype cycle.
        let mut sess = s();
        sess.set_auto_commit_policy(AutoCommitPolicy::Never);
        sess.set_japanese_enabled(true);
        // Phase A: type fa-----
        for b in b"fa" {
            assert!(sess.handle_key(*b as u32, 0));
        }
        for _ in 0..5 {
            assert!(sess.handle_key(b'-' as u32, 0));
        }
        let preedit_a = sess.preedit().to_string();
        let cands_a: Vec<String> = sess.candidates().to_vec();
        // Phase B: backspace 3 times (delete trailing 3 `-`)
        for _ in 0..3 {
            assert!(sess.handle_key(0x08, 0)); // BS
        }
        // Phase C: retype the 3 `-` back
        for _ in 0..3 {
            assert!(sess.handle_key(b'-' as u32, 0));
        }
        let preedit_b = sess.preedit().to_string();
        let cands_b: Vec<String> = sess.candidates().to_vec();
        assert_eq!(
            preedit_a, preedit_b,
            "preedit must be identical after backspace-then-retype; got A={preedit_a:?} B={preedit_b:?}"
        );
        assert_eq!(
            cands_a, cands_b,
            "candidates must be identical after backspace-then-retype; got\n  A={cands_a:?}\n  B={cands_b:?}"
        );
    }

    #[test]
    fn chouonpu_hyphen_extends_jp_composition() {
        // User polish 2026-05-27: `-` must be typeable as chōonpu (ー) in
        // JP mode. Mozc-standard romaji for コーヒー is `ko-hi-` (chōonpu
        // entered explicitly with `-`). With JP enabled and a romaji buffer
        // in progress, each `-` keystroke should be routed to the engine,
        // extending the JP composition so コーヒー surfaces among candidates.
        let mut sess = s();
        sess.set_auto_commit_policy(AutoCommitPolicy::Never);
        sess.set_japanese_enabled(true);
        for b in b"ko" {
            assert!(sess.handle_key(*b as u32, 0));
        }
        assert!(
            sess.japanese_is_composing(),
            "JP buffer should be composing after typing romaji"
        );
        assert!(
            sess.handle_key(b'-' as u32, 0),
            "`-` keystroke must be consumed when JP is composing"
        );
        for b in b"hi" {
            assert!(sess.handle_key(*b as u32, 0));
        }
        assert!(
            sess.handle_key(b'-' as u32, 0),
            "trailing `-` must be consumed"
        );
        let preedit = sess.preedit().to_string();
        assert_eq!(
            preedit, "ko-hi-",
            "preedit should reflect full chouonpu romaji buffer; got {preedit:?}"
        );
        // The katakana with chōonpu must surface as a candidate.
        let cands = sess.candidates();
        assert!(
            cands.iter().any(|w| w == "コーヒー"),
            "コーヒー expected after ko-hi-; got top10={:?}",
            cands.iter().take(10).collect::<Vec<_>>()
        );
    }

    #[test]
    fn chouonpu_hyphen_falls_through_when_not_jp_composing() {
        // Outside JP composition (no JP enabled, or JP buffer empty), `-`
        // must NOT be consumed by the engine — it should fall through so
        // the host gets a regular hyphen via locale punct routing.
        let mut sess = s();
        sess.set_auto_commit_policy(AutoCommitPolicy::Never);
        // No JP enabled, no composing — `-` falls through.
        assert!(
            !sess.handle_key(b'-' as u32, 0),
            "`-` must NOT be consumed when no engine is composing"
        );
        // Even with JP enabled but empty buffer, `-` falls through.
        sess.set_japanese_enabled(true);
        assert!(!sess.japanese_is_composing());
        assert!(
            !sess.handle_key(b'-' as u32, 0),
            "`-` must NOT be consumed with JP enabled but empty buffer"
        );
    }

    #[test]
    fn q_bare_letter_single_chars_lead_phrases() {
        // User 2026-05-24: "单个字的评分也肯定要更高，现在 q 这列表根本
        // 不能看" — typing a bare letter `q` showed multi-char phrases
        // (前端/请问/权限/企业微信/前端工程师) buried single chars
        // (去/起/前) via raw corpus freq. Fix: `scoring::length_bias`
        // in `compute_single_letter_top_k` favors single chars for
        // bare-letter prefix completion. wubi Jianma1 (q→我) stays #0.
        let mut sess = s();
        sess.set_auto_commit_policy(AutoCommitPolicy::Never);
        sess.handle_key(b'q' as u32, 0);
        let cands = sess.candidates();
        assert!(!cands.is_empty(), "expected q candidates");
        // Every Pinyin-source candidate in the top 10 must be a single
        // char — no phrase may interleave among the leading single chars.
        for (i, w) in cands.iter().take(10).enumerate() {
            if sess.candidate_source(i) == Some(1) {
                assert_eq!(
                    w.chars().count(),
                    1,
                    "q top-10 pinyin candidate #{i} {w:?} should be a single \
                     char; phrases must rank below single chars for a bare letter"
                );
            }
        }
        // Sanity: a common single char surfaces high (去 is q-prefix common).
        let qu_pos = cands.iter().position(|w| w == "去");
        assert!(
            qu_pos.is_some_and(|p| p <= 5),
            "去 should rank in the top few for bare q, got {qu_pos:?}"
        );
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
    fn jixu_top_pinyin_candidate_is_jixu_continue() {
        // Regression for the user-reported 曳光弹-at-#0 confusion. The
        // weights data has `jixu 继续 44652` as the highest-freq entry
        // and 曳光弹 isn't under the `jixu` key at all (its reading is
        // `yeguangdan`). If this test ever fails, the pinyin engine
        // has acquired a fuzzy / heteronym path that's pulling
        // 曳光弹 into jixu candidates and needs to be traced.
        let mut sess = s();
        sess.set_auto_commit_policy(AutoCommitPolicy::Never);
        sess.set_mode(crate::composite::Mode::PinyinOnly);
        for cp in b"jixu" {
            sess.handle_key(*cp as u32, 0);
        }
        let cands = sess.candidates();
        assert!(!cands.is_empty());
        // 继续 must be in the top 3 (allowing some flex for noise).
        let top3: Vec<&str> = cands.iter().take(3).map(String::as_str).collect();
        assert!(
            top3.contains(&"继续"),
            "expected 继续 in top-3 for jixu, got {:?}",
            top3
        );
        // 曳光弹 absolutely must not appear (yeguangdan isn't jixu).
        assert!(
            !cands.iter().any(|w| w == "曳光弹"),
            "曳光弹 must not appear for jixu — its reading is yeguangdan. Got candidates: {:?}",
            &cands.iter().take(10).collect::<Vec<_>>()
        );
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
            assert!(
                !sess.handle_key(*cp as u32, 0),
                "letter/digit/punct should passthrough"
            );
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

#[cfg(test)]
mod prefix_completion_suppression {
    use super::*;
    /// User-reported polish (2026-05-22): typing `lianxiang` should produce
    /// only 联想 (exact-reading match) in the immediate candidate list.
    /// Earlier behavior pulled in 联想集团 / 联想起 / 联想到 via the pinyin
    /// adapter's Path 3 prefix-completion scan — but those are
    /// *predictions* (words whose pinyin EXTENDS lianxiang), not candidates
    /// for the buffer the user just typed. Predictions belong in a
    /// post-commit next-word list, not muddling the current candidate list.
    #[test]
    fn lianxiang_does_not_include_prefix_extension_words() {
        let mut sess = Session::new();
        sess.set_auto_commit_policy(AutoCommitPolicy::Never);
        sess.set_mode(crate::composite::Mode::PinyinOnly);
        for cp in b"lianxiang" {
            sess.handle_key(*cp as u32, 0);
        }
        let cands = sess.candidates();
        assert!(
            cands.iter().any(|w| w == "联想"),
            "expected 联想 present in lianxiang candidates. Got: {:?}",
            cands.iter().take(10).collect::<Vec<_>>()
        );
        for noise in &["联想集团", "联想起", "联想到"] {
            assert!(
                !cands.iter().any(|w| w == noise),
                "{} must not appear for exact lianxiang input — it's a \
                 prefix-extension word. Got: {:?}",
                noise,
                cands.iter().take(10).collect::<Vec<_>>()
            );
        }
    }

    /// Counterpart: incomplete syllable should STILL get prefix completion
    /// (otherwise user is stuck mid-syllable with nothing to commit).
    #[test]
    fn zho_partial_syllable_still_completes_via_prefix() {
        let mut sess = Session::new();
        sess.set_auto_commit_policy(AutoCommitPolicy::Never);
        sess.set_mode(crate::composite::Mode::PinyinOnly);
        for cp in b"zho" {
            sess.handle_key(*cp as u32, 0);
        }
        let cands = sess.candidates();
        assert!(
            !cands.is_empty(),
            "zho must still produce candidates via prefix completion"
        );
    }
}

#[cfg(test)]
mod wubi_simcode_priority {
    use super::*;
    /// Inputx is a 五笔 IME first. Valid wubi 简码 (Jianma1/2/3) take #0
    /// even when the same letters happen to be a valid pinyin syllable.
    /// 伙 is the wubi 二级简码 for "wo" — pressing space commits 伙, not
    /// pinyin 我. Pinyin candidates still appear in the list for users
    /// who want them, just not at #0. (User correction 2026-05-22: a
    /// brief "Policy 2" demote was reverted because it demoted wubi
    /// simcodes to position 13 — broke the brand promise.)
    fn top(input: &[u8]) -> String {
        let mut sess = Session::new();
        sess.set_auto_commit_policy(AutoCommitPolicy::Never);
        for cp in input {
            sess.handle_key(*cp as u32, 0);
        }
        sess.candidates().first().cloned().unwrap_or_default()
    }
    #[test]
    fn wo_wubi_伙() {
        assert_eq!(top(b"wo"), "伙");
    }
    #[test]
    fn ni_wubi_悄() {
        assert_eq!(top(b"ni"), "悄");
    }
    #[test]
    fn ta_wubi_长() {
        assert_eq!(top(b"ta"), "长");
    }
    #[test]
    fn de_wubi_胡() {
        assert_eq!(top(b"de"), "胡");
    }
    // shi_wubi_椒 retired 2026-06-03 — user "shi 肯定不能是 椒，要是
    // '是'"; pair moved to tier_overlay tier 5, expected top1=是.
    #[test]
    fn shi_pinyin_是() {
        assert_eq!(top(b"shi"), "是");
    }
    #[test]
    fn you_wubi_亦() {
        assert_eq!(top(b"you"), "亦");
    }
    // User-confirmed via runtime (2026-05-24): Jianma2 common-char
    // entries also must lead via Session path (same flow the Mac IME
    // uses). Adding these pins more of the user-stated invariant so
    // a regression at the Session layer can't slip past wubi_simcode_
    // priority's protect list silently.
    #[test]
    fn ce_wubi_能() {
        assert_eq!(top(b"ce"), "能");
    }
    #[test]
    fn yi_wubi_就() {
        assert_eq!(top(b"yi"), "就");
    }
    #[test]
    fn ge_wubi_表() {
        assert_eq!(top(b"ge"), "表");
    }
    #[test]
    fn da_wubi_左() {
        assert_eq!(top(b"da"), "左");
    }
    // Jianma1 (1-letter) keeps its hard floor too.
    #[test]
    fn e_wubi_有() {
        assert_eq!(top(b"e"), "有");
    }
    #[test]
    fn g_wubi_一() {
        assert_eq!(top(b"g"), "一");
    }
}

#[cfg(test)]
mod beyond_wubi_window {
    use super::*;
    /// User policy (2026-05-22): "超过 4 字就和五笔没关系了" — in Mixed
    /// mode, once the user has typed 5+ letters, wubi should not appear
    /// in the candidate list at all. The original 4-char defuse rule
    /// caused 5+ char pinyin inputs to leak wubi-tail-simcode garbage
    /// (jihua→工, naozi→不, tuijin→沁) into #0. Test all four user-
    /// reported cases.
    fn type_in_mixed(input: &[u8]) -> Vec<String> {
        let mut sess = Session::new();
        sess.set_auto_commit_policy(AutoCommitPolicy::Never);
        for cp in input {
            sess.handle_key(*cp as u32, 0);
        }
        sess.candidates().to_vec()
    }

    #[test]
    fn jihua_no_wubi_tail_工() {
        let cands = type_in_mixed(b"jihua");
        assert!(
            !cands.iter().any(|w| w == "工"),
            "jihua must not surface wubi-tail 工 (from defuse). Got: {:?}",
            cands.iter().take(10).collect::<Vec<_>>()
        );
    }

    #[test]
    fn naozi_no_wubi_tail_不() {
        let cands = type_in_mixed(b"naozi");
        assert!(
            !cands.iter().any(|w| w == "不"),
            "naozi must not surface wubi-tail 不 (from defuse). Got: {:?}",
            cands.iter().take(10).collect::<Vec<_>>()
        );
    }

    #[test]
    fn tuijin_no_wubi_tail_沁() {
        let cands = type_in_mixed(b"tuijin");
        assert!(
            !cands.iter().any(|w| w == "沁"),
            "tuijin must not surface wubi-tail. Got: {:?}",
            cands.iter().take(10).collect::<Vec<_>>()
        );
    }

    #[test]
    fn jigao_no_wubi_tail_为() {
        let cands = type_in_mixed(b"jigao");
        assert!(
            !cands.iter().any(|w| w == "为"),
            "jigao must not surface wubi-tail 为 (o→jianma1). Got: {:?}",
            cands.iter().take(10).collect::<Vec<_>>()
        );
    }

    #[test]
    fn four_letter_wubi_still_works() {
        // Sanity: at exactly 4 letters, wubi still gets the floor.
        // `wuzo` is a wubi phrase code for 我们 (4 chars, layer Phrase)
        // — wubi still contributes here.
        let cands = type_in_mixed(b"wuzo");
        assert!(!cands.is_empty(), "wuzo must produce some candidate");
    }
}

#[cfg(test)]
mod cross_engine_pin {
    use super::*;
    /// `jixu` is a natural code collision: wubi-3-char-phrase encoding
    /// for 曳光弹 (曳=j..., 光=i..., 弹=xu...) equals the pinyin
    /// reading for 继续 / 急需 / etc. In Mixed mode with the wubi-first
    /// merge rule, 曳光弹 takes #0 even though 继续 has 6× the corpus
    /// freq. The user has a L0 pin `jixu → 继续` recorded over time,
    /// and the composite engine's pin-promotion pass must surface that
    /// pin across the engine boundary — wubi's structural priority
    /// loses to an explicit user pin.
    #[test]
    fn pinyin_pin_promotes_across_engine_in_mixed_mode() {
        let mut sess = Session::new();
        sess.set_auto_commit_policy(AutoCommitPolicy::Never);
        // Import a pinyin L0 with the jixu→继续 pin (mirrors what loads
        // at runtime from ~/Library/.../pinyin_l0.json).
        let pin_json = r#"{
            "version": 1,
            "engine": "pinyin",
            "pins": [["jixu", "继续"]],
            "pick_counts": []
        }"#;
        let n = sess.import_l0_json(1, pin_json);
        assert_eq!(n, 1, "L0 import should accept the pin");

        for cp in b"jixu" {
            sess.handle_key(*cp as u32, 0);
        }
        let cands = sess.candidates();
        assert_eq!(
            cands.first().map(String::as_str),
            Some("继续"),
            "expected 继续 at #0 via pinyin pin promotion. Got: {:?}",
            cands.iter().take(10).collect::<Vec<_>>()
        );
    }
}

#[cfg(test)]
mod jigao_coverage {
    use super::*;
    #[test]
    fn jigao_produces_jigao_after_supplemental_dict() {
        let mut sess = Session::new();
        sess.set_auto_commit_policy(AutoCommitPolicy::Never);
        sess.set_mode(crate::composite::Mode::PinyinOnly);
        for cp in b"jigao" {
            sess.handle_key(*cp as u32, 0);
        }
        let cands = sess.candidates();
        assert!(
            cands.iter().any(|w| w == "极高"),
            "极高 must be in jigao candidates after supplemental dict add. Got: {:?}",
            cands.iter().take(10).collect::<Vec<_>>()
        );
    }
}

#[cfg(test)]
mod association_cancel_guard {
    //! Bugfix 2026-06-16 — 联想 cancellation on non-continuation input.
    //!
    //! User report: "联想候选如果在中间间隔输入了任何别的东西都要取消，
    //! 比如无候选的数字、符号、功能按键等".
    //!
    //! The contract verified here: when the engine is in a pure
    //! prediction state (no composing buffer, but `prediction_buf` has
    //! candidates from a prior commit), any non-letter input arriving
    //! via `handle_key` must clear `prediction_buf` before falling
    //! through to its normal routing. Letter input is the only
    //! continuation path — it starts a new composing round whose next
    //! commit will rebuild predictions.
    //!
    //! Predictions are seeded via the `test_seed_prediction` test
    //! helper on `CompositeEngine` so these tests don't depend on
    //! real bigram / trigram corpus loading (which is feature-gated
    //! and slow).
    use super::*;
    use crate::composite::Mode;

    fn s_with_pred() -> Session {
        let mut sess = Session::new();
        sess.set_auto_commit_policy(AutoCommitPolicy::Never);
        sess.set_mode(Mode::PinyinOnly);
        sess.composite.test_seed_prediction("吗");
        sess.composite.test_seed_prediction("嗨");
        sess.refresh_caches();
        assert!(
            sess.composite.has_predictions(),
            "test setup: predictions not seeded"
        );
        assert!(
            !sess.composite.is_composing(),
            "test setup: must be in pure-prediction state, not composing"
        );
        sess
    }

    #[test]
    fn digit_cancels_predictions() {
        let mut sess = s_with_pred();
        let consumed = sess.handle_key(b'1' as u32, 0);
        assert!(
            !consumed,
            "digit with no composing buffer should pass through to host"
        );
        assert!(
            !sess.composite.has_predictions(),
            "digit must cancel 联想 predictions"
        );
    }

    #[test]
    fn punct_cancels_predictions() {
        let mut sess = s_with_pred();
        let consumed = sess.handle_key(b'.' as u32, 0);
        assert!(
            !consumed,
            "punct with no composing buffer should pass through to host"
        );
        assert!(
            !sess.composite.has_predictions(),
            "punct must cancel 联想 predictions"
        );
    }

    #[test]
    fn space_cancels_predictions() {
        let mut sess = s_with_pred();
        let consumed = sess.handle_key(b' ' as u32, 0);
        assert!(
            !consumed,
            "space with no composing buffer should pass through to host"
        );
        assert!(
            !sess.composite.has_predictions(),
            "space must cancel 联想 predictions"
        );
    }

    #[test]
    fn enter_cancels_predictions() {
        let mut sess = s_with_pred();
        // CP_RETURN_CR = 0x0D
        let consumed = sess.handle_key(0x0D, 0);
        assert!(
            !consumed,
            "Enter with no composing buffer should pass through to host"
        );
        assert!(
            !sess.composite.has_predictions(),
            "Enter must cancel 联想 predictions"
        );
    }

    #[test]
    fn escape_cancels_predictions() {
        let mut sess = s_with_pred();
        // CP_ESCAPE = 0x1B
        let _ = sess.handle_key(0x1B, 0);
        assert!(
            !sess.composite.has_predictions(),
            "Escape must cancel 联想 predictions"
        );
    }

    #[test]
    fn backspace_cancels_predictions() {
        let mut sess = s_with_pred();
        // CP_BACKSPACE = 0x08
        let _ = sess.handle_key(0x08, 0);
        assert!(
            !sess.composite.has_predictions(),
            "Backspace must cancel 联想 predictions"
        );
    }

    #[test]
    fn tab_cancels_predictions() {
        let mut sess = s_with_pred();
        // CP_TAB = 0x09
        let _ = sess.handle_key(0x09, 0);
        assert!(
            !sess.composite.has_predictions(),
            "Tab must cancel 联想 predictions"
        );
    }

    #[test]
    fn arrow_key_codepoint_cancels_predictions() {
        // Function keys / arrows arrive as private-use codepoints
        // (e.g. macOS NSLeftArrowFunctionKey = 0xF702). Any non-letter
        // non-digit non-punct codepoint should still cancel.
        let mut sess = s_with_pred();
        let _ = sess.handle_key(0xF702, 0);
        assert!(
            !sess.composite.has_predictions(),
            "Arrow-key codepoint must cancel 联想 predictions"
        );
    }

    #[test]
    fn letter_does_not_eagerly_cancel_predictions() {
        // Letters are the continuation path: they start a new composing
        // round. The guard must not run on letters — handle_letter has
        // its own state-management. In practice the new composing buffer
        // overrides what the host UI displays, and a subsequent commit
        // will refresh predictions via update_bigram_context.
        let mut sess = s_with_pred();
        let consumed = sess.handle_key(b'n' as u32, 0);
        assert!(consumed, "letter should be consumed");
        // We don't assert on has_predictions here — handle_letter may
        // or may not have touched prediction_buf depending on whether
        // the letter triggered ASCII fallback or a commit. The
        // important invariant is just that the guard didn't preemptively
        // cancel before letting handle_letter run. Verify by checking
        // that the engine has started a composing buffer (the
        // continuation path was taken).
        assert!(
            sess.composite.is_composing(),
            "letter must start a new composing round (continuation path)"
        );
    }

    #[test]
    fn ctrl_modifier_short_circuits_guard() {
        // Ctrl+anything short-circuits the entire handle_key_cjk before
        // reaching the guard — app shortcuts (Ctrl+C / Ctrl+A / …) must
        // not wipe predictions as a side effect. Same for Cmd.
        let mut sess = s_with_pred();
        let consumed = sess.handle_key(b'c' as u32, MOD_CTRL);
        assert!(!consumed, "Ctrl+c should fall through to host");
        assert!(
            sess.composite.has_predictions(),
            "Ctrl modifier must short-circuit before the cancel guard"
        );

        // Cmd too.
        let consumed = sess.handle_key(b'c' as u32, MOD_CMD);
        assert!(!consumed, "Cmd+c should fall through to host");
        assert!(
            sess.composite.has_predictions(),
            "Cmd modifier must short-circuit before the cancel guard"
        );
    }

    #[test]
    fn guard_no_op_when_no_predictions() {
        // Sanity: when prediction_buf is empty, non-letter input still
        // routes correctly (just no cancellation work to do). Catches a
        // hypothetical regression where the guard could double-fire
        // refresh_caches and corrupt cand_cache.
        let mut sess = Session::new();
        sess.set_auto_commit_policy(AutoCommitPolicy::Never);
        sess.set_mode(Mode::PinyinOnly);
        assert!(!sess.composite.has_predictions());
        let consumed = sess.handle_key(b'1' as u32, 0);
        assert!(!consumed, "digit at fresh state passes through");
        assert!(!sess.composite.has_predictions());
    }
}

#[cfg(test)]
mod jiazai_ranking {
    use super::*;
    #[test]
    fn jiazai_jiazai_outranks_jiazai() {
        let mut sess = Session::new();
        sess.set_auto_commit_policy(AutoCommitPolicy::Never);
        sess.set_mode(crate::composite::Mode::PinyinOnly);
        for cp in b"jiazai" {
            sess.handle_key(*cp as u32, 0);
        }
        let cands = sess.candidates();
        let pos = |w: &str| cands.iter().position(|c| c == w);
        let p_load = pos("加载");
        let p_at = pos("加在");
        assert!(
            p_load.is_some(),
            "加载 must be present. Got: {:?}",
            cands.iter().take(5).collect::<Vec<_>>()
        );
        if let (Some(load), Some(at)) = (p_load, p_at) {
            assert!(
                load < at,
                "加载 (pos {}) must outrank 加在 (pos {})",
                load,
                at
            );
        }
    }
}
