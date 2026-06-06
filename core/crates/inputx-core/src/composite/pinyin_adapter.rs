//! `PinyinAdapter` — a inputx-core-conventions wrapper around `inputx_pinyin`.
//!
//! `WubiEngine` is letter-by-letter with an internal buffer (max 4 chars);
//! `PinyinAdapter` mirrors that surface for pinyin (variable-length input,
//! no auto-commit). Composite session drives both with the same lifecycle:
//! `handle_letter` / `backspace` / `escape` / `candidates` / `commit_index`.
//!
//! Internally it skips `inputx_pinyin::Session` and calls `PinyinDict`
//! directly — the dict's `lookup_into` is allocation-friendly for hot
//! per-keystroke refresh.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::{Arc, OnceLock};

use inputx_ngram::NgramTable;
use inputx_pinyin::PinyinEngine;
use inputx_pinyin_helpers::{
    bigram_boost_from_ngm, combined_bigram_log_prob_q4, legacy_bigram_boost_from_ngm,
    pinyin_idf_reader, EMBEDDED_BIGRAMS_NGM, EMBEDDED_INTER_BIGRAMS_NGM, EMBEDDED_PINYIN_IDF,
};

use crate::rules::builtin::RepeatedLetterExpansion;
use crate::rules::candidate::{CandidateRule, CandidateRuleEngine, RuleCandidate};
use crate::rules::{Context, ContextFlags};
use super::mode::Mode;
use super::scoring;

/// Process-global NgramTable parsed once from the embedded bigrams.ngm
/// blob in `inputx-pinyin-cement`. Composite hot path uses this in
/// place of `inputx_pinyin::PinyinDict::bigram_boost` (v1.4.6 sub-phase
/// C2 cutover) — same Q4 log-prob data source, same legacy-units
/// bonus formula via `legacy_bigram_boost_from_ngm`. Lazy-init
/// because parsing the 595 KB blob + walking its sha256 trailer is
/// ~1 ms and we don't want to pay that on every PinyinAdapter::new
/// (a fresh adapter is created on every iOS Inputx session).
// ─────────────────────────────────────────────────────────────
// v1.14 minimal-pinyin debug toggle (2026-06-06).  User: "我们可
// 以先暂停所有的拼音里 拼接字、联想以及错别字模糊吗？只保留正确
// 拼写和预测性的输入，我们一个个细节来做好".  Three category
// gates; flip a single bool to `false` to re-enable that whole
// category of paths when you're ready to polish it.  Tests that
// pin disabled-path behavior carry an early-return guarded on the
// same const so they auto-revive when the const flips.
//
// `_COMPOSE`:    Path 0b (long-buffer Viterbi sentence), Path 5
//                (K-best short-buffer compose), Path 5b
//                (mechanical fallback compose).
// `_ASSOCIATION`: Path 0a (repeated-letter interjection shortcut
//                hhhh→哈哈哈哈 — user re-classified 2026-06-06 same
//                family as 简拼: 重复字母不应该是简拼拼接出来的吗),
//                Path 2 (简拼 first-letter abbreviation).
// `_FUZZY`:      Path 1b (southern-dialect z/zh swap variants),
//                Path 1c (2-consonant-prefix typo rescue),
//                Path 3b (syllable-aware trim-retry).
//
// Initial state: all three TRUE — only exact-syllable Path 1 +
// prefix-completion Path 3 + rare-CJK Path 4 filter survive.
// This is intentionally aggressive; the user will polish detail-
// by-detail and flip whichever const back off as each category is
// ready.
pub(crate) const PINYIN_DISABLE_COMPOSE: bool = true;
pub(crate) const PINYIN_DISABLE_ASSOCIATION: bool = true;
pub(crate) const PINYIN_DISABLE_FUZZY: bool = true;

fn embedded_bigrams_table() -> &'static NgramTable<&'static [u8]> {
    static TABLE: OnceLock<NgramTable<&'static [u8]>> = OnceLock::new();
    TABLE.get_or_init(|| {
        NgramTable::from_bytes(EMBEDDED_BIGRAMS_NGM)
            .expect("inputx-pinyin-cement EMBEDDED_BIGRAMS_NGM must be a valid NGMv1 blob")
    })
}

/// Process-global NgramTable parsed once from `bigrams_inter.ngm` —
/// inter-token (cross-word) bigram counts. v1.14 K-best 3-segment
/// chain gate consults this in addition to [`embedded_bigrams_table`]
/// so a token-adjacency like `(用, 不)` (corpus-frequent across word
/// boundaries) is recognized even when neither word's intra-table
/// includes the pair. Built with `min_count=15` to cut the (路, 要)-
/// style noise zone (count=12) below the (用, 不) signal zone
/// (count=16) — see user report 2026-06-06 luyaozhi calibration.
fn embedded_inter_bigrams_table() -> &'static NgramTable<&'static [u8]> {
    static TABLE: OnceLock<NgramTable<&'static [u8]>> = OnceLock::new();
    TABLE.get_or_init(|| {
        NgramTable::from_bytes(EMBEDDED_INTER_BIGRAMS_NGM)
            .expect("inputx-pinyin-helpers EMBEDDED_INTER_BIGRAMS_NGM must be a valid NGMv1 blob")
    })
}

/// Touch one byte per 4 KB page across `blob` so the OS keeps the
/// region in the working set even under memory pressure. Used by
/// `warmup()` to pre-fault all the `include_bytes!`-baked dict
/// pages — without this, evicted pages re-fault on first keystroke
/// after a pressure event for 10-50ms each.
///
/// `std::hint::black_box` defeats LLVM's dead-store / dead-read
/// elimination so the byte loads can't be optimized away.
fn warm_embedded_blob(blob: &'static [u8]) {
    let mut sum: u8 = 0;
    let mut i = 0;
    let page = 4096;
    while i < blob.len() {
        sum = sum.wrapping_add(blob[i]);
        i += page;
    }
    if blob.len() > 0 {
        sum = sum.wrapping_add(blob[blob.len() - 1]);
    }
    std::hint::black_box(sum);
}

// v1.4.7 sub-phase A4 step 1 cutover (this commit): exact / fuzzy /
// prefix-prediction reads route through `pinyin_idf_reader()` (cement-
// owned process-global IdfReader over EMBEDDED_PINYIN_IDF) instead of
// `self.engine.dict().lookup_*` / `prefix_for_each_raw`. The sort key
// is now Q4-log additive (`score_q4`, A3); the v1.4.6 C3-revert's
// concern about ~0.5% Q4 round-trip drift inverting the f64 score gate
// no longer applies — `log_prior_q4` is read straight from IDF, no
// round-trip, and legacy f64 `score` is only a tiebreaker. L0 pin
// state (pinned_word / pin / forget / export_l0 / import_l0) and dict-
// graph helpers (prefix_exists / best_composition / top_k_compositions)
// stay on the facade — those are per-session state and FSA structure,
// not corpus lookup.

/// Lazily-built CandidateRuleEngine carrying v3.0.2-migrated rules.
/// Lives behind OnceLock so the priority sort runs once per process.
/// Currently has only `RepeatedLetterExpansion`; subsequent v3.0.2.x
/// commits register more rules here as they're migrated.
static CANDIDATE_RULE_ENGINE: OnceLock<CandidateRuleEngine> = OnceLock::new();

fn candidate_rule_engine() -> &'static CandidateRuleEngine {
    CANDIDATE_RULE_ENGINE.get_or_init(|| {
        let rules: Vec<Arc<dyn CandidateRule>> = vec![
            Arc::new(RepeatedLetterExpansion),
        ];
        CandidateRuleEngine::new(rules)
    })
}

/// Process-global initials index for 简拼 (first-letter abbreviation)
/// lookup, e.g., `hhh → 哈哈哈, 好好好, …`. Built lazily on first miss
/// of full-pinyin lookup. Shared across all `PinyinAdapter` instances
/// in the same process via `Arc` so only one process pays the ~1-2s
/// build cost.
///
/// Storage (v1.6.x compaction, A/B'd via scripts/bench-auto.sh against
/// 089e026-baseline3): single flat byte pool + embedded FSA. Replaces
/// the prior `HashMap<String, Vec<String>>` which held ~400k separately-
/// allocated `String` items (`malloc_history --callTree` showed it as
/// the single biggest heap contributor at ~20 MB / 60% of total Rust
/// heap on this PinyinAdapter::warmup chain).
static INITIALS_INDEX: OnceLock<Arc<InitialsIndex>> = OnceLock::new();

/// Compact replacement for `HashMap<String, Vec<String>>`. All matching
/// words across every initials bucket are stored back-to-back in
/// `word_pool` as `[u16 length-LE][utf-8 bytes...]` records. The `fsa`
/// (`inputx_fsa::Fsa`) maps initials_bytes → packed `(first_word_offset
/// in low 32 bits | count in high 32 bits)`, both addressing the pool.
/// Lookups return an iterator that walks the pool slice in place — no
/// heap allocation per `next()`. Consumer copies to `String` only when
/// it needs an owned value.
struct InitialsIndex {
    word_pool: Vec<u8>,
    fsa_bytes: Vec<u8>,
}

struct InitialsMatches<'a> {
    pool: &'a [u8],
    offset: usize,
    remaining: usize,
}

impl<'a> Iterator for InitialsMatches<'a> {
    type Item = &'a str;
    fn next(&mut self) -> Option<&'a str> {
        if self.remaining == 0 { return None; }
        if self.offset + 2 > self.pool.len() { return None; }
        let len = u16::from_le_bytes([self.pool[self.offset], self.pool[self.offset + 1]])
            as usize;
        let start = self.offset + 2;
        let end = start + len;
        if end > self.pool.len() { return None; }
        self.offset = end;
        self.remaining -= 1;
        std::str::from_utf8(&self.pool[start..end]).ok()
    }
}

impl InitialsIndex {
    fn get(&self, initials: &[u8]) -> Option<InitialsMatches<'_>> {
        let fsa = inputx_fsa::Fsa::new(&self.fsa_bytes[..]).ok()?;
        let value = fsa.get(initials)?;
        let first_offset = (value & 0xFFFF_FFFF) as u32 as usize;
        let count = (value >> 32) as u32 as usize;
        Some(InitialsMatches {
            pool: &self.word_pool,
            offset: first_offset,
            remaining: count,
        })
    }
}

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
    /// Viterbi-derived sentence composition for long buffers (>= 6 chars).
    /// `Some(word)` when `PinyinDict::best_composition` returned a
    /// segmentation covering the whole buffer; `None` for short buffers
    /// or no-valid-cover cases. Surfaced at the top of `candidates_with_
    /// scores` with a fixed high score so the composed sentence is the
    /// default pick when the user types something long-and-pinyin-shaped
    /// like `nihaomawojiao`.
    composed_sentence: Option<String>,
    /// Candidates that came from fuzzy-pinyin expansion (z↔zh, c↔ch,
    /// s↔sh, etc. on the buffer prefix). Tracked so
    /// `candidates_with_scores` can apply a score discount — a fuzzy
    /// match is plausibly what the user meant but shouldn't beat an
    /// exact match in mixed lists.
    ///
    /// v1.8.0 WU-ν: value is the `inputx_phonetic_edit::edit_distance`
    /// from the typed buffer to the variant that matched in the dict.
    /// Path 1b (fuzzy variants) emits the real distance so closer
    /// typos rank higher than distant ones at the same dict head.
    /// (Pre-v1.8.1 Path 1a initials also lived here at a fixed 0.3;
    /// v1.8.1 carved initials out into [`initials_candidates`].)
    fuzzy_candidates: HashMap<String, f64>,
    /// v1.8.1 WU-ξ: candidates that arrived via Path 1c initials-
    /// shorthand lookup (`zg → 中国` style). Value is the
    /// `(typed_len, full_len)` pair driving
    /// `MatchType::Initials { typed_len, full_len }`'s proximity
    /// decay — `typed_len = consonant_prefix.len()` (how many initial
    /// letters the user typed), `full_len = word.chars().count() * 4`
    /// (estimated full pinyin length, average syllable ≈ 4 ASCII).
    initials_candidates: HashMap<String, (u8, u8)>,
    /// The Path 5 last-resort Viterbi composition (short non-lexeme buffer
    /// with no other candidate — e.g. `kaopu`→靠谱). `Some` only when that
    /// fallback fired. Scored in `candidates_with_scores` at
    /// `COMPOSED_FALLBACK_SCORE` — above mechanical JP kana but below a real
    /// dict word — so a composed-from-real-chars word outranks かおぷ-style
    /// kana transliterations in Mixed+JP, yet never beats a true match.
    fallback_composition: Option<String>,
    /// Path-3 prefix-completion scored entries (v1.3 WU-α CP-B).
    /// `word → predict_score(LIKELIHOOD_PINYIN_PREDICT_BASE, freq,
    /// PRIOR_FREQ_MULT_PINYIN, proximity)` where proximity =
    /// `self.buffer.len() / full_pinyin_code.len()`. Populated by
    /// `push_prefix_top_k` for multi-letter prefix scans only; single-letter
    /// prefix path (cached) stays on the NON_EXACT_FLOOR floor (proximity
    /// would be ~0.1, predict signal negligible). `candidates_with_scores`
    /// looks up here before falling through to the floor — so
    /// `zho → 中国` lands at ~250k visible across engines, while staying
    /// gated on `allow_prefix_completion = !has_non_speculative_candidate`
    /// so `lianxiang → 联想` exact match is untouched (2026-05-22 user rule).
    prefix_scored: HashMap<String, f64>,
    /// WU-γ parallel decomposition map: `word → ScoreComponents` for the
    /// same CP-B prediction entries that populate `prefix_scored`. Kept
    /// alongside (not folded in) so the existing f64 score read-path
    /// stays a single map lookup; `candidates_with_scores` zips both
    /// to emit `Scored` tuples carrying the (base, prior, likelihood)
    /// view for `inputx-probe`.
    prefix_components: HashMap<String, super::merge::ScoreComponents>,
}

impl Default for PinyinAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl PinyinAdapter {
    /// Build a snapshot Context for the rule engine. Cheap (one
    /// String clone of the buffer + 6 booleans). Called once per
    /// refresh_candidates invocation.
    ///
    /// Mode is fixed to `Mixed` here because the rule engine treats
    /// `Context::mode` as a hint for mode-gated rules; the actual
    /// per-mode routing happens in `dispatch::dispatch`. The adapter
    /// itself doesn't know which composite mode it's running under,
    /// so Mixed is the "all rules eligible" placeholder. Once the
    /// engine registry grows mode-specific rules, this signature
    /// will gain a `mode: Mode` parameter from the caller.
    fn build_rule_context(&self) -> Context {
        let has_vowel = self.buffer.chars()
            .any(|c| matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'v'));
        let starts_with_z = self.buffer.starts_with('z');
        let buffer_len = self.buffer.len();
        let has_ns = self.has_non_speculative_candidate;
        Context {
            mode: Mode::Mixed,
            buffer: self.buffer.clone(),
            prev_committed: None, // future rules may want this
            second_prev_committed: None,
            flags: ContextFlags {
                has_vowel,
                has_non_speculative_pinyin: has_ns,
                pinyin_intent: buffer_len > 0 && buffer_len <= 4
                    && has_vowel && has_ns,
                starts_with_z,
                buffer_len,
            },
        }
    }

    pub fn new() -> Self {
        Self {
            engine: PinyinEngine::new(),
            buffer: String::with_capacity(16),
            candidates: Vec::with_capacity(16),
            has_non_speculative_candidate: false,
            composed_sentence: None,
            fuzzy_candidates: HashMap::new(),
            initials_candidates: HashMap::new(),
            fallback_composition: None,
            prefix_scored: HashMap::new(),
            prefix_components: HashMap::new(),
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
        // Touch BOTH the facade dict (for prefix_exists /
        // best_composition / top_k_compositions hot paths that still
        // ride the inputx-pinyin FSA + value table) AND the cement-
        // owned IdfReader (for the exact / fuzzy / prefix-prediction
        // fills that v1.4.7 A4 cut over to EMBEDDED_PINYIN_IDF). Two
        // backing files; two distinct sets of pages to fault in.
        let mut buf: Vec<String> = Vec::with_capacity(32);
        let reader = pinyin_idf_reader();
        for seed in &[
            "a", "k", "p", "ni", "hao", "kp", "shi", "wo", "zhongguo", "h", "z", "ma",
        ] {
            self.engine.dict().lookup_into(seed, &mut buf);
            let _ = reader.lookup(seed.as_bytes());
        }
        // Force INITIALS_INDEX construction (otherwise first 简拼 query
        // pays a ~1-2s OneLock::get_or_init build). After this, any
        // 简拼 lookup is O(1) HashMap hit.
        let _ = initials_index(&self.engine);
        // Pre-warm the single-letter prefix cache for the heavy hitters
        // (`z` ~50k entries, `h` ~30k, `s` and `j` are the next biggest).
        // First-keystroke cost otherwise: 4-7ms scan; cached: microseconds.
        for c in ['z', 'h', 's', 'j', 'x', 'c', 'q', 'b', 'p', 'm'] {
            let _ = single_letter_cache(c);
        }

        // v1.6.6 IO/mem-pressure hardening (user 2026-05-31 raised:
        // "之前 IO / mem 占用大的时候卡得不行"). The dict blobs are
        // `include_bytes!`-baked into the binary's .rodata; macOS
        // treats them as on-disk pages that can be evicted under
        // memory pressure and re-faulted on next access — 10-50ms
        // stalls per page on a busy system. Force-touch one byte
        // per 4 KB page on each blob so the OS keeps them in the
        // working set across pressure events.
        //
        // Cost: ~1 byte read per page × ~3000 pages total across the
        // 5 embedded blobs ≈ a few hundred µs at startup. Pays for
        // itself on the first IO-pressure spike.
        warm_embedded_blob(EMBEDDED_BIGRAMS_NGM);
        warm_embedded_blob(EMBEDDED_PINYIN_IDF);
        // Build the inputx-ngram `OnceLock<HashMap>` ctx index now —
        // first `log_prob` call otherwise pays ~10-20ms to walk the
        // 64k-entry triplet table and bucket by ctx (per 7534b33's
        // lazy-index commit). Driving one realistic bigram query
        // forces the build; subsequent lookups are O(1).
        let _ = embedded_bigrams_table().log_prob(&["的"], "");
    }

    pub fn buffer_str(&self) -> &str {
        &self.buffer
    }

    /// Direct access to the underlying engine — used by the composite
    /// layer to call into PinyinDict methods (bigram_boost,
    /// predict_next_words, etc.) that the adapter doesn't otherwise
    /// proxy.
    pub fn engine(&self) -> &PinyinEngine {
        &self.engine
    }

    pub fn candidates(&self) -> &[String] {
        &self.candidates
    }

    /// `true` iff the current candidate list includes at least one
    /// candidate from Path 1 (exact-syllable lookup) — i.e., the user's
    /// buffer parses as one or more valid pinyin syllables AND there's
    /// a corpus entry at that exact reading. Used by the composite
    /// dispatch to demote wubi-defused tail candidates when pinyin
    /// clearly has the right answer (see dispatch.rs).
    pub fn has_exact_match(&self) -> bool {
        self.has_non_speculative_candidate
    }

    /// Scored variant of `candidates()`. Returns the current candidate
    /// list paired with each entry's unified score (see
    /// `inputx_pinyin::PinyinDict::lookup_with_scores_into` for the score
    /// formula), optionally enriched with a context-aware bigram bonus.
    ///
    /// `prev_committed` is the user's most-recently-committed word in
    /// this session (composite-layer state — adapter is stateless re:
    /// session). When `Some`, each candidate's score gets a positive
    /// additive bonus from `PinyinDict::bigram_boost(prev, candidate)`,
    /// which lifts candidates that frequently follow `prev` in the
    /// training corpus (e.g. after committing 今天, candidates 是/的/
    /// 我们 jump because of high bigram counts).
    ///
    /// Implementation: re-scores the existing `self.candidates` Vec.
    /// Path 1 (exact lookup) is scored via the dict's
    /// `lookup_with_scores_into`. Paths 2/3 (initials + prefix
    /// completion) inject candidates that wouldn't otherwise have a
    /// freq; for those we default to a low score so the cross-engine
    /// sort puts them below exact matches. Bigram bonus applies to all
    /// candidates (exact + non-exact).
    pub fn candidates_with_scores(
        &self,
        prev_committed: Option<&str>,
    ) -> Vec<super::merge::Scored> {
        if self.candidates.is_empty() || self.buffer.is_empty() {
            return Vec::new();
        }
        // Score exact-match entries via the dict; everything else
        // (initials + prefix-completion injected entries) gets a small
        // floor so the cross-engine merge still ranks them.
        let mut scored: Vec<super::merge::Scored> =
            Vec::with_capacity(self.candidates.len());
        // v1.4.7 sub-phase A4 step 1: exact-match fill reads through
        // the cement-owned IdfReader over EMBEDDED_PINYIN_IDF. The
        // FST code index makes each lookup O(|buffer|) and the
        // returned `Entry` already carries `log_prior_q4` (Q4 fixed-
        // point ln(1+freq) baked at build time — sub-phase B1 absorb
        // of `prior_correction` lands here too), so the cement-side
        // path no longer runs `log_prior_from_freq` per candidate.
        //
        // Legacy f64 `score` is now a deterministic tiebreaker only;
        // v1.4.7 A3 cut the primary sort key to `score_q4`. The
        // estimated_freq round-trip drift (~0.5%) that blocked the
        // v1.4.6 C3 attempt no longer flips ranking.
        const PINYIN_PHRASE_BASE: f64 = inputx_scoring::consts::PINYIN_PHRASE_BASE;
        const L0_PIN_MULTIPLIER: f64 = inputx_scoring::consts::L0_PIN_MULTIPLIER;
        // Normalize buffer for FST/IDF queries — collapses lue/nue
        // alias spellings to lve/nve so the embedded IDF (keyed under
        // lve/nve) is reachable from a user who typed lue/nue. The
        // PinyinDict methods normalize internally; the cement IDF
        // reader is a separate flat byte store with no normalization
        // of its own, so the call site has to feed it the canonical
        // key. See `inputx_pinyin::normalize_lookup_key` for the rule.
        let lookup_buf = inputx_pinyin::normalize_lookup_key(&self.buffer);
        let exact_entries = pinyin_idf_reader().lookup(lookup_buf.as_bytes());
        // L0 pin lookup — cement-level state, intentionally orthogonal
        // to the corpus snapshot in EMBEDDED_PINYIN_IDF.
        let pinned: Option<String> = self
            .engine
            .dict()
            .pinned_word(&self.buffer);
        // Build (word, legacy_score) map + parallel (word, log_prior_q4,
        // log_likelihood_q4) map so downstream chains can compose either.
        let mut exact_map: std::collections::HashMap<String, f64> =
            std::collections::HashMap::with_capacity(exact_entries.len());
        let mut exact_components: std::collections::HashMap<String, super::merge::ScoreComponents> =
            std::collections::HashMap::with_capacity(exact_entries.len());
        for entry in &exact_entries {
            let word = entry.word;
            let pin_mult = if pinned.as_deref() == Some(word) {
                L0_PIN_MULTIPLIER
            } else {
                1.0
            };
            // Legacy f64 (post-A3 tiebreaker): byte-equivalent to the
            // historical PinyinDict::lookup_with_scores_into output —
            // raw_freq is now carried losslessly in the IDF entry
            // (v1.4.7 A4 step 1 schema bump, replaced the unused
            // bigram_offset slot). When two entries land in the same
            // Q4 log_prior bucket (e.g. 乎/护 both quantize to 170 at
            // code `hu`), raw_freq still distinguishes them — fixes the
            // ranking inversion that estimated_freq_from_log_prior's
            // inverse would otherwise collapse.
            let legacy_score = (PINYIN_PHRASE_BASE + entry.raw_freq as f64) * pin_mult;
            // Orthodox Q4 log decomposition — `log_prior_q4` read
            // straight from IDF (no round-trip back through
            // `log_prior_from_freq`), so the only quantization point
            // is the writer's `Q4·ln(1+freq).round()` at .idf build.
            let log_prior_q4 = entry.log_prior as i32;
            let likelihood_linear = PINYIN_PHRASE_BASE * pin_mult;
            let log_likelihood_q4 = (likelihood_linear
                .max(1.0)
                .ln()
                * inputx_scoring::Q4 as f64)
                .round() as i32;
            exact_map.insert(word.to_string(), legacy_score);
            // WU-ψ tier assignment for pinyin exact-match candidates
            // Phase B (2026-06-03 PLAN-tier-by-quantile §3.1):
            //   - pinned    → 0 (user assertion)
            //   - otherwise → z-score quantile via raw_freq
            //                 (inputx_scoring::pinyin_tier_from_freq)
            //
            // Pre-Phase-B: hard cutoff `raw_freq >= 20_000 → tier 1` left
            // mo / shi / zhi / yi buffers with 21-58 tier-1 candidates each,
            // burying nihongo top-tier basic-kana candidates (も, モ) at
            // rank 22+.  See `.claude/PLAN-tier-by-quantile-spike-data.md`
            // for the spike that fixed (μ, σ) + per-tier z thresholds.
            //
            // PHRASES (word_chars >= 2) share the SAME z-score function as
            // single chars.  Pre-Phase-B-pass-2 we tried `phrase → tier 1`
            // unconditional, but that promoted every low-freq jieba sub-word
            // (馀额 / 皮袄 / 喜恶 / 图案 / 尼昂 / 密哦 ...) above legitimate
            // top single chars.  Real common phrases (我们 freq=54252 → z=2.58
            // → tier 1) keep their tier 1; corpus noise sub-words (z<2.5)
            // fall into tier 2-5 and let single-char tops surface.
            //
            // Phase 5: per-(buffer, word) tier_overlay.tsv can override
            // any of these natural tiers (e.g. `juti 具体 0`).
            //
            // Phase E (2026-06-03) — bigram quality gate for 2-char
            // phrases: when the (char1, char2) intra-bigram boost is
            // below the floor, the phrase is likely jieba over-
            // segmentation noise (馆里 21k / 局里 17k / 剧里 14k vs
            // real 这里 50k / 公里 50k / 居里 33k) and gets a TWO-tier
            // demote so the noise phrase sinks behind both real
            // tier-2 phrases AND real tier-3 candidates at the same
            // buffer.  1-tier demote was insufficient because thin-
            // candidate buffers (guanli has only 管理 in tier 2) left
            // the demoted noise still leading tier-3 by freq.
            //
            // 3+ char phrases unchanged — sub-word bleed is a 2-char
            // problem (jieba's over-segmentation pattern is mostly
            // "noun + locative", "noun + verb", etc. at 2 chars).
            let mut natural_tier: u8 = if pinned.as_deref() == Some(word) {
                0
            } else {
                inputx_scoring::pinyin_tier_from_freq(entry.raw_freq.into())
            };
            let mut chars = word.chars();
            let (c1, c2, c3) = (chars.next(), chars.next(), chars.next());
            if c3.is_none() {
                if let (Some(a), Some(b)) = (c1, c2) {
                    let s1 = a.to_string();
                    let s2 = b.to_string();
                    let bg = self.engine.dict().bigram_boost(Some(&s1), &s2);
                    // Combo gate: phrase looks like jieba over-segmentation
                    // ONLY when bigram is low AND freq sits in the inflation
                    // BAND.  Above ceil = real-common (屋里 32k / 这里 45k)
                    // OR user-attested quickfix (锚定 40k).  Below floor =
                    // real-rare (靠谱 10k / 铆钉 15k — consistent low-low).
                    // jieba over-segment sweet spot is the mid band 22k-30k:
                    // freq looks "common-ish" but bigram says the chars don't
                    // actually co-occur in real text → over-segmentation.
                    let f = u64::from(entry.raw_freq);
                    if bg < inputx_scoring::consts::PHRASE_BIGRAM_SIGNAL_FLOOR
                        && f >= inputx_scoring::consts::PHRASE_INFLATION_FLOOR_FREQ
                        && f <  inputx_scoring::consts::PHRASE_INFLATION_CEIL_FREQ
                    {
                        natural_tier = (natural_tier + 2).min(9);
                    }
                }
            }
            let tier_pinyin: u8 = inputx_scoring::tier_overlay::get(
                &self.buffer,
                word,
            ).unwrap_or(natural_tier);
            exact_components.insert(
                word.to_string(),
                super::merge::ScoreComponents::three_axis_tiered(
                    log_prior_q4,
                    log_likelihood_q4,
                    inputx_scoring::MatchType::Exact,
                    tier_pinyin,
                ),
            );
        }
        // Floor for non-exact (initials / prefix-completion) entries —
        // sits below the lowest natural exact-match score so exact
        // matches dominate. 1k chosen as "any positive but tiny".
        const NON_EXACT_FLOOR: f64 = inputx_scoring::consts::NON_EXACT_FLOOR;
        // Decay non-exact entries by position so the original within-
        // path ordering is preserved at the bottom of the merged list.
        let dict = self.engine.dict();
        // Composed-sentence score: chosen to sit just above a typical
        // top single-phrase score (~480k = 400k base + ~80k freq) so
        // the Viterbi result wins #0 for long buffers, but stays well
        // below wubi simcodes (~600k-1M) so simcodes can still take
        // priority when both engines have a strong claim.
        const COMPOSED_SCORE: f64 = inputx_scoring::consts::COMPOSED_SCORE;
        // Path 5 last-resort composition (kaopu→靠谱). Sits ABOVE mechanical
        // JP kana (scoring::LIKELIHOOD_JP_HIRAGANA_BASE = 150k, katakana 110k, +freq×3k
        // — but mechanical renders carry freq 0) so a word composed from real
        // single chars beats a かおぷ-style transliteration in Mixed+JP, while
        // staying BELOW any real pinyin dict word (~445k), wubi 简码 (600k–1M)
        // and the long-buffer COMPOSED_SCORE. Path 5 only fires when pinyin
        // itself is empty, so this never leapfrogs a real pinyin candidate.
        // Real common words (靠谱/榨干) belong IN the dict (coverage —
        // dict-pipeline T0); once there they score as real words, above this.
        const COMPOSED_FALLBACK_SCORE: f64 = inputx_scoring::consts::COMPOSED_FALLBACK_SCORE;
        // Fuzzy-match discount: a candidate that only matched after
        // initial-prefix fuzzy expansion (z↔zh, f↔h, etc.) is a typo-
        // correction guess — the user typed something close to, but not
        // the same as, a real dict entry. User rule 2026-05-27 ("你都
        // 打错了，有就不错了") puts fuzzy at the BOTTOM of the tier
        // ordering: above NON_EXACT_FLOOR (1k) so it stays visible, but
        // BELOW prediction (CP-B, base 180k + decay → 180-230k range)
        // and BELOW JP exact whole-buffer (240k). User-reported: `fami`
        // surfaced 哈密 / 哈米 (fuzzy of `hami` via f↔h swap) above
        // ファミ — the typo-correction guess outranked the JP exact
        // match. Calibration: FUZZY_BASE * FUZZY_DISCOUNT = 350k * 0.3
        // = 105k, comfortably below prediction min 180k and JP exact
        // 240k.
        const FUZZY_DISCOUNT: f64 = inputx_scoring::consts::FUZZY_DISCOUNT;
        // Fuzzy candidates need a synthetic base if they have no exact
        // dict entry at the typed buffer — they DO have an entry at the
        // fuzzy-variant buffer (`zhongguo` for typed `zongguo`), but
        // exact_map (built from `lookup_with_scores_into(self.buffer)`)
        // only sees the typed-buffer entries. Give them a mid-tier base.
        const FUZZY_BASE: f64 = inputx_scoring::consts::FUZZY_BASE;
        // Composition base. Junk compositions (是嗯据库) are already dropped at
        // generation by the per-char quality gate in refresh_candidates, so a
        // composed_sentence reaching here is good. When an exact full-buffer
        // dict word exists, a forced segmentation (用中 for yongzhong) is still
        // lower confidence than the real phrase (臃肿) → drop it just below the
        // lowest exact score; otherwise keep the high COMPOSED_SCORE so a real
        // sentence wins #0 (nihaomawojiao→你好吗我叫).
        let composed_base = if exact_map.is_empty() {
            COMPOSED_SCORE
        } else {
            let min_exact = exact_map.values().copied().fold(f64::INFINITY, f64::min);
            (min_exact - 1.0).min(COMPOSED_SCORE)
        };
        // v1.4.7 A2 step 4 orthodox decomposition: each path emits its
        // (log_prior_q4, log_likelihood_q4, match_type) directly at the
        // source. The exact and CP-B prediction paths already carry an
        // upstream-built decomposition (raw freq → log_prior, base+proximity
        // → log_likelihood) via `exact_components` / `prefix_components`.
        // The remaining 4 paths — composed_sentence, Viterbi fallback,
        // fuzzy variants, NON_EXACT_FLOOR — have no raw corpus freq
        // (they're mechanical / typo-tolerant / degenerate); their
        // `base` is purely a likelihood signal (engine's confidence in
        // *this kind* of match), so log_prior_q4 = 0 by construction
        // and log_likelihood_q4 = Q4·ln(base). Pattern mirrors wubi/
        // pinyin exact-path A2 step 1+2 (36b9d3d) and nihongo composed
        // A2 step 3 (da91f5b): pure-data axis emitted at the source,
        // no synth helper indirection.
        //
        // The bigram_bonus is added AFTER and not folded into
        // components — it's a cross-engine context signal, not part of
        // P(i|W) for this adapter.
        let to_log_q4 = |s: f64| -> i32 {
            (s.max(1.0).ln() * inputx_scoring::Q4 as f64).round() as i32
        };
        // v1.7.4: synthetic fill sites (composed-sentence, Path-5
        // fallback, fuzzy, NON_EXACT_FLOOR) have no raw corpus freq —
        // pre-v1.7.4 they set `log_prior_q4 = 0` which meant "log(1+0) =
        // no signal" in the unnormalized world. Under real
        // log-probability semantics, `log_prior_q4 = 0` means "log P(W)
        // = 0 → probability 1" which makes synthetic candidates dominate
        // the merge — the opposite of intent. The corpus-floor
        // `log_prob_corpus_from_freq(0, total)` ≈ `-Q4·ln(1+total)` is
        // the new "no signal" baseline: a freq-0 entry under the
        // pinyin corpus, placing the synthetic at the worst possible
        // prior. This restores the legacy below-real-entries ranking
        // for synthetic candidates without per-site bespoke offsets.
        let pinyin_floor = inputx_scoring::log_prob_corpus_from_freq(
            0,
            inputx_pinyin_helpers::pinyin_corpus_total(),
        );
        for (i, w) in self.candidates.iter().enumerate() {
            let is_composed = Some(w.as_str()) == self.composed_sentence.as_deref();
            let is_fallback = Some(w.as_str()) == self.fallback_composition.as_deref();
            let fuzzy_edit_distance: Option<f64> = self.fuzzy_candidates.get(w).copied();
            let _is_fuzzy = fuzzy_edit_distance.is_some();
            let initials_lens: Option<(u8, u8)> = self.initials_candidates.get(w).copied();
            let (base, components): (f64, Option<super::merge::ScoreComponents>) =
                if is_composed {
                    // A composition that coincides with a real exact dict word
                    // (zhongguo→中国, women→我们) keeps its real exact score — it
                    // is a genuine word, not forced junk. Only a segmentation
                    // that is NOT itself a dict word (用中 for yongzhong, 是嗯据库
                    // for shinjuku) drops to composed_base, below every exact word.
                    let s = exact_map.get(w).copied().unwrap_or(composed_base);
                    // v1.4.7 A3 step 4a: when the composed-top coincides with a
                    // real exact dict entry, reuse `exact_components` — those
                    // carry the raw-freq log_prior_q4 the composed-only path
                    // can't reconstruct. Without this, `score_q4` for the
                    // exact-coinciding composition is `0 + Q4·ln(s)`, missing
                    // the entire prior axis, and ranks below mechanical
                    // Viterbi segmentations (种过, 生火, 点映 for zhongguo /
                    // shenghuo / dianying) that scored lower in legacy f64
                    // but identical in q4 once the prior is dropped. Only
                    // fall back to synthetic `three_axis(0, …)` when w is a
                    // forced segmentation (not a dict word).
                    let c = exact_components.get(w).copied().unwrap_or_else(|| {
                        let mt = inputx_scoring::MatchType::Composed { bigram_links: 1 };
                        // Phase F (2026-06-04): composed-Viterbi
                        // segmentations that are NOT themselves a
                        // dict word → tier 5 (less_common).
                        //
                        // User report 2026-06-04: "为什么组合词评分会
                        // 这么高，这个评分当时做的不对，我还想不到
                        // 任何一个组合词需要高分的，都是作为填充物的".
                        //
                        // Pre-Phase-F (WU-ψ): with_tier(1) so a real
                        // Chinese sentence (nihaomawojiao → 你好吗我叫)
                        // wins #0 over mechanical JP renderings.  But
                        // that same tier 1 let 2-char jieba sub-words
                        // (changshi → 长时 via 长+时 bigram, shoumai →
                        // 收卖, etc.) pre-empt real dict tier-2
                        // phrases (changshi → 尝试 freq 35k z=2.0).
                        //
                        // Post Phase C-3 (2026-06-04), 5+ char JP
                        // mechanical kana is tier 4 — Composed at
                        // tier 5 still surfaces in PinyinOnly top-10
                        // (no other engine to compete), and in
                        // Mixed+JP only when no dict candidate exists
                        // (sparse pool).  Exact dict words (你好/中国
                        // /用不了 ARE in library via the coinciding-
                        // exact branch above) keep their natural
                        // z-score tier.
                        super::merge::ScoreComponents::three_axis(pinyin_floor, to_log_q4(s), mt)
                            .with_tier(5)
                    });
                    (s, Some(c))
                } else if is_fallback {
                    // Path 5 last-resort Viterbi compose — no bigram support
                    // (gated to short buffers where no real composition fits).
                    let mt = inputx_scoring::MatchType::Composed { bigram_links: 0 };
                    // Phase G (2026-06-03): Path 5b fallback → tier 8
                    // (speculative band).  User report 2026-06-03 akashi:
                    // "阿卡是 不是一个应该出现的东西... 同情况都要处理掉".
                    //
                    // The original WU-ψ rationale ("tier 1 lifts it above
                    // JP basic kana for kaopu") doesn't hold post-治理:
                    //   - kaopu 靠谱 is now an Exact dict entry (library
                    //     freq=10666 → tier 3 via Phase B z-score),
                    //     leads JP mechanical kana via tier ordering
                    //     without needing the fallback path
                    //   - The remaining real fallback fires (akashi →
                    //     阿卡是, etc.) are buffers that DON'T have a
                    //     valid Chinese composition; user typed JP/foreign
                    //     and got a mechanically-forced segment
                    //
                    // Tier 8 = speculative (per RANKING-MODEL-INVARIANTS
                    // §1).  Path 5b candidate stays visible deep in the
                    // list but never dominates JP prediction (tier 7) or
                    // any dict-based candidate.
                    //
                    // Path 5 REAL composition (bigram_links ≥ 1) at line
                    // ~654 above keeps its tier 1 — composed sentences with
                    // bigram support ARE real Chinese input.
                    let c = super::merge::ScoreComponents::three_axis(
                        pinyin_floor, to_log_q4(COMPOSED_FALLBACK_SCORE), mt,
                    ).with_tier(8);
                    (COMPOSED_FALLBACK_SCORE, Some(c))
                } else if let Some(s) = exact_map.get(w).copied() {
                    // v1.4.7 A2 step 2: use the orthodox (log_prior_q4,
                    // log_likelihood_q4) decomposition built upstream
                    // from raw freq + PINYIN_PHRASE_BASE. exact_components
                    // is guaranteed to have the same key set as exact_map.
                    let c = exact_components.get(w).copied();
                    (s, c)
                } else if let Some(s) = self.prefix_scored.get(w).copied() {
                    // CP-B prediction hit: pull the decomposition from the
                    // parallel components map so probe can render it.
                    // Checked BEFORE fuzzy: when a word is reachable both as
                    // a mid-typing prediction (`famin*` → 发明 via prefix
                    // scan) AND as a typo-corrected fuzzy hit (`famin` ↔
                    // `faming` via in↔ing swap), the prediction reading is
                    // more confident (the user is mid-typing toward a real
                    // word) than the typo guess. User polish-log 2026-05-27:
                    // 发明 should rank as prediction for `famin`, not as
                    // bottom-tier fuzzy.
                    // WU-ψ: predictions → tier 7 (specialty).
                    let c = self.prefix_components.get(w).copied()
                        .map(|c| c.with_tier(7));
                    (s, c)
                } else if let Some((typed_len, full_len)) = initials_lens {
                    // v1.8.1 WU-ξ: initials shorthand has its own
                    // LIKELIHOOD tier (`MatchType::Initials`),
                    // distinct from fuzzy. The proximity decay is
                    // gentler than Prefix (K=1 vs K=3) — the user
                    // typed initial letters as an abbreviation, which
                    // is more confident than a multi-edit typo.
                    let weights = inputx_scoring::EngineWeights::inputx_default();
                    let mt = inputx_scoring::MatchType::Initials { typed_len, full_len };
                    let log_likelihood_q4 = inputx_scoring::derive_log_likelihood(
                        weights.initials_likelihood_base_q4,
                        mt,
                    );
                    // Legacy f64 `score` stays at NON_EXACT_FLOOR-
                    // tier — pre-v1.8.1 initials lived here, and the
                    // f64 field is the secondary tiebreaker only, so
                    // not disturbing it keeps the merge's tertiary
                    // ordering identical.
                    let s = NON_EXACT_FLOOR * 0.99f64.powi(i as i32);
                    // WU-ψ: initials shorthand → tier 8 (predict-tier,
                    // user typed letters that aren't a real syllable).
                    let c = super::merge::ScoreComponents::three_axis(
                        pinyin_floor, log_likelihood_q4, mt,
                    ).with_tier(8);
                    (s, Some(c))
                } else if let Some(distance) = fuzzy_edit_distance {
                    // v1.8.0 WU-ν: fuzzy candidates carry their real
                    // `inputx_phonetic_edit::edit_distance` from the
                    // typed buffer to the variant that hit. Map
                    // distance → `MatchType::Fuzzy(cost_milli)` and
                    // derive the log_likelihood via
                    // `EngineWeights::fuzzy_likelihood_floor_q4 +
                    // ln(1 − cost/1000)·Q4`. Closer typos pay less
                    // decay; the canonical fuzzy distance of 0.3
                    // (single zh↔z swap) reproduces the pre-v1.8
                    // flat-discount log_likelihood when cost_milli ≈
                    // 700.
                    //
                    // The legacy linear-space `score` field stays at
                    // `FUZZY_BASE * FUZZY_DISCOUNT` so the merge's
                    // f64-tiebreaker behavior is unchanged — only the
                    // Q4 log-likelihood (primary sort) responds to
                    // edit_distance.
                    let weights = inputx_scoring::EngineWeights::inputx_default();
                    // Linear distance → cost_milli mapping, clamped
                    // to 999 (the Fuzzy(1000) edge case would push
                    // ln(0) = -inf which the scoring crate caps
                    // upstream, but explicit min keeps the schema
                    // intent honest).
                    let cost_milli = ((distance * 1000.0).round() as i32)
                        .clamp(0, 999) as u16;
                    let mt = inputx_scoring::MatchType::Fuzzy(cost_milli);
                    let log_likelihood_q4 = inputx_scoring::derive_log_likelihood(
                        weights.fuzzy_likelihood_floor_q4,
                        mt,
                    );
                    let s = FUZZY_BASE * FUZZY_DISCOUNT;
                    // WU-ψ: fuzzy → tier 8 ("你都打错了，有就不错了").
                    let c = super::merge::ScoreComponents::three_axis(
                        pinyin_floor, log_likelihood_q4, mt,
                    ).with_tier(8);
                    (s, Some(c))
                } else {
                    // NON_EXACT_FLOOR tier — degenerate; mark Exact for
                    // schema purposes (these are dead-tier candidates
                    // ranking at the bottom of the list).
                    let s = NON_EXACT_FLOOR * 0.99f64.powi(i as i32);
                    // WU-ψ: degenerate dead-tier → tier 9 (longtail).
                    let c = super::merge::ScoreComponents::three_axis(
                        pinyin_floor, to_log_q4(s), inputx_scoring::MatchType::Exact,
                    ).with_tier(9);
                    (s, Some(c))
                };
            // v1.4.6 sub-phase C2 cutover: source the bigram bonus
            // from the NGMv1 cement table (data/private-dict/v0.0.1/
            // pinyin/bigrams.ngm, embedded via inputx-pinyin-cement)
            // instead of the facade's PinyinDict::bigram_boost (which
            // reads inputx-pinyin's bundled bigrams.fsa). Same Q4
            // log-prob source under the hood (PinyinDict::iter_bigrams
            // generates .ngm by summing inter+intra FSTs); cement's
            // `legacy_bigram_boost_from_ngm` inverts Q4 log → est count
            // → re-applies the v1.3 calibration formula so the legacy
            // f64 sort key sees comparable values during the cutover
            // window. Sort-key proper cutover (legacy f64 → Q4 log
            // additive) follows in sub-phase C3.
            let _ = dict; // dict still in scope for path 5 below; ack the unused legacy reader.
            let bigram_bonus = legacy_bigram_boost_from_ngm(
                embedded_bigrams_table(),
                prev_committed,
                w,
            );
            // v1.8.2 WU-ο: bigram boost flows into log_likelihood_q4
            // too — the Q4 sort key (primary) finally sees the same
            // bigram signal the legacy f64 (tiebreaker) has had since
            // v1.3. Pre-v1.8.2 these axes diverged for any candidate
            // following a `prev_committed`, so the Q4 ranking
            // disagreed with the f64 ranking exactly when bigram
            // context mattered most. `bigram_boost_from_ngm` returns
            // an i16 in Q4 log-space (`Q4 · ln(count)`), zero when
            // `prev_committed` is None or the pair is unseen — so
            // cold-session ranking is unchanged.
            let bigram_q4 = bigram_boost_from_ngm(
                embedded_bigrams_table(),
                prev_committed,
                w,
            ) as i32;
            let components = components.map(|mut c| {
                c.log_likelihood_q4 = c.log_likelihood_q4.saturating_add(bigram_q4);
                c
            });
            scored.push((w.clone(), base + bigram_bonus, components));
        }
        scored
    }

    /// User-pinned word for the *current* pinyin buffer, if any. Used by
    /// composite::engine to apply cross-engine pin promotion: when the
    /// user has explicitly trained `jixu → 继续`, the merged candidate
    /// list should surface 继续 at position 0 even though wubi-3-char-
    /// phrase coincidence (曳光弹 also encodes to `jixu`) would
    /// structurally push 曳光弹 ahead of pinyin in the merge.
    pub fn pinned_word_for_buffer(&self) -> Option<String> {
        if self.buffer.is_empty() {
            return None;
        }
        self.engine.dict().pinned_word(&self.buffer)
    }

    /// `true` if the current buffer is a prefix of at least one word in
    /// Path 1c (initials-fallback typo rescue) eligibility check.
    /// Returns the 2-letter consonant prefix when the gate fires:
    ///
    ///   - no non-speculative candidate yet (room to add one)
    ///   - buffer.len() ∈ [4, 5] (Phase H cap)
    ///   - buffer is NOT a valid pinyin dict prefix
    ///   - 音节意识细化 gate: buffer does NOT have a clean ≥3-char
    ///     valid syllable prefix (if it does, the user committed
    ///     to that syllable and the trailing chars are mid-typing
    ///     junk — handled by Path 3b trim-retry instead, see
    ///     `docs/PLAN-syllable-aware-pinyin.md`)
    ///   - prefix-up-to-first-vowel is exactly 2 consonants
    ///   - suffix length ≥ 2 (so the gate looks typo-shaped, not
    ///     just a 2-letter input)
    ///
    /// Used by:
    ///   - Path 1c itself (the actual lookup site below) to decide
    ///     whether to run.
    ///   - `composite/engine.rs::is_pure_garbage` to decide whether
    ///     to LET Path 1c run before ASCII-fallback wipes the buffer.
    ///     Without this gate `shehv` (5 chars, 'v' not a syllable
    ///     starter) used to wipe in JP-off mode because
    ///     `has_future_match` returns false for it — verified
    ///     2026-06-05.
    pub(crate) fn path1c_consonant_prefix(&self) -> Option<String> {
        if self.has_non_speculative_candidate {
            return None;
        }
        if !(4..=5).contains(&self.buffer.len()) {
            return None;
        }
        if self.engine.dict().prefix_exists(&self.buffer) {
            return None;
        }
        // 音节意识细化 (2026-06-06): if the buffer starts with a
        // ≥3-char valid syllable, the user committed to that syllable
        // — Path 1c (designed for "missing-vowel-from-the-start"
        // typos) doesn't apply. Path 3b trim-retry will surface
        // continuations of the committed syllable instead. 2-char
        // syllables (he / ma / na / ...) are excluded from this gate
        // because they overlap with English-word starts (hello, may,
        // ...) and would false-block Path 1c on genuine English.
        if inputx_pinyin::longest_valid_syllable_prefix(&self.buffer)
            .map(|s| s.len())
            .unwrap_or(0)
            >= 3
        {
            return None;
        }
        let consonant_prefix: String = self.buffer.chars()
            .take_while(|c| !matches!(*c, 'a' | 'e' | 'i' | 'o' | 'u' | 'v'))
            .collect();
        let suffix_len = self.buffer.len() - consonant_prefix.len();
        if consonant_prefix.len() != 2 || suffix_len < 2 {
            return None;
        }
        // v1.14 (user report 2026-06-06 tkinn): for 5-char buffers
        // the suffix after the 2-consonant prefix must be a plausible
        // pinyin syllable tail — there must exist at least one valid
        // pinyin syllable ending with it. Without this check, `tkinn`
        // (suffix `inn`, length 3) happily triggers reverse-lookup of
        // every t-k-initials word (痛苦/天空/...) even though no
        // Chinese syllable ends with `inn`.
        //
        // 4-char buffers stay on the original "any 2-char suffix"
        // rule: `pyin` (suffix `in`), `xlab` (suffix `ab`) — both
        // legitimate typo / wubi-shape rescue cases the existing
        // tests pin. The added structural check kicks in only at
        // length 5, where the longer tail makes the corpus-shape
        // check meaningful.
        if self.buffer.len() == 5 {
            let suffix = &self.buffer[consonant_prefix.len()..];
            if !inputx_pinyin::VALID_SYLLABLES
                .iter()
                .any(|s| s.ends_with(suffix))
            {
                return None;
            }
        }
        Some(consonant_prefix)
    }

    /// 音节意识细化 (2026-06-06) — "buffer has a clean ≥3-char
    /// valid syllable prefix". Predicate shared between the Path 1c
    /// gate (which excludes such buffers from typo rescue) and
    /// Path 3b trim-retry (which produces candidates for them) and
    /// `is_pure_garbage` (which won't wipe such buffers).
    ///
    /// 3-char threshold rationale in
    /// `docs/PLAN-syllable-aware-pinyin.md` §3: 2-char syllables
    /// (he/ma/...) overlap with English-word starts, false positives
    /// like `hello → he+llo`; 3+ char syllables are unambiguously
    /// Chinese-shape.
    pub fn has_clean_syllable_prefix(&self) -> bool {
        inputx_pinyin::longest_valid_syllable_prefix(&self.buffer)
            .map(|s| s.len())
            .unwrap_or(0)
            >= 3
    }

    /// Same gate as `path1c_consonant_prefix` but returns just a bool,
    /// for composite-layer dispatch decisions where the prefix value
    /// itself isn't needed.
    pub fn path1c_would_fire(&self) -> bool {
        self.path1c_consonant_prefix().is_some()
    }

    /// the pinyin dict (i.e., the user could keep typing and land on a
    /// real pinyin word). Used by the composite engine to veto wubi
    /// auto-commit when pinyin's still building toward a multi-syllable
    /// word — exact-match candidates may be empty (e.g., `beij` has no
    /// stand-alone entry) but `prefix("beij")` returns `北京` etc.
    pub fn has_future_match(&self) -> bool {
        if self.buffer.is_empty() {
            return false;
        }
        // First-tier: exact prefix match in pinyin dict.
        if self.engine.dict().prefix_exists(&self.buffer) {
            return true;
        }
        // Second-tier: handle mid-typing of multi-syllable inputs.
        // User 2026-05-24 `yongbuliao → no candidates`: at intermediate
        // state "yongbul", dict has no entry with pinyin "yongbul*"
        // (final 'l' starts next syllable). Trim trailing 1-4 chars
        // AND require the SUFFIX to be a valid pinyin syllable PREFIX
        // (e.g. 'l' starts li/la/le; 'wxzy' doesn't start anything).
        // This distinguishes "user mid-typing yong+bu+l[iao]" (alive)
        // from "user typing garbage qwxzy" (dead).
        for trim in 1..=4.min(self.buffer.len() - 1) {
            let shorter = &self.buffer[..self.buffer.len() - trim];
            let suffix = &self.buffer[self.buffer.len() - trim..];
            if !suffix_could_start_syllable(suffix, inputx_pinyin::is_valid_syllable) {
                continue;
            }
            if self.engine.dict().prefix_exists(shorter) {
                return true;
            }
        }
        // Third-tier: Viterbi viability for medium+ buffers. If a
        // prefix of the buffer can be segmented by best_composition,
        // user is mid-typing a long composable string. Catches cases
        // where intermediate prefix isn't an exact dict-pinyin match
        // (e.g. "nihaomaw" — no dict word at that exact pinyin, but
        // "nihaoma" composes 你好吗 and the trailing "w" starts 我).
        // Threshold 6 = Viterbi's effective MIN_LEN floor + headroom.
        if self.buffer.len() >= 6 {
            for trim in 0..=3.min(self.buffer.len() - 4) {
                let shorter = &self.buffer[..self.buffer.len() - trim];
                if self.engine.dict().best_composition(shorter).is_some() {
                    return true;
                }
            }
        }
        false
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
        self.prefix_scored.clear();
        self.prefix_components.clear();
        true
    }

    pub fn clear_all(&mut self) {
        self.buffer.clear();
        self.candidates.clear();
        self.has_non_speculative_candidate = false;
        self.prefix_scored.clear();
        self.prefix_components.clear();
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
        self.prefix_scored.clear();
        self.prefix_components.clear();
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
    pub fn export_l0(&self) -> inputx_pinyin::L0Snapshot {
        self.engine.dict().export_l0()
    }

    /// L0 restore. Returns count of accepted pins.
    pub fn import_l0(&self, snap: inputx_pinyin::L0Snapshot) -> usize {
        self.engine.dict().import_l0(snap)
    }

    fn refresh_candidates(&mut self) {
        self.candidates.clear();
        self.has_non_speculative_candidate = false;
        self.composed_sentence = None;
        self.fuzzy_candidates.clear();
        self.initials_candidates.clear();
        self.fallback_composition = None;
        self.prefix_scored.clear();
        self.prefix_components.clear();
        if self.buffer.is_empty() {
            return;
        }

        // Path 0a (v3.0.2b: migrated to rule-engine).
        // RepeatedLetterExpansion in rules/builtin/repeated_letter.rs.
        // Engine output is read here and placed into the existing
        // composed_sentence slot — keeps every other path's logic
        // unchanged. Once all 7 pinyin paths migrate, composed_sentence
        // and self.candidates will both be rule-engine outputs.
        //
        // v1.14 (2026-06-06): user re-classified Path 0a into the
        // ASSOCIATION bucket — "重复字母不应该是简拼拼接出来的吗".
        // The 7-letter interjection table (h→哈 / a→啊 / o→哦 / e→诶
        // / m,n→嗯 / w→呜) is conceptually a typing-shortcut just like
        // Path 2 简拼, not "correct spelling".  Gated alongside Path 2
        // so flipping `PINYIN_DISABLE_ASSOCIATION` re-enables both at
        // once.
        if !PINYIN_DISABLE_ASSOCIATION {
            let ctx = self.build_rule_context();
            let mut rule_cands: Vec<RuleCandidate> = Vec::new();
            let _trace = candidate_rule_engine().run(&ctx, &mut rule_cands);
            if let Some(c) = rule_cands.into_iter()
                .find(|c| c.source == "repeated-letter")
            {
                self.composed_sentence = Some(c.word);
            }
        }

        // Path 0b (Viterbi composition): for LONG buffers (>= 8 bytes),
        // try to segment the whole input into a sequence of dict-matched
        // phrases. When it works, the composed string surfaces at the
        // top of the candidate list (see `candidates_with_scores`).
        //
        // 8 is the threshold for two reasons:
        //   1. Under 8, normal exact-phrase + prefix lookup already
        //      cover everything (e.g. nihao→你好 directly).
        //   2. Short Viterbi compositions can construct strings that
        //      LOOK like real dict words but at the wrong pinyin —
        //      e.g. `nuanhe` (6 chars) composes 暖+和 → "暖和", but
        //      the actual word 暖和 has pinyin "nuanhuo", not "nuanhe".
        //      Pushing this composition to #0 would be a wrong-reading
        //      false positive. Threshold 8 sidesteps this entirely:
        //      no real ambiguous short composition reaches it.
        if !PINYIN_DISABLE_COMPOSE
            && self.buffer.len() >= 8
            && let Some((score, sentence, chain))
                = self.engine.dict().best_composition_chain(&self.buffer)
        {
            // Two-tier quality gate:
            //
            // Tier 1 — per-char score floor (user 2026-05-26): a Viterbi
            // composition whose per-char score is too negative is junk —
            // 是嗯据库 (~−22k/char for the Japanese romaji `shinjuku`) vs
            // a real sentence ~−11k/char (你好吗我叫). Drop at generation
            // so it never becomes a candidate.
            const COMPOSED_QUALITY_FLOOR: f64 = inputx_scoring::consts::COMPOSED_QUALITY_FLOOR;
            let per_char = score / (self.buffer.chars().count().max(1) as f64);
            //
            // Tier 2 — cross-segment bigram support. Calibrate by chain
            // length (user reports: houxuanqu 2026-05-27, kakarimasu
            // 2026-06-02, luyaozhi 2026-06-06):
            //
            //   - 0 / 1 segments: trivially true (nothing to gate).
            //   - 2 segments (1 link): STRICT — the link MUST have
            //     bigram support (intra OR inter > 0). `候选去`
            //     mechanical concat where (候选, 去) is in neither
            //     table → drops. `候选词` where (候选, 词) > 0 → keeps.
            //   - 3+ segments (N-1 links): MAJORITY — at least
            //     ceil((N-1)/2) links must have non-zero combined
            //     bigram support. Half-coverage threshold means a real
            //     long sentence with a couple of corpus gaps still
            //     passes, but a mechanical force-segmentation where
            //     most pairs are zero fails. `卡-卡-日-马-苏`
            //     (kakarimasu, 4 links, 0 non-zero combined) → drops.
            //     `你好-吗-我-叫` (3 links, 0-1 non-zero) → drops too,
            //     which the user has confirmed is fine.
            //
            // v1.14 (user report 2026-06-06 luyaozhi → 路要职): the
            // strength signal is the MAX over intra (within-word
            // char-pair counts) and inter (cross-token transition
            // counts). Intra alone misses real-but-not-intra signals
            // like `(用, 不)` (count 28 intra, but cut by NGM top-N
            // filter — the embedded blob keeps high-count entries
            // only). Inter alone misses real intra signals like
            // `(要, 职)`. MAX surfaces the stronger of the two.
            //
            // The inter blob is filtered at build time to count ≥ 15
            // (`build-inter-bigrams-ngm --min-count 15`); the noise
            // pair `(路, 要)` count=12 is excluded by construction,
            // while the real pair `(用, 不)` count=16 survives. This
            // pushes calibration into the data pipeline so the runtime
            // check stays a clean `> 0`.
            let ngm_table = embedded_bigrams_table();
            let inter_table = embedded_inter_bigrams_table();
            let link_present = |a: &str, b: &str| -> bool {
                combined_bigram_log_prob_q4(ngm_table, inter_table, Some(a), b) > 0
            };
            let bigrams_ok = match chain.len() {
                0 | 1 => true,
                n => {
                    let links = n - 1;
                    let non_zero = (1..n)
                        .filter(|&i| link_present(&chain[i - 1], &chain[i]))
                        .count();
                    // v1.14 (luyaozhi 2026-06-06): strict-all — every
                    // link must be combined-present. The old ceil-
                    // half rule worked under intra-only data because
                    // the intra blob's top-N cut left most inter-
                    // token transitions at zero, so requiring half
                    // was effectively requiring most. With the inter
                    // blob added, common transitions like (我, 叫) /
                    // (好, 吗) all surface, turning ceil-half into a
                    // pass-everything gate. Strict-all restores the
                    // "real composition has structure end-to-end"
                    // semantic: noise like `[路, 要, 职]` (1/2
                    // combined) and `[你, 好, 吗, 我, 叫]` (3/4)
                    // drop, while clean compositions `[用, 不, 了]`
                    // (2/2) and `[是, 好, 好]` (2/2) survive.
                    let needed = links;
                    non_zero >= needed
                }
            };
            if per_char >= COMPOSED_QUALITY_FLOOR && bigrams_ok {
                self.composed_sentence = Some(sentence.clone());
                // Push immediately so it surfaces even when Path 1/2/3 all
                // return empty for this long buffer (yongbuliao 2026-05-24).
                self.candidates.push(sentence);
            }
        }

        // Path 1: exact-syllable lookup (含 fuzzy / tone-strip / heteronym
        // collapsing). Buffer must already parse as one or more valid
        // pinyin syllables; partial-syllable input like "zho" returns ∅.
        //
        // v1.6.5 (user polish-log 2026-05-28, liangle→凉了): switched
        // fill source from facade `PinyinDict::lookup_into` to the
        // cement IdfReader. The two sets diverge by exactly the
        // BAKED_ADDITIONS + BAKED_EXCLUSIONS in idf-from-pinyin-dict
        // (cement IDF = facade entries − exclusions + additions);
        // since v1.4.7 A4 step 1 the score-time path already reads
        // cement IDF, leaving Path 1 fill on the facade source meant
        // baked additions never reached `self.candidates` when the
        // facade source had any (typically polluted) entry for the
        // same code. Per-entry ordering doesn't matter — the L0 pin
        // pull-to-front and the legacy `freq desc` ordering both
        // happen in `candidates_with_scores`, not here.
        let mut seen: HashSet<String> = HashSet::with_capacity(64);
        let lookup_buf = inputx_pinyin::normalize_lookup_key(&self.buffer);
        for entry in pinyin_idf_reader().lookup(lookup_buf.as_bytes()) {
            let w = entry.word.to_string();
            if seen.insert(w.clone()) {
                self.candidates.push(w);
                self.has_non_speculative_candidate = true;
            }
        }

        // Path 1c (typo-shaped initials fallback): catches missing-vowel
        // typos like `pyin` (intended pinyin → expected 拼音).
        //
        // Gate: only triggers when the buffer is *not* a valid pinyin
        // prefix of any dict entry (`prefix_exists` = false). Mid-typing
        // sequences like `zhon` (en route to `zhong*`) are valid prefixes
        // and stay on the normal Path-3 prefix-completion track. A
        // genuine typo like `pyin` has no prefix match, falls here, and
        // its 2-char consonant cluster gets looked up in the 简拼 index.
        //
        // Results marked as fuzzy so they rank below true exact matches.
        //
        // NOTE: does NOT set `has_non_speculative_candidate`. Path 1c is
        // a speculative typo-correction guess, not "user is actively
        // typing pinyin". Setting the flag would leak this speculation
        // into downstream cross-engine demotion (the v0.5 pinyin-intent
        // wubi-Phrase demote), causing legitimate wubi candidates at
        // clearly-wubi-shaped input like `xlab` (wubi 细节) to get
        // demoted below speculative xl-initials pinyin matches (向量
        // etc.). User-reported 2026-05-24.
        // Phase H (2026-06-04): cap buffer length.  Pre-fix `tsuitachi`
        // (9-char 日語ローマ字 一日=ついたち) took consonant prefix `ts`
        // and reverse-looked-up every t-s initials Chinese phrase (调试 /
        // 推送 / 通缩 / 退市 ...) — user: "这里面怎么还会有这么多中文,
        // 这是怎么命中的".  Path 1c is meant ONLY for missing-vowel
        // typos with 2-consonant prefix (pyin → 拼音); those are always
        // 4-5 chars total.  Long buffers are日语ローマ字, full pinyin
        // phrase composed of more syllables, or some other non-typo
        // input — never legitimate consonant-cluster typos.
        if !PINYIN_DISABLE_FUZZY
            && let Some(consonant_prefix) = self.path1c_consonant_prefix()
        {
            {
                let idx = initials_index(&self.engine);
                if let Some(matches) = idx.get(consonant_prefix.as_bytes()) {
                    let typed_len = consonant_prefix.len().min(u8::MAX as usize) as u8;
                    for w in matches.take(50) {
                        let owned = w.to_owned();
                        if seen.insert(owned.clone()) {
                            // v1.8.1 WU-ξ: initials shorthand has its
                            // own `MatchType::Initials` tier (not
                            // fuzzy). Estimate `full_len = chars · 4`
                            // since average pinyin syllable ≈ 4 ASCII
                            // letters (中 = "zhong" = 5, 国 = "guo"
                            // = 3, avg 4). proximity = typed/full
                            // drives the K=1 decay in
                            // `derive_log_likelihood`.
                            let full_len_estimate =
                                (owned.chars().count().saturating_mul(4))
                                    .min(u8::MAX as usize) as u8;
                            self.candidates.push(owned.clone());
                            self.initials_candidates
                                .insert(owned, (typed_len, full_len_estimate));
                        }
                    }
                }
            }
        }

        // Path 1b (fuzzy pinyin): southern-dialect-tolerant initial swaps
        // on the buffer (z↔zh, c↔ch, s↔sh, n↔l, f↔h, r↔l, in↔ing,
        // en↔eng, an↔ang). Common Sogou behavior: type `zongguo` →
        // surface `中国` at a score discount. We expand the buffer's
        // initial syllable through the FuzzyConfig and look up each
        // variant, marking results as fuzzy so `candidates_with_scores`
        // can demote them.
        //
        // Skipped when Path 1 already returned a non-speculative match
        // (the user got the spelling right, no need to spray fuzzy
        // alternates) — preserves the "exact wins" rule.
        //
        // NOTE: does NOT set `has_non_speculative_candidate`. Fuzzy
        // variants are speculative (the user may have meant something
        // entirely different); letting them drive cross-engine
        // demotion would crowd out legitimate wubi entries at
        // wubi-shaped buffers. Path 1c carries the same caveat.
        if !PINYIN_DISABLE_FUZZY && !self.has_non_speculative_candidate {
            for variant in fuzzy_buffer_variants(&self.buffer) {
                if variant == self.buffer {
                    continue;
                }
                // v1.8.0 WU-ν: real weighted phonetic edit-distance
                // from the typed buffer to the variant that matched.
                // Each fuzzy candidate carries its own distance so
                // closer typos (in↔ing at 0.2) rank above looser
                // swaps (multi-pair edits compounding > 0.5). Pre-
                // v1.8 every fuzzy hit shared a flat 0.3 discount
                // regardless of how far the variant strayed.
                let distance = inputx_phonetic_edit::edit_distance(
                    &self.buffer,
                    &variant,
                    &inputx_phonetic_edit::MANDARIN_DEFAULT,
                );
                // v1.4.7 A4 step 1: fuzzy variant lookup routes
                // through cement IdfReader. Fuzzy candidates carry no
                // freq downstream (they hit the FUZZY_BASE *
                // FUZZY_DISCOUNT path in candidates_with_scores), so
                // we only need the word list — entry.log_prior is
                // discarded here.
                for entry in pinyin_idf_reader().lookup(variant.as_bytes()) {
                    let w = entry.word.to_string();
                    if seen.insert(w.clone()) {
                        self.candidates.push(w.clone());
                        self.fuzzy_candidates.insert(w, distance);
                    }
                }
            }
        }

        // Path 2: 简拼 (first-letter abbreviation) — vowel-free input only.
        // `hhh → 哈哈哈`, `zg → 中国`. Uses process-global lazy initials
        // index. Skipped when input has vowels (would be a valid syllable
        // start handled by Path 3).
        if !PINYIN_DISABLE_ASSOCIATION && looks_like_initials(&self.buffer) {
            let idx = initials_index(&self.engine);
            if let Some(matches) = idx.get(self.buffer.as_bytes()) {
                for w in matches.take(200) {
                    let owned = w.to_owned();
                    if seen.insert(owned.clone()) {
                        self.candidates.push(owned);
                        self.has_non_speculative_candidate = true;
                    }
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
        // Suppress prefix-completion when Path 1 (exact-syllable) already
        // produced any candidate. Rationale (user-reported, 2026-05-22):
        // typing `lianxiang` should yield only 联想 (exact reading) in
        // the immediate candidate list, NOT 联想集团 / 联想起 / etc. The
        // latter are *predictions* — words whose pinyin EXTENDS what the
        // user typed. They belong in a post-commit "next-word" list,
        // not muddling the immediate candidates the user is choosing
        // among right now. Path 3 still fires when Path 1 was empty
        // (e.g., `zho` mid-syllable → 中/中国/众/… via prefix scan).
        let allow_prefix_completion = !self.has_non_speculative_candidate;
        if allow_prefix_completion && self.candidates.len() < cap {
            let want = cap - self.candidates.len();
            push_prefix_top_k(
                &self.buffer,
                want,
                &mut seen,
                &mut self.candidates,
                &mut self.prefix_scored,
                &mut self.prefix_components,
            );
        }

        // Path 4: rare-CJK filter (same as wubi/table.rs).
        if !crate::wubi::show_rare() {
            self.candidates.retain(|w| crate::wubi::is_displayable(w));
        }

        // Inject the Viterbi composed sentence at the FRONT of the
        // candidate list (computed at the very top of this method).
        // The composed string is given a fixed high score in
        // `candidates_with_scores` so it surfaces as #0 even though
        // its multi-segment word doesn't have a freq entry of its own.
        // Done AFTER all other paths so it doesn't get filtered out
        // by Path 4's rare-CJK retain (composed strings are by
        // construction common-char only).
        if let Some(sentence) = self.composed_sentence.clone()
            && !self.candidates.iter().any(|w| w == &sentence)
        {
            self.candidates.insert(0, sentence);
        }

        // Path 5 (last-resort Viterbi for SHORT buffers): if every path
        // above produced nothing — the buffer is not a lexeme, not a
        // prefix of one, and not a 简拼/typo/fuzzy hit — compose it from
        // single-char dict entries so it isn't a dead end. User-reported
        // 2026-05-25: `kaopu`→靠谱, `woyao`→我要, `taikexi`→太可惜 all
        // returned ZERO candidates because Viterbi (the only path that
        // composes 靠+谱) was gated to >=8 bytes in Path 0b. Gated on
        // `is_empty()` so it CANNOT reorder any buffer that already has
        // candidates: `nuanhe` keeps 滦河 and never surfaces the
        // wrong-reading composition 暖(nuan)+和(he)→暖和. (Long empty
        // buffers were already covered by Path 0b above.)
        if !PINYIN_DISABLE_COMPOSE && self.candidates.is_empty() {
            // K-best Viterbi (v1.3 polish, 2026-05-26): 1-best (the original
            // best_composition) commits to dp[j]'s top word and can miss
            // strong-bigram alternates. User-reported `pianni`: 1-best gave
            // 片你 (freq-greedy at pian: 片>骗; (片,你) bigram weak) but the
            // (骗,你) bigram is much stronger — K-best surfaces 骗你 as #1.
            // Cap K=5: enough to bring in real-bigram alternates, small
            // enough that even pathological short-buffer cases finish in
            // microseconds (perfgate-validated).
            let comps = self.engine.dict().top_k_compositions(&self.buffer, 5);
            // Clone `top` so the borrow of `comps` ends before the
            // `for (_, sentence) in comps` move below.
            let top_owned: Option<String> = comps.first().map(|(_, t)| t.clone());
            if let Some(top) = top_owned.as_ref() {
                // Quality gate (user polish-log 2026-05-27, famiriaare):
                // foreign-romaji inputs (`famiriaare` → 法弥日呵呵热) get
                // composed from single-pinyin char dict entries that score as
                // 1-pinyin-char-per-1-Chinese-char (avg ratio 1.0-1.7). Real
                // compositions (kaopu→靠谱, nihaomawojiao→你好吗我叫,
                // pianni→骗你) average ≥ 2.0 pinyin chars per output char —
                // because real pinyin syllables are 2-3 chars and STEP_PENALTY
                // favors multi-char dict entries.
                //
                // v1.6.6 (user polish-log 2026-05-29, `rokuman` → 儿哦库曼
                // etc): the ratio < 2.0 branch now SUPPRESSES the entire
                // K-best fanout, not just `fallback_composition`. Before
                // v1.6.6, the gate only blocked the top1 from claiming
                // COMPOSED_FALLBACK_SCORE (250k); the 5 K-best comps still
                // pushed into `self.candidates` and surfaced at
                // NON_EXACT_FLOOR (1000) — visible to the user as a
                // crowd of mechanical pinyin garbage below the legitimate
                // kana / katakana. The gate's whole point is "this buffer
                // isn't real Chinese pinyin", so no K-best comp under the
                // ratio threshold deserves a candidate slot — not even at
                // the bottom of the list.
                //
                // Empirical from `_explore_composition_scores`:
                //   famiriaare (10 pinyin / 6 chars) ratio 1.67 → MECHANICAL
                //   rokuman    ( 7 pinyin / 4 chars) ratio 1.75 → MECHANICAL
                //   shinjuku   ( 8 pinyin / 4 chars) ratio 2.00 → borderline (separate compose_sentence quality gate handles)
                //   kaopu      ( 5 pinyin / 2 chars) ratio 2.50 → REAL
                //   nihaomawojiao (13/5) ratio 2.60 → REAL
                let top_chars = top.chars().count().max(1);
                let ratio = self.buffer.len() as f64 / top_chars as f64;
                if ratio >= 2.0 {
                    // Mark only the top composition with fallback_composition so
                    // candidates_with_scores gives it COMPOSED_FALLBACK_SCORE
                    // (250k). Subsequent compositions fall through to
                    // NON_EXACT_FLOOR-tier scoring and rank near the bottom —
                    // visible to the user as 60-percentile fallbacks if the
                    // top is wrong, without crowding the #1 spot.
                    self.fallback_composition = Some(top.clone());
                    // Per-alternate bigram gate (user polish-log 2026-06-01,
                    // `julei` produced 句累/局累 from K-best alternates that
                    // joined single-char dict entries with zero corpus bigram
                    // support). The Path-5 quality gate at line ~970 already
                    // applies this check to `composed_sentence` (top-1) — extend
                    // it to every K-best alternate so mechanical char-concat
                    // products can't sneak in through the NON_EXACT_FLOOR tier.
                    //
                    // Heuristic chain reconstruction: K-best alternates from
                    // `top_k_compositions` are typically built from single-char
                    // dict entries (Viterbi prefers them for last-resort
                    // composition), so iterating chars of the sentence string
                    // approximates the chain. For 2-char alternates (the
                    // dominant case here — `julei` → `句累`), this is exact.
                    // Multi-char dict entries that K-best stitches together (a
                    // rare case under Path-5's empty-candidates gate) get
                    // checked at char granularity, which is stricter than the
                    // entry boundary — acceptable since this whole path is
                    // last-resort and the strict check just drops more low-
                    // confidence stuff.
                    // alternate_bigrams_ok — uses the SAME combined
                    // intra+inter + strict-for-short rule as the Path
                    // 0b composed_sentence gate above. Kept in sync so
                    // a candidate that passes ratio>=2.0 but is still a
                    // force-segmentation (e.g. `卡-卡-日-马-苏` from
                    // `kakarimasu` — ratio exactly 2.0, only (马,苏)
                    // is corpus-present) can't sneak in through this
                    // last-resort path. v1.14 (luyaozhi 2026-06-06):
                    // L=2 chains now require BOTH links combined-
                    // present so 路-要-职-style piggyback assemblies
                    // can't survive on a single real intra bigram.
                    let ngm_table = embedded_bigrams_table();
                    let inter_table = embedded_inter_bigrams_table();
                    let alternate_bigrams_ok = |sentence: &str| -> bool {
                        let chars: Vec<String> = sentence
                            .chars()
                            .map(|c| c.to_string())
                            .collect();
                        let n = chars.len();
                        if n <= 1 {
                            return true;
                        }
                        let links = n - 1;
                        let non_zero = (1..n).filter(|&i| {
                            combined_bigram_log_prob_q4(
                                ngm_table,
                                inter_table,
                                Some(chars[i - 1].as_str()),
                                &chars[i],
                            ) > 0
                        }).count();
                        // v1.14 strict-all: see Path 0b gate comment.
                        non_zero >= links
                    };
                    // top-1 (fallback_composition) NO LONGER exempt
                    // (user report 2026-06-02 `kakarimasu` → 卡卡日马苏).
                    // The earlier exemption rationale ("already passed
                    // composed_sentence quality gate") was wrong for
                    // Path 5 fallback: that path fires PRECISELY when
                    // composed_sentence is empty (Path 5 only runs
                    // when exact dict matches and composed_sentence
                    // both came up dry). Top-1 has to pass the gate
                    // on its own merits; `靠谱` does ((靠, 谱) > 0),
                    // mechanical force-segmentations don't.
                    if !alternate_bigrams_ok(top) {
                        self.fallback_composition = None;
                    }
                    for (_, sentence) in comps {
                        if &sentence == top {
                            // Already gated above; push if survived.
                            if self.fallback_composition.is_some() {
                                self.candidates.push(sentence);
                            }
                            continue;
                        }
                        if !alternate_bigrams_ok(&sentence) {
                            continue;
                        }
                        if !self.candidates.iter().any(|w| w == &sentence) {
                            self.candidates.push(sentence);
                        }
                    }
                }
                // ratio < 2.0: drop all K-best comps. self.candidates stays
                // empty for this path; in Mixed+JP, the kana / katakana
                // candidates from japanese_adapter still surface via the
                // cross-engine dispatch merge.
            }
            // NOTE: the proper home for common words like 靠谱/榨干 is the
            // dict itself (coverage — dict-pipeline T0); this is the safety
            // net until the rebuild adds them.
        }

        // Path 3b (音节意识细化, 2026-06-06): syllable-aware trim-retry,
        // true last-resort. Fires ONLY when:
        //   - every prior path produced nothing (`self.candidates.is_empty()`,
        //     same gate as Path 5 above), and
        //   - the buffer is 4-5 chars (Phase H cap territory; longer
        //     buffers are likely JP ローマ字 or multi-syllable inputs
        //     that other engines handle, not trim-retry territory), and
        //   - the buffer has a clean ≥3-char syllable prefix
        //     (`has_clean_syllable_prefix`).
        //
        // Behavior: drop trailing chars one at a time until
        // `prefix_exists` succeeds on the trimmed buffer, then push
        // that shorter prefix's completions. The user sees the same
        // candidate panel as if they hadn't typed the trailing chars.
        //
        //   `shehv` → trim `v` → `sheh` (prefix_exists ✓) → push 50
        //     cands (社会 / 奢华 / 设好 / 射核 / …).
        //
        // Spec: docs/PLAN-syllable-aware-pinyin.md §5.3. Placement
        // AFTER Path 5 (not after Path 3 as the spec's first draft
        // proposed) ensures Path 3b doesn't pre-empt the Viterbi
        // compose path (kaopu→靠谱, woyao→我要, taikexi→太可惜).
        let buf_len = self.buffer.len();
        if !PINYIN_DISABLE_FUZZY
            && self.candidates.is_empty()
            && (4..=5).contains(&buf_len)
            && self.has_clean_syllable_prefix()
        {
            let mut seen = std::collections::HashSet::<String>::new();
            let cap = match buf_len {
                4 => 200,
                _ => 200,
            };
            let max_trim = 4.min(buf_len.saturating_sub(1));
            for trim in 1..=max_trim {
                let shorter = &self.buffer[..buf_len - trim];
                if self.engine.dict().prefix_exists(shorter) {
                    push_prefix_top_k(
                        shorter,
                        cap,
                        &mut seen,
                        &mut self.candidates,
                        &mut self.prefix_scored,
                        &mut self.prefix_components,
                    );
                    break;
                }
            }
        }
    }
}

/// Fuzzy-pinyin buffer variants: produce alternate spellings by
/// swapping the buffer's initial-prefix consonants per common
/// dialect-tolerant rules (z↔zh, c↔ch, s↔sh, n↔l, f↔h, r↔l) and
/// final-prefix vowel groups (in↔ing, en↔eng, an↔ang). Returns the
/// original buffer + each variant; caller is responsible for skipping
/// the original when iterating.
///
/// Single-rule application (no cascade): `zin` produces `zhin` and
/// `zing`, not `zhing`. Good enough for typing tolerance; cascades
/// would explode the candidate list.
fn fuzzy_buffer_variants(buffer: &str) -> Vec<String> {
    let mut out = Vec::with_capacity(8);
    out.push(buffer.to_string());
    let initial_swaps: &[(&str, &str)] = &[
        ("zh", "z"), ("z", "zh"),
        ("ch", "c"), ("c", "ch"),
        ("sh", "s"), ("s", "sh"),
        ("n", "l"), ("l", "n"),
        ("f", "h"), ("h", "f"),
        ("r", "l"),
    ];
    for (from, to) in initial_swaps {
        if let Some(rest) = buffer.strip_prefix(from) {
            let mut alt = String::with_capacity(buffer.len() + 1);
            alt.push_str(to);
            alt.push_str(rest);
            if !out.contains(&alt) {
                out.push(alt);
            }
        }
    }
    // Final-prefix vowel-group swaps: `xin` ↔ `xing`, etc. Apply on the
    // FIRST syllable only (won't catch all positions but covers the
    // common case of single-syllable input where the user typed `zin`
    // wanting `zing`).
    let final_swaps: &[(&str, &str)] = &[
        ("ing", "in"), ("in", "ing"),
        ("eng", "en"), ("en", "eng"),
        ("ang", "an"), ("an", "ang"),
    ];
    for (from, to) in final_swaps {
        if let Some(stem) = buffer.strip_suffix(from) {
            if !stem.is_empty() {
                let mut alt = String::with_capacity(buffer.len() + 1);
                alt.push_str(stem);
                alt.push_str(to);
                if !out.contains(&alt) {
                    out.push(alt);
                }
            }
        }
    }
    out
}

/// Does `suffix` look like the START of some valid pinyin syllable?
/// Used by `has_future_match` to distinguish "user mid-typing yong+bu+l"
/// (l starts li/la/le → return true) from "user typing garbage qwxzy"
/// (no valid syllable starts qw → return false).
///
/// Heuristic: try concatenating suffix with 0/1/2/3 trailing chars
/// (any ASCII alpha) and see if any forms a valid syllable. Cheap
/// since we only iterate suffix len * 26^N which is bounded.
fn suffix_could_start_syllable(
    suffix: &str,
    is_valid: impl Fn(&str) -> bool,
) -> bool {
    // suffix itself a valid syllable?
    if is_valid(suffix) { return true; }
    // suffix + 1 trailing char forms valid? (l + i = li)
    for c1 in b'a'..=b'z' {
        let mut test = suffix.to_string();
        test.push(c1 as char);
        if is_valid(&test) { return true; }
        // + another char (li + a = lia? no; li + n = lin yes)
        for c2 in b'a'..=b'z' {
            let mut test2 = test.clone();
            test2.push(c2 as char);
            if is_valid(&test2) { return true; }
        }
    }
    false
}

// Repeated-letter expansion: migrated to rules/builtin/repeated_letter.rs
// (v3.0.2b, 2026-05-24). The CandidateRule impl there is the single
// source of truth; refresh_candidates above invokes it via the global
// CANDIDATE_RULE_ENGINE. Inline function deleted.

/// Scan the cement-owned pinyin IdfReader for entries whose code starts
/// with `prefix`, pick the top `k` by frequency (excluding anything
/// already in `seen`), push them onto `out` in freq-desc order, and
/// (v1.3 WU-α CP-B) record a `predict_score` for each winner into
/// `out_scored` keyed by word.
///
/// `out_scored` is populated only for the multi-letter prefix path; the
/// single-letter cached path leaves `out_scored` untouched, so those
/// candidates retain the legacy NON_EXACT_FLOOR floor in
/// `candidates_with_scores`. Rationale: at len=1 proximity = 1/N is so
/// small (~0.1) that `proximity^K` ≈ 0 — predict_score reduces to base,
/// which would lift every single-letter completion uniformly to ~250k
/// and crowd out the natural high-freq ordering. The `length_bias` path
/// already orders single-letter completions correctly.
///
/// v1.4.7 A4 step 1: data source is now `pinyin_idf_reader()` (cement-
/// owned IdfReader over EMBEDDED_PINYIN_IDF) instead of
/// `engine.dict().prefix_for_each_raw`. The IDF entry carries Q4
/// `log_prior` directly — we use it as the heap-pre-check key (monotone
/// in raw freq modulo Q4 quantization) so the scan-loop hot path
/// avoids `estimated_freq_from_log_prior`'s `exp()` per entry. Raw
/// freq is reconstructed at drain time on the ≤k winners only, before
/// feeding `predict_score_with_components`.
///
/// Heap discipline: min-heap of size k keyed by `log_prior`. New entry
/// is admitted iff its `log_prior` beats the current heap minimum.
/// Word-asc tiebreaker for determinism; code length tagged along
/// (unused for sorting since word breaks ties) so the drain pass can
/// compute proximity = `prefix.len() / code.len()` per winner.
fn push_prefix_top_k(
    prefix: &str,
    k: usize,
    seen: &mut HashSet<String>,
    out: &mut Vec<String>,
    out_scored: &mut HashMap<String, f64>,
    out_components: &mut HashMap<String, super::merge::ScoreComponents>,
) {
    if k == 0 {
        return;
    }
    // Single-letter prefix cache. Bare 1-letter prefixes like `z` (~50k
    // entries) and `h` (~30k) dominate perfgate worst-case; pre-compute
    // top-K-by-freq for each of the 26 single letters once at warmup
    // (or lazy on first miss), then subsequent queries are a cache hit
    // — microseconds instead of milliseconds. CP-B leaves these on the
    // legacy NON_EXACT_FLOOR path (see doc above).
    if prefix.len() == 1
        && let Some(c) = prefix.chars().next()
        && c.is_ascii_lowercase()
    {
        let cached = single_letter_cache(c);
        for word in cached.iter().take(k) {
            if seen.insert(word.clone()) {
                out.push(word.clone());
            }
        }
        return;
    }

    // Heap key: (raw_freq, Reverse(word), code_len). raw_freq is the
    // pre-quantization corpus frequency stored alongside log_prior in
    // the IDF entry (v1.4.7 A4 step 1 schema bump), so the heap-pre-
    // check sees the lossless ordering with no `exp()` per entry —
    // unlike a log_prior-keyed heap, ties never need a tiebreaker
    // round-trip. Outer Reverse so BinaryHeap behaves as a min-heap
    // (top = smallest raw_freq, ready to evict).
    type HeapEntry = Reverse<(u32, Reverse<String>, usize)>;
    let mut heap: BinaryHeap<HeapEntry> = BinaryHeap::with_capacity(k + 1);

    pinyin_idf_reader().prefix_for_each_entry(prefix.as_bytes(), |e| {
        // Cheap pre-check FIRST against heap minimum. >99% of FST
        // entries on short prefixes fail this and bail before allocating
        // the word String. For `z`-prefix scans this saves both the
        // dedup vs. `seen` AND the per-entry allocation.
        if heap.len() == k {
            let min_freq = heap.peek().expect("heap is full (len == k)").0.0;
            if e.raw_freq <= min_freq {
                return;
            }
            heap.pop();
        }
        heap.push(Reverse((e.raw_freq, Reverse(e.word.to_owned()), e.code.len())));
    });

    // Drain in raw_freq-desc + lex-asc order. Same ordering as the
    // legacy `prefix_for_each_raw`-driven path — IDF carries the same
    // raw freq the facade dict used to emit.
    let mut drained: Vec<(u32, String, usize)> = heap
        .into_iter()
        .map(|Reverse((freq, Reverse(word), code_len))| (freq, word, code_len))
        .collect();
    drained.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let prefix_len = prefix.len();
    for (raw_freq, word, code_len) in drained {
        // Always populate out_scored/out_components for prediction hits,
        // even when the word is already in `seen` (e.g., a fuzzy path
        // inserted it earlier). The scoring layer checks prefix_scored
        // BEFORE fuzzy_candidates so an entry that's reachable as both
        // a mid-typing prediction AND a typo-corrected fuzzy match gets
        // the prediction (180k+) score, not the fuzzy (105k) score —
        // user polish-log 2026-05-27 (`famin` → 发明: prefix-scan of
        // `famin*` finds `faming`→发明 as prediction, fuzzy swap
        // in↔ing ALSO finds 发明; the prediction reading is the
        // confident one).
        let proximity = (prefix_len as f64 / code_len.max(1) as f64).min(1.0);
        let (score, components) = scoring::predict_score_with_components(
            scoring::LIKELIHOOD_PINYIN_PREDICT_BASE,
            raw_freq as u64,
            scoring::PRIOR_FREQ_MULT_PINYIN,
            proximity,
            inputx_pinyin_helpers::pinyin_corpus_total(),
        );
        // WU-ψ: pinyin predictions → tier 7 (specialty).
        let components = components.with_tier(7);
        out_scored.insert(word.clone(), score);
        out_components.insert(word.clone(), components);
        // Candidate-list push: only when not already there (dedup vs.
        // earlier paths' insertions). The scored/components maps above
        // are populated unconditionally so the upgrade path can hit
        // them via word-key lookup.
        if seen.insert(word.clone()) {
            out.push(word);
        }
    }
}

/// Single-letter prefix cache: 26 entries, each holding the top-30 words
/// by length-biased freq for that prefix. Built lazily on first miss;
/// subsequent queries are O(1) HashMap lookup + slice clone. Warmup
/// pre-touches `h` and `z` so the cold-path cost is paid up front
/// during `Session::warmup`.
///
/// v1.4.7 A4 step 1: source is `pinyin_idf_reader()`; raw_freq is
/// read losslessly from the IDF entry (schema bump replaced the unused
/// bigram_offset slot), so `length_bias × raw_freq` matches the legacy
/// `prefix_for_each_raw`-driven heap exactly. Runs once per letter at
/// `Session::warmup`; cost amortizes across the process lifetime.
fn single_letter_cache(letter: char) -> Arc<Vec<String>> {
    use std::sync::Mutex;
    static CACHE: OnceLock<Mutex<HashMap<char, Arc<Vec<String>>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::with_capacity(26)));
    {
        let g = cache.lock().expect("single_letter_cache mutex poisoned");
        if let Some(arc) = g.get(&letter) {
            return arc.clone();
        }
    }
    // Cache miss — compute, then store.
    let computed = compute_single_letter_top_k(letter, 30);
    let arc = Arc::new(computed);
    let mut g = cache.lock().expect("single_letter_cache mutex poisoned");
    g.entry(letter).or_insert_with(|| arc.clone()).clone()
}

fn compute_single_letter_top_k(letter: char, k: usize) -> Vec<String> {
    let prefix = letter.to_string();
    // Entry key is the *length-biased* freq (raw freq × scoring::length_bias)
    // so single chars lead multi-char phrases for a bare letter. See
    // `scoring::length_bias` for the rationale (user: "单字评分要更高").
    type HeapEntry = Reverse<(u64, Reverse<String>)>;
    let mut heap: BinaryHeap<HeapEntry> = BinaryHeap::with_capacity(k + 1);
    pinyin_idf_reader().prefix_for_each_entry(prefix.as_bytes(), |e| {
        let adj = (e.raw_freq as f64 * scoring::length_bias(e.word.chars().count())) as u64;
        if heap.len() == k {
            let min_adj = heap.peek().expect("heap full").0.0;
            if adj <= min_adj {
                return;
            }
            heap.pop();
        }
        heap.push(Reverse((adj, Reverse(e.word.to_owned()))));
    });
    let mut drained: Vec<(u64, String)> = heap
        .into_iter()
        .map(|Reverse((adj, Reverse(word)))| (adj, word))
        .collect();
    drained.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    drained.into_iter().map(|(_, w)| w).collect()
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
fn initials_index(seed_engine: &PinyinEngine) -> Arc<InitialsIndex> {
    INITIALS_INDEX
        .get_or_init(|| Arc::new(build_initials_index(seed_engine)))
        .clone()
}

fn build_initials_index(engine: &PinyinEngine) -> InitialsIndex {
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
    // Compact storage path (v1.6.x). Per-bucket: compute char-quality-
    // weighted score, sort. Then flatten ALL buckets' words into a
    // single byte pool + build an FSA mapping initials → (offset,
    // count) into the pool. Replaces a `HashMap<String, Vec<String>>`
    // (~20 MB / 400k Strings) with a single `Vec<u8>` (~3 MB) + a
    // single FSA byte buffer.
    use inputx_fsa::Builder as FsaBuilder;
    let mut word_pool: Vec<u8> = Vec::with_capacity(1 << 22); // ~4 MB hint
    let mut fsa = FsaBuilder::new();
    for (initials, bucket) in tmp {
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

        let first_offset = word_pool.len() as u32;
        let mut count: u32 = 0;
        for (w, _) in &scored {
            let bytes = w.as_bytes();
            if bytes.len() > u16::MAX as usize {
                continue;  // defensive — won't happen for IME candidates
            }
            word_pool.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
            word_pool.extend_from_slice(bytes);
            count += 1;
        }
        let value = (first_offset as u64) | ((count as u64) << 32);
        fsa.insert(initials.as_bytes(), value);
    }
    word_pool.shrink_to_fit();
    InitialsIndex {
        word_pool,
        fsa_bytes: fsa.finish(),
    }
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
            if inputx_pinyin::is_valid_syllable(cand) {
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
    fn viterbi_rejects_mechanical_force_segmentation() {
        // Updated 2026-06-02 (user report on `kakarimasu` →
        // 卡卡日马苏): the previous assertion that Viterbi MUST fire
        // for `nihaomawojiao` (你好吗我叫) was wrong per user's
        // refined judgment — "你好吗我叫 这也不算是个句子, 这个其实
        // 也不应该出现". Both that and `卡-卡-日-马-苏` are mechanical
        // force-segmentations that the LENIENT (≥1 link non-zero)
        // bigram gate let through; the stricter ceil((N-1)/2)-link
        // majority gate now drops them.
        //
        // This test pins the behavior: nihaomawojiao gets NO
        // composed_sentence (no real Chinese sentence backing in
        // the bigram corpus).
        //
        // v1.14 (2026-06-06 luyaozhi): the gate moved from ceil-half
        // to strict-all when the inter-bigram NGM blob landed —
        // combined intra+inter signal makes more individual links
        // non-zero ((好, 吗) inter=20, (我, 叫) inter=16 — both kept
        // by `--min-count 15`), so ceil-half becomes too lenient.
        // Strict-all keeps the original "this isn't a real sentence"
        // semantic: chain `[你, 好, 吗, 我, 叫]` has (吗, 我) absent
        // in both tables → 3/4 non-zero → strict-all fails → drops.
        let mut a = PinyinAdapter::new();
        for b in b"nihaomawojiao" {
            a.handle_letter(*b);
        }
        assert!(a.composed_sentence.is_none(),
            "nihaomawojiao should NOT surface composed_sentence under \
             the strict-all bigram gate; got {:?}",
            a.composed_sentence);
    }

    #[test]
    fn repeat_letter_expands_to_interjection_chain() {
        if super::PINYIN_DISABLE_ASSOCIATION { return; }
        let mut a = PinyinAdapter::new();
        for b in b"hhhhh" { a.handle_letter(*b); }
        assert_eq!(a.candidates().first().cloned(), Some("哈哈哈哈哈".to_string()),
            "hhhhh should produce 哈哈哈哈哈 at #0; got {:?}", a.candidates());

        let mut a = PinyinAdapter::new();
        for b in b"aaaa" { a.handle_letter(*b); }
        assert_eq!(a.candidates().first().cloned(), Some("啊啊啊啊".to_string()));
    }

    #[test]
    fn repeat_letter_below_threshold_no_expansion() {
        let mut a = PinyinAdapter::new();
        for b in b"hh" { a.handle_letter(*b); }
        // hh is < 3 chars — falls through to 简拼 path (lookup "hh" in
        // initials). The auto-laughter expansion shouldn't fire.
        assert_ne!(a.candidates().first().map(String::as_str), Some("哈哈"));
    }

    #[cfg(not(feature = "bootstrap_only"))]
    #[test]
    fn fuzzy_zongguo_surfaces_zhongguo() {
        if super::PINYIN_DISABLE_FUZZY { return; }
        let mut a = PinyinAdapter::new();
        for b in b"zongguo" { a.handle_letter(*b); }
        // zongguo → no exact match, but fuzzy z→zh expansion finds 中国
        // via the zhongguo dict entry. Should appear somewhere in
        // candidates (not necessarily #0 since wubi/non-fuzzy may take
        // priority in mixed mode — here we just verify presence).
        assert!(a.candidates().iter().any(|w| w == "中国"),
            "fuzzy z→zh should surface 中国 for zongguo; got {:?}", a.candidates());
    }

    #[cfg(not(feature = "bootstrap_only"))]
    #[test]
    fn typo_pyin_surfaces_pinyin_via_initials() {
        if super::PINYIN_DISABLE_FUZZY { return; }
        let mut a = PinyinAdapter::new();
        for b in b"pyin" { a.handle_letter(*b); }
        // pyin = missing-vowel typo for pinyin. Path 1c picks up
        // consonant prefix "py" and queries initials_index → 拼音 etc.
        assert!(a.candidates().iter().any(|w| w == "拼音"),
            "py initials should surface 拼音 for typo pyin; got {:?}", a.candidates());
    }

    #[test]
    fn viterbi_short_buffer_never_claims_top() {
        let mut a = PinyinAdapter::new();
        // 6-byte buffer — under the 8-byte threshold. Path 0b's #0
        // injection (composed_sentence) must stay off for short buffers,
        // so a wrong-reading composition like 暖(nuan)+和(he)→暖和 can
        // never win the top slot (暖和 is really "nuanhuo").
        for b in b"nuanhe" {
            a.handle_letter(*b);
        }
        assert!(a.composed_sentence.is_none(),
            "composed_sentence (#0 boost) must not fire for short buffers");
    }

    #[cfg(not(feature = "bootstrap_only"))]
    #[test]
    fn nuanhe_keeps_real_match_not_wrong_reading_composition() {
        if super::PINYIN_DISABLE_COMPOSE || super::PINYIN_DISABLE_FUZZY { return; }
        // nuanhe is NOT empty (滦河 via n→l fuzzy), so the Path 5
        // last-resort fallback must NOT fire — the wrong-reading
        // composition 暖和 must not even appear, let alone outrank 滦河.
        let mut a = PinyinAdapter::new();
        for b in b"nuanhe" { a.handle_letter(*b); }
        assert!(!a.candidates().is_empty(), "nuanhe should have candidates");
        assert_ne!(a.candidates().first().map(String::as_str), Some("暖和"),
            "wrong-reading 暖和 must not be #0 for nuanhe; got {:?}", a.candidates());
    }

    #[cfg(not(feature = "bootstrap_only"))]
    #[test]
    fn short_non_lexeme_composes_instead_of_empty() {
        if super::PINYIN_DISABLE_COMPOSE { return; }
        // Regression for user-reported 2026-05-25: short multi-syllable
        // inputs that aren't a dict lexeme and aren't a prefix of one
        // returned ZERO candidates. Path 5 composes them from single-char
        // dict entries (靠+谱, 我+要, 太+可+惜) as a last resort.
        for (buf, want) in [
            (&b"kaopu"[..], "靠谱"),
            (&b"woyao"[..], "我要"),
            (&b"taikexi"[..], "太可惜"),
        ] {
            let mut a = PinyinAdapter::new();
            for b in buf { a.handle_letter(*b); }
            assert!(a.candidates().iter().any(|w| w == want),
                "{} should compose {want} (was empty before Path 5); got {:?}",
                core::str::from_utf8(buf).unwrap(), a.candidates());
        }
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
        if super::PINYIN_DISABLE_ASSOCIATION { return; }
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
        if super::PINYIN_DISABLE_ASSOCIATION { return; }
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
        if super::PINYIN_DISABLE_ASSOCIATION { return; }
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
        if super::PINYIN_DISABLE_ASSOCIATION { return; }
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
    fn complete_syllable_zhong_excludes_prefix_extension_words() {
        // Inverted from earlier behavior (pre-2026-05-22). When the input
        // resolves to a *complete* syllable like `zhong`, exact-reading
        // matches (中, 众, 终, ...) take the candidate list and prefix-
        // extension words like 中国 (whose reading is "zhongguo", strictly
        // longer than `zhong`) are SUPPRESSED. They're predictions, not
        // current candidates — they belong in a post-commit next-word
        // list. Same principle as the user-reported lianxiang→联想 case.
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
            !cands.iter().any(|w| w == "中国"),
            "zhong must NOT surface 中国 — it's a prefix-extension prediction. \
             Got: {:?}",
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

    // Skipped under `cargo test --release` (parallel) because CPU
    // contention from concurrent workspace test binaries makes single-
    // sample timing measurements meaningless — we've measured the same
    // probe hitting p50=4.8ms in isolation vs p95=23ms under load even
    // though the actual algorithmic cost didn't change. Run via
    // `scripts/perf_isolated.sh` which enforces the real strict gate
    // (single-threaded, 16ms p95). The test body still asserts honestly
    // there; this annotation just keeps the noisy parallel run from
    // failing on infrastructure noise that's not user-facing.
    #[test]
    #[cfg_attr(not(feature = "perfgate"), ignore = "run via scripts/perf_isolated.sh")]
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
        // v1.5d adds long-pinyin probes that exercise the Viterbi
        // viability tier in has_future_match (8+ chars → ASCII
        // fallback check calls best_composition on up to 4 prefixes).
        let probes: &[&str] = &[
            "z", "zh", "zho", "zhon", "zhong", "zhongguo", "wo", "women", "ni", "nihao", "h",
            "hh", "hhh",
            // Long-pinyin Viterbi-viability hot path.
            "nihaoma", "nihaomawoj", "nihaomawojiao", "yongbuliao",
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
            // P95 across ITER samples — `times[N*95/100]` after sort_unstable.
            // We use p95 (not max) for the frame-budget check below because
            // `cargo test --release` runs workspace crates in parallel and
            // single-sample max gets clobbered by CPU contention spikes that
            // aren't representative of the algorithm. p95 reflects the
            // sustained worst case the user actually feels. The real perf
            // story is verified by `scripts/perf_isolated.sh` (single-thread,
            // no contention) where max stays inside 16ms too.
            let p95 = times[(times.len() * 95) / 100];
            let max = *times.last().unwrap();

            eprintln!(
                "perfgate {input:>8}: min={:>5.2}ms p50={:>5.2}ms p95={:>5.2}ms max={:>5.2}ms",
                min as f64 / 1_000_000.0,
                p50 as f64 / 1_000_000.0,
                p95 as f64 / 1_000_000.0,
                max as f64 / 1_000_000.0,
            );

            if !cfg!(debug_assertions) {
                if min > MIN_BUDGET_NS {
                    eprintln!(
                        "  ^^ FAIL: min {:.2}ms exceeds {}ms uncontended budget — \
                         indicates an algorithmic regression, NOT noise",
                        min as f64 / 1_000_000.0,
                        MIN_BUDGET_NS / 1_000_000
                    );
                    all_passed = false;
                }
                if p95 > MAX_BUDGET_NS {
                    eprintln!(
                        "  ^^ FAIL: p95 {:.2}ms exceeds {}ms frame budget — \
                         sustained slow case the user would feel",
                        p95 as f64 / 1_000_000.0,
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

    // ------------------------------------------------------------------
    // Realistic-load perfgate — exercises `candidates_with_scores` with
    // a non-None `prev_committed`, which is the path the IME runtime
    // actually walks per keystroke once the user has committed any
    // prior word. The bare-`handle_letter` perfgate above misses this
    // because it never threads a prev_word through, so the bigram-
    // boost call short-circuits at `let Some(prev) = prev else
    // { return 0 }`.
    //
    // Regression caught by adding this fixture: v1.4.6 NGMv1 cutover
    // (commit 399b142) put `legacy_bigram_boost_from_ngm` on the
    // refresh path, but the underlying `NgramTable::log_prob` was an
    // O(N) full-table linear scan with two utf8 validations per
    // triplet. Per-keystroke cost ballooned to ~100ms under realistic
    // candidate counts (sample(1) on PID 5930, 2026-05-31). Fixed by
    // adding a lazy ctx → triplet-range index in inputx-ngram (the
    // FST ctx index section in the on-disk format reserves space for
    // this; the writer hasn't populated it yet so the reader builds
    // an in-memory equivalent on first use).
    //
    // Budget is looser than the bare-handle_letter gate (32ms p95 vs
    // 16ms) because this fixture also runs scoring + cross-engine
    // merge prep on each iteration, which inflates the wall-clock
    // even when the bigram-boost path is cheap. A 2× regression
    // there would still surface as p95 > 32ms.
    #[test]
    #[cfg_attr(not(feature = "perfgate"), ignore = "run via scripts/perf_isolated.sh")]
    fn perfgate_candidates_with_scores_prev_committed() {
        let mut warmer = PinyinAdapter::new();
        warmer.warmup();
        // Trigger lazy ctx-index build inside the bigrams NGM table by
        // doing one no-op scored call. Subsequent timed iterations
        // measure steady-state cost only.
        let _ = warmer.candidates_with_scores(Some("我"));

        const ITER: usize = 30;
        const MAX_BUDGET_NS: u128 = 32_000_000; // 32 ms p95

        // (buffer, prev_word) pairs. prev_word is a common Chinese
        // word the bigrams table is guaranteed to have entries for —
        // picked to land on the slow path the regression exposed.
        let probes: &[(&str, &str)] = &[
            ("zhongguo", "我"),
            ("women", "你"),
            ("nihaoma", "今天"),
            ("yongbuliao", "你"),
            ("nihaomawojiao", "我们"),
        ];

        let mut all_passed = true;
        for (input, prev) in probes {
            let bytes = input.as_bytes();
            let mut times: Vec<u128> = Vec::with_capacity(ITER);

            for _ in 0..ITER {
                let mut a = PinyinAdapter::new();
                for &b in bytes {
                    a.handle_letter(b);
                }
                // Time the scoring path with a real prev_committed —
                // this is what the runtime invokes per keystroke once
                // the user has committed any prior word.
                let start = std::time::Instant::now();
                let _ = a.candidates_with_scores(Some(prev));
                times.push(start.elapsed().as_nanos());
            }

            times.sort_unstable();
            let min = times[0];
            let p50 = times[times.len() / 2];
            let p95 = times[(times.len() * 95) / 100];
            let max = *times.last().unwrap();

            eprintln!(
                "perfgate-scored {input:>14} (prev={prev}): \
                 min={:>5.2}ms p50={:>5.2}ms p95={:>5.2}ms max={:>5.2}ms",
                min as f64 / 1_000_000.0,
                p50 as f64 / 1_000_000.0,
                p95 as f64 / 1_000_000.0,
                max as f64 / 1_000_000.0,
            );

            if !cfg!(debug_assertions) && p95 > MAX_BUDGET_NS {
                eprintln!(
                    "  ^^ FAIL: p95 {:.2}ms exceeds {}ms scored-path budget — \
                     bigram-boost or scoring path has regressed",
                    p95 as f64 / 1_000_000.0,
                    MAX_BUDGET_NS / 1_000_000
                );
                all_passed = false;
            }
        }

        assert!(
            all_passed || cfg!(debug_assertions),
            "scored-path perfgate failed — see eprintln output above"
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
