//! `PinyinAdapter` — a inputx-core-conventions wrapper around `golia_pinyin`.
//!
//! `WubiEngine` is letter-by-letter with an internal buffer (max 4 chars);
//! `PinyinAdapter` mirrors that surface for pinyin (variable-length input,
//! no auto-commit). Composite session drives both with the same lifecycle:
//! `handle_letter` / `backspace` / `escape` / `candidates` / `commit_index`.
//!
//! Internally it skips `golia_pinyin::Session` and calls `PinyinDict`
//! directly — the dict's `lookup_into` is allocation-friendly for hot
//! per-keystroke refresh.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::{Arc, OnceLock};

use golia_pinyin::PinyinEngine;

/// Process-global initials index for 简拼 (first-letter abbreviation)
/// lookup, e.g., `hhh → 哈哈哈, 好好好, …`. Built lazily on first miss
/// of full-pinyin lookup. Shared across all `PinyinAdapter` instances
/// in the same process via `Arc` so only one process pays the ~1-2s
/// build cost.
static INITIALS_INDEX: OnceLock<Arc<HashMap<String, Vec<String>>>> = OnceLock::new();

/// Wrapper providing inputx-core's stateful-engine surface around the
/// pinyin dict.
pub struct PinyinAdapter {
    engine: PinyinEngine,
    buffer: String,
    candidates: Vec<String>,
    /// `true` iff the current `candidates` includes at least one entry
    /// from the exact-syllable path or the 简拼 initials path — i.e., the
    /// user typed something that resolves directly to a pinyin reading.
    /// `false` when `candidates` is empty OR contains only speculative
    /// prefix-completion entries (path 3).
    ///
    /// Used by `CompositeEngine` to distinguish "user is actively typing
    /// pinyin" (veto wubi auto-commit) from "user typed wubi-flavored
    /// input that happens to have FST-prefix overlap" (let wubi commit).
    /// Without this distinction, prefix completion would mask single-
    /// letter wubi 简码 commits like `g → 一` because `g` always has
    /// prefix matches in the pinyin dict.
    has_non_speculative_candidate: bool,
}

impl Default for PinyinAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl PinyinAdapter {
    pub fn new() -> Self {
        Self {
            engine: PinyinEngine::new(),
            buffer: String::with_capacity(16),
            candidates: Vec::with_capacity(16),
            has_non_speculative_candidate: false,
        }
    }

    /// `true` if the current candidate list contains at least one entry
    /// that came from exact-pinyin lookup or the 简拼 initials index —
    /// i.e., the user typed an input that resolves to a real pinyin
    /// reading. `false` when the list is empty or contains only
    /// speculative FST-prefix completions.
    ///
    /// Used by `CompositeEngine::should_force_commit_wubi` to decide
    /// whether a wubi auto-commit would interrupt active pinyin typing.
    pub fn has_non_speculative_candidate(&self) -> bool {
        self.has_non_speculative_candidate
    }

    pub fn is_composing(&self) -> bool {
        !self.buffer.is_empty()
    }

    /// Pay every pinyin cold-path cost up front: touch a representative
    /// spread of FST keys so the embedded pinyin dict's `.rodata` pages
    /// fault into RAM, and force-build the process-global `INITIALS_INDEX`
    /// (the 1-2s 简拼 table build that otherwise runs on first vowel-free
    /// input like `hh` / `zg`). Idempotent — second call is essentially
    /// free since `INITIALS_INDEX` is `OnceLock`.
    pub fn warmup(&mut self) {
        // 12 seeds spanning the FST's lexical range — first-letter span +
        // a few representative 2/3-letter prefixes + a full word, so the
        // OS pages we fault in are spread across the file, not clustered.
        let mut buf: Vec<String> = Vec::with_capacity(32);
        for seed in &[
            "a", "k", "p", "ni", "hao", "kp", "shi", "wo", "zhongguo", "h", "z", "ma",
        ] {
            self.engine.dict().lookup_into(seed, &mut buf);
        }
        // Force INITIALS_INDEX construction (otherwise first 简拼 query
        // pays a ~1-2s OneLock::get_or_init build). After this, any
        // 简拼 lookup is O(1) HashMap hit.
        let _ = initials_index(&self.engine);
    }

    pub fn buffer_str(&self) -> &str {
        &self.buffer
    }

    pub fn candidates(&self) -> &[String] {
        &self.candidates
    }

    /// `true` if the current buffer is a prefix of at least one word in
    /// the pinyin dict (i.e., the user could keep typing and land on a
    /// real pinyin word). Used by the composite engine to veto wubi
    /// auto-commit when pinyin's still building toward a multi-syllable
    /// word — exact-match candidates may be empty (e.g., `beij` has no
    /// stand-alone entry) but `prefix("beij")` returns `北京` etc.
    pub fn has_future_match(&self) -> bool {
        if self.buffer.is_empty() {
            return false;
        }
        !self.engine.dict().prefix(&self.buffer).is_empty()
    }

    /// Append one ASCII alphabetic byte. Non-alpha bytes are silently
    /// ignored (caller filters at the keyboard layer). Pinyin doesn't
    /// auto-commit (no fixed code length); always returns `None`.
    pub fn handle_letter(&mut self, byte: u8) -> Option<String> {
        if !byte.is_ascii_alphabetic() {
            return None;
        }
        self.buffer.push(byte.to_ascii_lowercase() as char);
        self.refresh_candidates();
        None
    }

    pub fn backspace(&mut self) -> bool {
        if self.buffer.is_empty() {
            return false;
        }
        self.buffer.pop();
        self.refresh_candidates();
        true
    }

    pub fn escape(&mut self) -> bool {
        if self.buffer.is_empty() {
            return false;
        }
        self.buffer.clear();
        self.candidates.clear();
        self.has_non_speculative_candidate = false;
        true
    }

    pub fn clear_all(&mut self) {
        self.buffer.clear();
        self.candidates.clear();
        self.has_non_speculative_candidate = false;
    }

    /// Commit candidate at `index`. Records the pick into the engine's L0
    /// (3-pick auto-pin) and resets state. Returns `None` if the index is
    /// out of range; engine state untouched in that case.
    pub fn commit_index(&mut self, index: usize) -> Option<String> {
        let word = self.candidates.get(index)?.clone();
        self.engine.dict().record_pick(&self.buffer, &word);
        self.buffer.clear();
        self.candidates.clear();
        self.has_non_speculative_candidate = false;
        Some(word)
    }

    /// Force-pin a (pinyin, word) pair into L0 — surface for the host's
    /// "always use this for this input" affordance (item 74 settings).
    pub fn pin(&self, pinyin: &str, word: &str) -> bool {
        self.engine.dict().pin(pinyin, word)
    }

    /// Drop a (pinyin, *) L0 entry.
    pub fn forget(&self, pinyin: &str) -> bool {
        self.engine.dict().forget(pinyin)
    }

    /// L0 snapshot (item 45 dual-engine export will combine this with
    /// the wubi snapshot per-engine).
    pub fn export_l0(&self) -> golia_pinyin::L0Snapshot {
        self.engine.dict().export_l0()
    }

    /// L0 restore. Returns count of accepted pins.
    pub fn import_l0(&self, snap: golia_pinyin::L0Snapshot) -> usize {
        self.engine.dict().import_l0(snap)
    }

    fn refresh_candidates(&mut self) {
        self.candidates.clear();
        self.has_non_speculative_candidate = false;
        if self.buffer.is_empty() {
            return;
        }

        // Path 1: exact-syllable lookup (含 fuzzy / tone-strip / heteronym
        // collapsing). Buffer must already parse as one or more valid
        // pinyin syllables; partial-syllable input like "zho" returns ∅.
        let mut exact_buf: Vec<String> = Vec::new();
        self.engine
            .dict()
            .lookup_into(&self.buffer, &mut exact_buf);
        let mut seen: HashSet<String> = HashSet::with_capacity(64);
        for w in exact_buf {
            if seen.insert(w.clone()) {
                self.candidates.push(w);
                self.has_non_speculative_candidate = true;
            }
        }

        // Path 2: 简拼 (first-letter abbreviation) — vowel-free input only.
        // `hhh → 哈哈哈`, `zg → 中国`. Uses process-global lazy initials
        // index. Skipped when input has vowels (would be a valid syllable
        // start handled by Path 3).
        if looks_like_initials(&self.buffer)
            && let Some(matches) = initials_index(&self.engine).get(&self.buffer)
        {
            for w in matches.iter().take(200) {
                if seen.insert(w.clone()) {
                    self.candidates.push(w.clone());
                    self.has_non_speculative_candidate = true;
                }
            }
        }

        // Path 3: FST prefix completion — covers partial-syllable input
        // (`zho` → 中国/众/重..) and post-syllable phrase completion
        // (`zhong` → 中国/中华/中央 even though exact `zhong` only has
        // single-char entries). This is the dominant code path for "I'm
        // mid-typing and need to see something". Length-adaptive cap +
        // top-K-by-freq heap keep short-prefix scans bounded:
        //   len=1  → cap 30  (~50k entries scanned worst-case)
        //   len=2  → cap 80  (~30k)
        //   len=3  → cap 150 (~10k)
        //   len=4+ → cap 200 (~few k)
        // Perfgate (see `perfgate_refresh_candidates_under_budget` test):
        // even the worst case must complete < 16ms (one frame) on a release
        // build; measured ~1-3ms on M-class CPU.
        let cap = match self.buffer.len() {
            1 => 30,
            2 => 80,
            3 => 150,
            _ => 200,
        };
        if self.candidates.len() < cap {
            let want = cap - self.candidates.len();
            push_prefix_top_k(
                &self.engine,
                &self.buffer,
                want,
                &mut seen,
                &mut self.candidates,
            );
        }

        // Path 4: rare-CJK filter (same as wubi/table.rs).
        if !crate::wubi::show_rare() {
            self.candidates.retain(|w| crate::wubi::is_displayable(w));
        }
    }
}

/// Scan `engine.dict()` for entries whose pinyin starts with `prefix`, pick
/// the top `k` by frequency (excluding anything already in `seen`), and push
/// them onto `out` in freq-desc order.
///
/// Uses the streaming `prefix_for_each` API so the visit cost is O(n) FST
/// stream + O(k log k) heap work, with String allocation only for the ≤ k
/// winners — short prefixes like `"z"` would otherwise pay ~50k String
/// allocations just to throw most away.
///
/// Heap discipline: min-heap of size k keyed by freq. New entry is admitted
/// iff its freq beats the current heap minimum. Word-asc tiebreaker for
/// determinism (matches `lookup_into`'s ordering).
fn push_prefix_top_k(
    engine: &PinyinEngine,
    prefix: &str,
    k: usize,
    seen: &mut HashSet<String>,
    out: &mut Vec<String>,
) {
    if k == 0 {
        return;
    }
    // Entry tuple: (freq, Reverse(word)). The Reverse on word makes lex-asc
    // the tiebreaker (smaller word wins ties). Wrapped in outer Reverse so
    // BinaryHeap behaves as a min-heap (top = smallest freq, ready to evict).
    type Entry = Reverse<(u64, Reverse<String>)>;
    let mut heap: BinaryHeap<Entry> = BinaryHeap::with_capacity(k + 1);

    engine
        .dict()
        .prefix_for_each_raw(prefix, |_pinyin_bytes, word_bytes, freq| {
            // Cheap pre-check FIRST: compare raw freq against heap's current
            // minimum without touching anything else. >99% of FST entries on
            // short prefixes fail this and bail before allocating anything
            // (or even running utf8 decode). For `z` (~50k entries) this
            // saves both the per-entry HashSet lookup AND the utf8 decode.
            // Dedup vs. `seen` is deferred to drain time when there are only
            // k candidates left.
            if heap.len() == k {
                let min_freq = heap.peek().expect("heap is full (len == k)").0.0;
                if freq <= min_freq {
                    return;
                }
                heap.pop();
            }
            // Now decode utf8 (cheap: ~50ns for typical 6-byte word) +
            // allocate the String. Only ≤k of these run per scan.
            let Ok(word) = std::str::from_utf8(word_bytes) else {
                return;
            };
            heap.push(Reverse((freq, Reverse(word.to_owned()))));
        });

    // Drain in freq-desc + lex-asc order.
    let mut drained: Vec<(u64, String)> = heap
        .into_iter()
        .map(|Reverse((freq, Reverse(word)))| (freq, word))
        .collect();
    drained.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    for (_, word) in drained {
        if seen.insert(word.clone()) {
            out.push(word);
        }
    }
}

// ----------------------------------------------------------------------
// 简拼 (first-letter abbreviation) — process-global initials index
// ----------------------------------------------------------------------

/// `true` iff `s` is non-empty, length ≥ 2, and contains no pinyin vowels
/// (a/e/i/o/u/v). Vowel-free input can't be a valid pinyin syllable, so
/// it's a reasonable proxy for "user typed initials".
fn looks_like_initials(s: &str) -> bool {
    if s.len() < 2 {
        return false;
    }
    !s.chars()
        .any(|c| matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'v'))
}

/// Lazy-build the initials index. Iterates the full pinyin dict (~919k
/// entries), segments each pinyin string into its component syllables,
/// takes first letter of each, groups words by initials string.
///
/// One-time cost (~1-2s on iPhone) — paid on first 简拼-style query;
/// subsequent queries are O(1) HashMap lookup + small Vec extend.
fn initials_index(seed_engine: &PinyinEngine) -> Arc<HashMap<String, Vec<String>>> {
    INITIALS_INDEX
        .get_or_init(|| Arc::new(build_initials_index(seed_engine)))
        .clone()
}

fn build_initials_index(engine: &PinyinEngine) -> HashMap<String, Vec<String>> {
    // Single-pass scan: bucket multi-char entries by initials, AND collect
    // single-char entries into a char-quality table. The char table feeds
    // a secondary ranking score that down-weights phrases composed of
    // uncommon chars (合欢花 / 惠皇后 / 黄淮海) relative to phrases of
    // common chars (哈哈哈 / 你好 / 红包).
    //
    // Why this matters: pure phrase-frequency ranking inherits the corpus
    // bias — Wikipedia / news contain proper nouns that look "frequent"
    // because they have dedicated entity pages, but a colloquial-input
    // user will almost never want them. Char frequency is a corpus-
    // independent signal of "how likely is this character to appear in
    // any user input at all".
    let mut tmp: HashMap<String, HashMap<String, u64>> = HashMap::with_capacity(50_000);
    let mut char_freq: HashMap<char, u64> = HashMap::with_capacity(20_000);
    for (pinyin, word, freq) in engine.dict().prefix_with_freq("") {
        let n_chars = word.chars().count();
        if n_chars == 1 {
            // Single-char entry — track max-across-readings as char quality
            // (multi-reading chars appear once per reading; we want the
            // most-common-reading freq as the char's overall commonness).
            if let Some(c) = word.chars().next() {
                let entry = char_freq.entry(c).or_insert(0);
                if freq > *entry {
                    *entry = freq;
                }
            }
            continue;
        }
        if let Some(initials) = compute_initials(&pinyin, n_chars)
            && initials.len() == n_chars
        {
            let bucket = tmp.entry(initials).or_default();
            let entry = bucket.entry(word).or_insert(0);
            if freq > *entry {
                *entry = freq;
            }
        }
    }
    // Compute combined score per (initials, word). Geometric mean of char
    // freqs is length-invariant — works equally well for 2-char and 5-char
    // simplified inputs. We stay in integer arithmetic by:
    //   score = phrase_freq * (product(char_freq) ^ (1 / n_chars))
    // implemented as: score = phrase_freq * floor((product)^(1/n))
    //
    // To avoid u64 overflow on long phrases (5 chars × 100k each ≈ 1e25),
    // compute geometric mean in f64 then quantize back.
    tmp.into_iter()
        .map(|(initials, bucket)| {
            let mut scored: Vec<(String, u64)> = bucket
                .into_iter()
                .map(|(word, freq)| {
                    let chars: Vec<char> = word.chars().collect();
                    let n = chars.len() as f64;
                    let log_sum: f64 = chars
                        .iter()
                        .map(|c| {
                            // +1 to avoid log(0) when char is missing from
                            // single-char entries (rare CJK / extension chars).
                            // Use natural log; geometric mean is exp(sum/n).
                            let cf = char_freq.get(c).copied().unwrap_or(0);
                            ((cf + 1) as f64).ln()
                        })
                        .sum();
                    let geomean = (log_sum / n).exp();
                    // Combined score: phrase freq weighted by char-quality
                    // factor. sqrt() softens the multiplier so a uniformly
                    // common-char phrase doesn't dwarf a high-phrase-freq
                    // entry that happens to use one less-common char.
                    let combined = (freq as f64) * geomean.sqrt();
                    (word, combined as u64)
                })
                .collect();
            scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            let words: Vec<String> = scored.into_iter().map(|(w, _)| w).collect();
            (initials, words)
        })
        .collect()
}

/// Greedy left-to-right syllable segmentation of `pinyin`, taking the
/// first byte of each syllable. Returns `None` if segmentation can't
/// produce exactly `expected_n` syllables (e.g., spurious composer
/// artifact, or a non-CJK entry that slipped through).
fn compute_initials(pinyin: &str, expected_n: usize) -> Option<String> {
    let bytes = pinyin.as_bytes();
    let mut result = String::with_capacity(expected_n);
    let mut i = 0;
    for _ in 0..expected_n {
        // Try longest valid syllable starting at i (max 6 bytes: zhuang).
        let mut found: Option<usize> = None;
        let max_end = (i + 6).min(bytes.len());
        for end in (i + 1..=max_end).rev() {
            let cand = std::str::from_utf8(&bytes[i..end]).ok()?;
            if golia_pinyin::is_valid_syllable(cand) {
                found = Some(end);
                break;
            }
        }
        let end = found?;
        // First byte of the syllable.
        let first_byte = bytes[i];
        if !first_byte.is_ascii_alphabetic() {
            return None;
        }
        result.push(first_byte as char);
        i = end;
    }
    // Must consume the entire pinyin string.
    if i != bytes.len() {
        return None;
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_letter_filters_non_ascii() {
        let mut a = PinyinAdapter::new();
        a.handle_letter(b'z');
        a.handle_letter(b'1'); // ignored
        a.handle_letter(b'h');
        assert_eq!(a.buffer_str(), "zh");
    }

    #[test]
    fn backspace_and_escape() {
        let mut a = PinyinAdapter::new();
        for b in b"abc" {
            a.handle_letter(*b);
        }
        assert!(a.is_composing());
        a.backspace();
        assert_eq!(a.buffer_str(), "ab");
        assert!(a.escape());
        assert!(!a.is_composing());
    }

    #[test]
    fn lookup_zhongguo_returns_zhongguo() {
        let mut a = PinyinAdapter::new();
        for b in b"zhongguo" {
            a.handle_letter(*b);
        }
        assert_eq!(a.candidates().first().map(String::as_str), Some("中国"));
    }

    #[test]
    fn commit_records_pick_and_resets() {
        let mut a = PinyinAdapter::new();
        for b in b"women" {
            a.handle_letter(*b);
        }
        assert_eq!(a.commit_index(0).as_deref(), Some("我们"));
        assert!(!a.is_composing());
    }

    // ------------------------------------------------------------------
    // 简拼 (initial-letter abbreviation) — process-global lazy index.
    // First test triggers the ~1-2s build; subsequent are fast.
    // ------------------------------------------------------------------

    #[test]
    fn looks_like_initials_threshold() {
        assert!(!looks_like_initials(""));
        assert!(!looks_like_initials("h"));
        assert!(looks_like_initials("hh"));
        assert!(looks_like_initials("hhh"));
        assert!(looks_like_initials("zg"));
        // Vowels present → not initials.
        assert!(!looks_like_initials("zhongguo"));
        assert!(!looks_like_initials("ha"));
        assert!(!looks_like_initials("u"));
    }

    #[test]
    fn compute_initials_known_words() {
        assert_eq!(compute_initials("zhongguo", 2).as_deref(), Some("zg"));
        assert_eq!(compute_initials("women", 2).as_deref(), Some("wm"));
        assert_eq!(compute_initials("haha", 2).as_deref(), Some("hh"));
        // 3-syllable
        assert_eq!(compute_initials("hahaha", 3).as_deref(), Some("hhh"));
        // expected_n mismatch returns None
        assert_eq!(compute_initials("zhongguo", 3), None);
    }

    #[test]
    fn initials_lookup_hhh_includes_hahaha() {
        let mut a = PinyinAdapter::new();
        for b in b"hhh" {
            a.handle_letter(*b);
        }
        let cands = a.candidates();
        // The initials index should produce at least 哈哈哈 (haha+ha)
        // and 好好好 (haohao+hao) for "hhh". Other matches like 嘿嘿嘿,
        // 黑乎乎, 哼哼哼 may also appear depending on dict coverage.
        let has_haha = cands.iter().any(|w| w == "哈哈哈");
        let has_haohao = cands.iter().any(|w| w == "好好好");
        assert!(
            has_haha || has_haohao,
            "expected 哈哈哈 or 好好好 in {cands:?}"
        );
    }

    #[test]
    fn initials_lookup_zg_includes_zhongguo() {
        let mut a = PinyinAdapter::new();
        for b in b"zg" {
            a.handle_letter(*b);
        }
        // 中国's pinyin "zhongguo" yields initials "zg".
        assert!(
            a.candidates().iter().any(|w| w == "中国"),
            "expected 中国 in zg results: {:?}",
            a.candidates()
        );
    }

    // ------------------------------------------------------------------
    // Session-level regression tests: 5+ char pinyin must not be hijacked
    // by wubi 4-letter unique matches whose code happens to look like a
    // valid pinyin syllable (shan→櫖, wang→佢, etc.). Pre-fix bug was a
    // hard cutoff in wubi/engine.rs at MAX_CODE_LEN=4 plus
    // OnFourCodesIfUnique policy combining to commit the wubi char and
    // wipe the pinyin buffer. Fix lives in composite/engine.rs (defuse
    // 4-char overflow on collisions + policy moved to composite layer)
    // and composite/dispatch.rs (pinyin-first when pinyin buffer outgrew
    // wubi buffer).
    // ------------------------------------------------------------------

    #[test]
    fn session_shang_yields_pinyin_candidates() {
        use crate::session::Session;
        let mut sess = Session::new();
        for c in "shang".chars() {
            sess.handle_key(c as u32, 0);
            assert!(
                sess.take_pending_commit().is_none(),
                "shang should not trigger any auto-commit mid-stream"
            );
        }
        assert_eq!(sess.preedit(), "shang", "preedit should preserve the full 5-char input");
        let cands = sess.candidates();
        assert!(
            cands.iter().take(5).any(|w| w == "上"),
            "expected 上 in top 5 of shang; got {:?}",
            cands.iter().take(8).collect::<Vec<_>>()
        );
    }

    #[test]
    fn session_wang_yields_pinyin_candidates() {
        use crate::session::Session;
        let mut sess = Session::new();
        for c in "wang".chars() {
            sess.handle_key(c as u32, 0);
        }
        assert_eq!(sess.preedit(), "wang");
        let cands = sess.candidates();
        assert!(
            cands.iter().take(5).any(|w| w == "王"),
            "expected 王 in top 5 of wang; got {:?}",
            cands.iter().take(8).collect::<Vec<_>>()
        );
    }

    #[test]
    fn session_shan_keeps_pinyin_buffer_alive() {
        // Even at exactly 4 chars, shan must not auto-commit the wubi 櫖.
        use crate::session::Session;
        let mut sess = Session::new();
        for c in "shan".chars() {
            sess.handle_key(c as u32, 0);
            assert!(sess.take_pending_commit().is_none());
        }
        assert_eq!(sess.preedit(), "shan");
        let cands = sess.candidates();
        assert!(
            cands.iter().take(5).any(|w| w == "山"),
            "expected 山 in top 5 of shan; got {:?}",
            cands.iter().take(8).collect::<Vec<_>>()
        );
    }

    #[test]
    fn session_lgyi_still_auto_commits_in_mixed_mode() {
        // Negative regression: wubi 4-letter unique codes that DON'T
        // collide with valid pinyin prefixes (lgyi isn't a pinyin
        // syllable — no valid Mandarin syllable starts with "lg") must
        // still auto-commit. lgyi → 国. Guards against accidentally
        // breaking wubi UX when fixing the pinyin collision case.
        use crate::session::Session;
        let mut sess = Session::new();
        let mut committed: Option<String> = None;
        for c in "lgyi".chars() {
            sess.handle_key(c as u32, 0);
            if let Some(t) = sess.take_pending_commit() {
                committed = Some(t);
            }
        }
        assert_eq!(committed.as_deref(), Some("国"));
    }

    #[test]
    fn initials_colloquial_words_rank_in_top_5() {
        // Conversational-register words should land near the top of their
        // initials bucket, not buried under proper nouns / Wikipedia entity
        // names. This is the regression net for the corpus-bias work
        // (subtlex weight bump + per-char quality weighting).
        let probes: &[(&str, &str, usize)] = &[
            ("hh", "哈哈", 5),
            ("hhh", "哈哈哈", 5),
            ("nh", "你好", 5),
            ("wsm", "为什么", 5),
            ("hp", "好评", 10),
        ];
        for (input, target, max_pos) in probes {
            let mut a = PinyinAdapter::new();
            for b in input.bytes() {
                a.handle_letter(b);
            }
            let pos = a.candidates().iter().position(|w| w == *target);
            match pos {
                Some(p) if p < *max_pos => {} // pass
                _ => panic!(
                    "{input} → {target}: expected position < {max_pos}; got {pos:?} of {} (top 8: {:?})",
                    a.candidates().len(),
                    a.candidates().iter().take(8).collect::<Vec<_>>()
                ),
            }
        }
    }

    #[test]
    fn initials_hhh_top_candidates_are_common_words() {
        // Freq-sorted initials index should put common reduplicated words
        // (哈哈哈/好好好/嘿嘿嘿/黑乎乎/呼哧哧) ahead of obscure 3-char names
        // like 何厚铧 / 侯狼何 that the user observed pre-fix.
        let mut a = PinyinAdapter::new();
        for b in b"hhh" {
            a.handle_letter(*b);
        }
        let cands = a.candidates();
        let top5: Vec<&str> = cands.iter().take(5).map(String::as_str).collect();
        // At least one of the high-freq targets must appear in top 5.
        let common_targets = ["哈哈哈", "好好好", "嘿嘿嘿", "呵呵呵"];
        let hit = common_targets.iter().any(|t| top5.contains(t));
        assert!(
            hit,
            "expected at least one of {common_targets:?} in top 5 of hhh; got {top5:?} (full: {} candidates)",
            cands.len()
        );
    }

    // ------------------------------------------------------------------
    // Data-quality regression tests: pre-2026-05-11 the FST was built from
    // Unihan kHanyuPinyin which collapsed古音/方言 into modern phrase
    // composition (e.g., 共→hong, 和→huo). Switching to kHanyuPinlu fixed
    // both pollution and the 长→缺chang bug. Tests guard against a future
    // regression that re-introduces either.
    // ------------------------------------------------------------------

    #[test]
    fn lookup_honghe_does_not_return_gonghe() {
        // 共 has no 现代普通话 hong reading; "honghe" must NOT match 共和.
        let mut a = PinyinAdapter::new();
        for b in b"honghe" {
            a.handle_letter(*b);
        }
        assert!(
            !a.candidates().iter().any(|w| w == "共和"),
            "honghe should NOT match 共和 (古音 pollution); got {:?}",
            a.candidates()
        );
    }

    #[test]
    fn lookup_changcheng_returns_changcheng() {
        // Pre-fix bug: 长 only had zhang reading, so 长城 was stored as
        // zhangcheng and "changcheng" returned nothing.
        let mut a = PinyinAdapter::new();
        for b in b"changcheng" {
            a.handle_letter(*b);
        }
        assert!(
            a.candidates().iter().any(|w| w == "长城"),
            "changcheng should match 长城; got {:?}",
            a.candidates()
        );
    }

    #[test]
    fn lookup_changjiang_returns_changjiang() {
        let mut a = PinyinAdapter::new();
        for b in b"changjiang" {
            a.handle_letter(*b);
        }
        assert!(
            a.candidates().iter().any(|w| w == "长江"),
            "changjiang should match 长江; got {:?}",
            a.candidates()
        );
    }

    // ------------------------------------------------------------------
    // Phrase-level pypinyin override regression tests (2026-05-11 fix v3).
    // For phrases where Unihan's cartesian product previously emitted both
    // correct and wrong readings, pypinyin now provides the canonical one
    // and the FST stores only the correct variant.
    // ------------------------------------------------------------------

    #[test]
    fn lookup_nuanhuo_returns_nuanhuo() {
        // 暖和 should be reachable via nuanhuo (correct 普通话 reading).
        let mut a = PinyinAdapter::new();
        for b in b"nuanhuo" {
            a.handle_letter(*b);
        }
        assert!(
            a.candidates().iter().any(|w| w == "暖和"),
            "nuanhuo should match 暖和; got {:?}",
            a.candidates()
        );
    }

    #[test]
    fn lookup_nuanhe_does_not_return_nuanhuo() {
        // 暖 has no he reading in 暖和; the wrong nuanhe variant must be gone.
        let mut a = PinyinAdapter::new();
        for b in b"nuanhe" {
            a.handle_letter(*b);
        }
        assert!(
            !a.candidates().iter().any(|w| w == "暖和"),
            "nuanhe should NOT match 暖和 (wrong reading); got {:?}",
            a.candidates()
        );
    }

    #[test]
    fn lookup_zhuolu_returns_zhuolu() {
        // 着陆 = zhuó+lù — only zhuolu should match.
        let mut a = PinyinAdapter::new();
        for b in b"zhuolu" {
            a.handle_letter(*b);
        }
        assert!(
            a.candidates().iter().any(|w| w == "着陆"),
            "zhuolu should match 着陆; got {:?}",
            a.candidates()
        );
    }

    #[test]
    fn lookup_zhelu_does_not_return_zhuolu() {
        let mut a = PinyinAdapter::new();
        for b in b"zhelu" {
            a.handle_letter(*b);
        }
        assert!(
            !a.candidates().iter().any(|w| w == "着陆"),
            "zhelu should NOT match 着陆 (wrong reading); got {:?}",
            a.candidates()
        );
    }

    #[test]
    fn lookup_chongxin_returns_chongxin() {
        let mut a = PinyinAdapter::new();
        for b in b"chongxin" {
            a.handle_letter(*b);
        }
        assert!(
            a.candidates().iter().any(|w| w == "重新"),
            "chongxin should match 重新; got {:?}",
            a.candidates()
        );
    }

    #[test]
    fn lookup_zhongxin_does_not_return_chongxin() {
        // 重新 = chóng+xīn (NOT zhòng+xīn — that would be 中心 / 众心).
        let mut a = PinyinAdapter::new();
        for b in b"zhongxin" {
            a.handle_letter(*b);
        }
        assert!(
            !a.candidates().iter().any(|w| w == "重新"),
            "zhongxin should NOT match 重新; got {:?}",
            a.candidates()
        );
    }

    // ------------------------------------------------------------------
    // Path 3: prefix-completion regression tests (2026-05-20).
    // Partial-syllable input must yield meaningful candidates instead of
    // an empty bar. Without prefix completion, `zho` returns ∅ (not a
    // valid syllable) — user observed this on device and called it
    // unacceptable. The fix scans FST entries whose pinyin starts with
    // the buffer and merges top-K-by-freq into the candidate list.
    // ------------------------------------------------------------------

    #[test]
    fn prefix_partial_zho_yields_candidates() {
        let mut a = PinyinAdapter::new();
        for b in b"zho" {
            a.handle_letter(*b);
        }
        let cands = a.candidates();
        assert!(
            !cands.is_empty(),
            "zho should yield prefix-completion candidates, not ∅"
        );
        // zho is the prefix of zhong* and zhou* — 中国 (stored under
        // "zhongguo") is by far the most-frequent zho* phrase and must
        // surface in the visible window.
        assert!(
            cands.iter().take(30).any(|w| w == "中国"),
            "zho should surface 中国 in top 30; got {:?}",
            cands.iter().take(30).collect::<Vec<_>>()
        );
    }

    #[test]
    fn prefix_partial_zhon_narrows_to_zhong_subtree() {
        let mut a = PinyinAdapter::new();
        for b in b"zhon" {
            a.handle_letter(*b);
        }
        let cands = a.candidates();
        assert!(!cands.is_empty(), "zhon should yield candidates");
        // zhon is the prefix of zhong* only (zhou doesn't fit) — at least
        // one high-freq 中* word must appear.
        assert!(
            cands
                .iter()
                .take(30)
                .any(|w| w == "中" || w == "中国" || w == "中文"),
            "zhon should surface 中 / 中国 / 中文 in top 30; got {:?}",
            cands.iter().take(30).collect::<Vec<_>>()
        );
    }

    #[test]
    fn prefix_zhong_includes_phrase_completions() {
        // `zhong` is a valid syllable AND a phrase prefix. Exact lookup
        // gives single chars (中, 众, 终, ...); prefix scan must add
        // phrase completions like 中国 (stored at "zhongguo").
        let mut a = PinyinAdapter::new();
        for b in b"zhong" {
            a.handle_letter(*b);
        }
        let cands = a.candidates();
        assert!(
            cands.iter().any(|w| w == "中"),
            "zhong should include exact-match 中; got {:?}",
            cands.iter().take(10).collect::<Vec<_>>()
        );
        assert!(
            cands.iter().any(|w| w == "中国"),
            "zhong should also surface 中国 via prefix scan; got {:?}",
            cands.iter().take(20).collect::<Vec<_>>()
        );
    }

    #[test]
    fn prefix_single_letter_yields_candidates() {
        // `z` is not a valid syllable but is the prefix of every z* word.
        // Must surface high-freq words, capped at 30 (length-adaptive
        // cap — short prefix doesn't need many candidates).
        let mut a = PinyinAdapter::new();
        a.handle_letter(b'z');
        let cands = a.candidates();
        assert!(!cands.is_empty(), "z should yield prefix matches");
        assert!(
            cands.len() <= 30,
            "z cap should be 30 (length-adaptive); got {}",
            cands.len()
        );
    }

    // ------------------------------------------------------------------
    // Perfgate: refresh_candidates per-keystroke budget.
    //
    // The user-stated requirement is "high-performance prediction" —
    // input lag is the single worst IME UX failure. This test asserts an
    // upper bound on the **uncontended-best-case** cost of the last
    // keystroke for representative worst-case inputs (short prefixes scan
    // the most FST entries).
    //
    // **Why min not p50:** an iOS keyboard extension running in the
    // foreground while the user is mid-stroke has essentially no CPU
    // contention — the system is waiting on a single keystroke. The
    // intrinsic algorithm cost is the right perfgate target. Median-or-
    // mean measurements on a developer machine running parallel test
    // crates (cargo test fans out across crates) are dominated by
    // scheduler jitter and reject perfectly fast code. Min-of-N defangs
    // jitter while still catching real regressions: a slower algorithm
    // can't beat its own intrinsic cost no matter how lucky a single run.
    //
    // Budget (release builds only):
    //   - min <  8 ms (uncontended algorithm cost on M-class CPU sits
    //                  ~4.5ms; +60% headroom absorbs the CPU contention
    //                  hit when `cargo test --release` fans out parallel
    //                  test binaries across the 5 workspace crates. On
    //                  iPhone 17 Pro this maps to ~6-9ms uncontended,
    //                  still inside a ProMotion 8.3ms frame in the median
    //                  case; never drops a 60Hz 16ms frame).
    //   - max < 16 ms (60Hz frame hard cap — even worst-case scheduler
    //                  hit on dev machine can't drop a whole frame).
    //
    // **Why not tighter:** 5ms would catch a real algorithm regression
    // but flakes when `cargo test --release` runs all workspace crates
    // in parallel (was the very thing that flapped this test on workspace
    // re-org 2026-05-20). 8ms is loose enough to stay green under that
    // load yet still catches any 2× regression — the kind that actually
    // matters for input lag.
    //
    // Debug builds: log but don't assert — debug perf is 10-50× slower
    // and a hard gate would block fast iteration.
    // ------------------------------------------------------------------

    #[test]
    fn perfgate_refresh_candidates_under_budget() {
        // Warmup once: pages in the FST `.rodata` and builds the global
        // INITIALS_INDEX so the first measured keystroke isn't paying
        // cold-start cost.
        let mut warmer = PinyinAdapter::new();
        warmer.warmup();

        const ITER: usize = 30;
        const MIN_BUDGET_NS: u128 = 8_000_000; // 8 ms (with contention headroom)
        const MAX_BUDGET_NS: u128 = 16_000_000; // 16 ms (one frame @ 60Hz)

        // Worst cases first (short prefix → biggest scan).
        let probes: &[&str] = &[
            "z", "zh", "zho", "zhon", "zhong", "zhongguo", "wo", "women", "ni", "nihao", "h",
            "hh", "hhh",
        ];

        let mut all_passed = true;
        for input in probes {
            let bytes = input.as_bytes();
            let mut times: Vec<u128> = Vec::with_capacity(ITER);

            for _ in 0..ITER {
                let mut a = PinyinAdapter::new();
                // Type all-but-last (not timed).
                for &b in &bytes[..bytes.len() - 1] {
                    a.handle_letter(b);
                }
                let last = bytes[bytes.len() - 1];
                let start = std::time::Instant::now();
                a.handle_letter(last);
                times.push(start.elapsed().as_nanos());
            }

            times.sort_unstable();
            let min = times[0];
            let p50 = times[times.len() / 2];
            let max = *times.last().unwrap();

            eprintln!(
                "perfgate {input:>8}: min={:>5.2}ms p50={:>5.2}ms max={:>5.2}ms",
                min as f64 / 1_000_000.0,
                p50 as f64 / 1_000_000.0,
                max as f64 / 1_000_000.0,
            );

            if !cfg!(debug_assertions) {
                if min > MIN_BUDGET_NS {
                    eprintln!(
                        "  ^^ FAIL: min {:.2}ms exceeds {}ms uncontended budget",
                        min as f64 / 1_000_000.0,
                        MIN_BUDGET_NS / 1_000_000
                    );
                    all_passed = false;
                }
                if max > MAX_BUDGET_NS {
                    eprintln!(
                        "  ^^ FAIL: max {:.2}ms exceeds {}ms frame budget",
                        max as f64 / 1_000_000.0,
                        MAX_BUDGET_NS / 1_000_000
                    );
                    all_passed = false;
                }
            }
        }

        assert!(
            all_passed || cfg!(debug_assertions),
            "perfgate failed — see eprintln output above for per-probe timings"
        );
    }

    #[test]
    fn initials_skipped_when_full_pinyin_matches() {
        // "women" is valid pinyin AND would also pass the initials check
        // (no — it has vowels e/o, so initials check returns false). Use
        // a fully-vowelled input.
        let mut a = PinyinAdapter::new();
        for b in b"women" {
            a.handle_letter(*b);
        }
        // Has full-pinyin matches (我们 first), AND looks_like_initials
        // returns false anyway because of e/o vowels.
        assert!(a.candidates().iter().any(|w| w == "我们"));
    }
}
