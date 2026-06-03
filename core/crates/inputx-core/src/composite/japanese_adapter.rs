//! Thin wrapper around the composite-side [`crate::japanese::JapaneseEngine`]
//! (v1.5.1 WU-κ carved out of `inputx_nihongo::JapaneseEngine` —
//! same API surface, jukugo / kanji lookups now route through IDF
//! readers instead of facade const tables) so the composite layer
//! can plug it next to `PinyinAdapter` with a matching shape:
//! `handle_letter` / `backspace` / `escape` / `clear_all` / `is_composing`
//! / `buffer_str` / `candidates` / `commit_index`.
//!
//! Kept deliberately thin — no policy or merging here. The engine itself
//! is allocation-light and tests cleanly in isolation; the adapter exists
//! to let `composite/dispatch.rs` and `composite/engine.rs` hold all
//! three engines uniformly without inputx-core taking a hard compile-
//! time switch on whether JP is in the build.

// v1.5.1 WU-κ carve-out: composite-side JapaneseEngine reads jukugo /
// kanji corpus through cement IDF readers instead of the facade
// `JUKUGO_TABLE` / `KANJI_TABLE` const tables. Facade
// `inputx_nihongo::JapaneseEngine` stays intact for direct-facade
// consumers (notably `inputx-nihongo-wasm`); see
// `crate::japanese::mod` for the carve-out rationale.
use crate::japanese::JapaneseEngine;

/// Filter: drop candidates that aren't usable IME output. Two rejection
/// classes, both products of the engine's mechanical rendering rather than
/// of real conversion:
///
/// 1. **Residual ASCII letters.** `inputx_nihongo`'s Hepburn romaji→kana state
///    machine treats unclaimed letters (e.g. `g` not followed by a vowel)
///    as literal Latin passthrough. For `gkih` it produces `g` →
///    (consumes `ki` as `き`) → `h` and emits `gきh` / `gキh` —
///    mechanically correct for the engine's contract, useless as an IME
///    candidate.
///
/// 2. **Particle-kana-led kanji garbage.** `compose_sentence`'s bare-tail
///    / 2-segment paths can splice a leading particle kana onto a trailing
///    single-kanji reading: `woyao` → `をや小` (particle を+や leading 小,
///    the `o`-reading kanji), drowning out 我要. A real JP conversion that
///    contains *any* kanji is always content-word-led — 私は学生, 食べる
///    (okurigana), 日本です all START with kanji. So a candidate that
///    contains kanji but does not start with kanji is spliced junk. Pure
///    kana (をやお / ヲヤオ) and pure kanji (日本) are unaffected.
fn is_jp_clean(word: &str) -> bool {
    if word.chars().any(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    if word.chars().any(is_kanji) {
        if let Some(first) = word.chars().next() {
            if !is_kanji(first) {
                return false;
            }
        }
    }
    true
}

/// CJK Unified Ideographs basic block — the same range the wubi import
/// tools and the composite proptest generators use for "is this a Han
/// character". Kana (U+3040–30FF) deliberately fall outside, so okurigana
/// tails and particle kana don't read as kanji here.
fn is_kanji(c: char) -> bool {
    ('\u{4E00}'..='\u{9FFF}').contains(&c)
}

/// `true` if `buf` contains a romaji substring that's only used for foreign
/// loanwords — i.e., a syllable from the extended-Hepburn table that native
/// JP vocabulary doesn't use. Triggers the katakana > hiragana score swap
/// in `candidates_with_scores` so words like `famiriaare` (ファミリアアレ),
/// `vaiorin` (ヴァイオリン), `pa-thi-` (パーティー) surface katakana ahead
/// of hiragana — matching real JP convention for gairaigo.
///
/// Patterns are the foreign-syllable subset of `inputx_nihongo::romaji::TABLE`.
/// Order: longest first (so 3-letter matches lock before 2-letter could).
/// Doesn't include kunrei-vs-Hepburn pairs (shi/si, chi/ti, tsu/tu, ji/zi):
/// those are native JP, not foreign loanword markers.
fn buffer_is_foreign_romaji(buf: &str) -> bool {
    // Patterns sorted longest-first to avoid false positives via prefix
    // overlap (none of these prefix-overlap with native syllables — `fa`
    // doesn't prefix any native, etc. — but length-desc is the convention).
    const FOREIGN: &[&str] = &[
        // 3-letter foreign extensions
        "fya", "fyu", "fyo", "vya", "vyu", "vyo",
        "tsa", "tsi", "tse", "tso", "che", "she",
        "kwa", "kwi", "kwe", "kwo", "gwa", "gwi", "gwe", "gwo",
        "wha", "whi", "whe", "who",
        "tha", "thi", "the", "tho", "dha", "dhi", "dhe", "dho",
        "twu", "dwu",
        // 2-letter foreign extensions
        "fa", "fi", "fe", "fo",
        "va", "vi", "vu", "ve", "vo",
        "wi", "we", "je",
        "xa", "xi", "xu", "xe", "xo",
        "la", "li", "lu", "le", "lo",
    ];
    let lower = buf.to_ascii_lowercase();
    FOREIGN.iter().any(|p| lower.contains(p))
}

pub struct JapaneseAdapter {
    engine: JapaneseEngine,
}

impl Default for JapaneseAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl JapaneseAdapter {
    pub fn new() -> Self {
        Self { engine: JapaneseEngine::new() }
    }

    pub fn handle_letter(&mut self, b: u8) -> bool {
        self.engine.handle_letter(b)
    }

    pub fn backspace(&mut self) -> bool {
        self.engine.backspace()
    }

    pub fn escape(&mut self) -> bool {
        self.engine.escape()
    }

    pub fn clear_all(&mut self) {
        let _ = self.engine.escape();
    }

    pub fn is_composing(&self) -> bool {
        self.engine.is_composing()
    }

    pub fn buffer_str(&self) -> &str {
        self.engine.preedit()
    }

    /// Materialize ALL JP candidates as `Vec<String>` — kept for the
    /// `commit_index` round-trip where the host already has the merged
    /// list and just needs a flat slice. Garbage filter applied (see
    /// `is_jp_clean`).
    pub fn candidates(&self) -> Vec<String> {
        self.engine
            .candidates()
            .iter()
            .map(|c| c.word.clone())
            .filter(|w| is_jp_clean(w))
            .collect()
    }

    pub fn kanji_candidates(&self) -> Vec<String> {
        use inputx_nihongo::KanaKind;
        self.engine
            .candidates()
            .iter()
            .filter(|c| c.kind == KanaKind::Kanji)
            .map(|c| c.word.clone())
            .filter(|w| is_jp_clean(w))
            .collect()
    }

    #[allow(dead_code)]
    pub fn kana_candidates(&self) -> Vec<String> {
        use inputx_nihongo::KanaKind;
        self.engine
            .candidates()
            .iter()
            .filter(|c| c.kind != KanaKind::Kanji)
            .map(|c| c.word.clone())
            .filter(|w| is_jp_clean(w))
            .collect()
    }

    /// Commit by index into the engine's candidate list.
    pub fn commit_index(&mut self, i: usize) -> Option<String> {
        self.engine.commit_index(i)
    }

    /// Scored JP candidates for the cross-engine merge.
    ///
    /// JP has no real corpus freq (data is hand-curated jukugo +
    /// kanji-with-readings tables), so we synthesize per-kind scores
    /// chosen to slot into the cross-engine ranking:
    ///
    ///   * Single-kanji (whole-buffer on/kun reading) → scoring::LIKELIHOOD_JP_SINGLE_KANJI_BASE
    ///   * Hiragana (mechanical kana rendering)        → scoring::LIKELIHOOD_JP_HIRAGANA_BASE
    ///   * Katakana                                    → scoring::LIKELIHOOD_JP_KATAKANA_BASE
    /// plus scoring::PRIOR_FREQ_MULT_JP × freq (mechanical kana renders carry
    /// freq 0). scoring.rs is the source of truth — values currently are
    /// single-kanji 100k, hiragana 150k, katakana 110k (do NOT hardcode copies
    /// here; this comment drifted once and mislabeled hiragana as 100k).
    ///
    /// These sit *below* a typical pinyin top-phrase score (~445k =
    /// 400k base + 45k freq) so pinyin-resolvable inputs still rank
    /// Chinese first, but *above* wubi Auto-layer noise (~107k) so
    /// confident JP matches aren't drowned out. The user can override
    /// per-buffer via picking #2/#3 — that goes to PolishLog and gets
    /// rolled into next pipeline run.
    pub fn candidates_with_scores(&self) -> Vec<super::merge::Scored> {
        use inputx_nihongo::KanaKind;
        use crate::composite::scoring;
        // Short-buffer compose_sentence garbage filter (user polish-log
        // 2026-05-26, jieni): for short romaji buffers (< 8 chars), the
        // engine's compose_sentence path can produce ~30 mechanical
        // "X+particle+Y" cartesian products that aren't real Japanese —
        // jieni → 時へに / 事へに / 治へに / 耳へに / ... (X=ji-yomi kanji,
        // particle=へ from `e`, Y=ni-reading). They score at
        // LIKELIHOOD_JP_COMPOSED_BASE so they don't lead the candidate
        // list, but their sheer count (~30) crowds out the visible window.
        //
        // The 8-char cutoff mirrors pinyin Path 0b Viterbi: under 8 chars
        // the user isn't typing a multi-segment JP sentence, so any
        // composed product is noise. Real long-form composed sentences
        // (私は学生, watashiwagakusei = 14 chars) survive — at ≥8 chars
        // there's enough buffer for a genuine compose to be intentional.
        let short_buffer = self.engine.preedit().chars().count() < 8;
        // Full-match signal: a real full-buffer jukugo (multi-char kanji
        // with freq > 0) means the entire romaji buffer maps to a genuine
        // Japanese word — high-confidence "user is typing Japanese". In
        // that case the whole JP group is promoted so a high-freq jukugo
        // (新宿) beats the Chinese forced-composition fallback and kana
        // (esp. katakana) surfaces. See scoring::LIKELIHOOD_JP_FULL_MATCH_PROMOTE.
        // EXCLUDES compose_sentence products (`c.composed`): those are
        // mechanical guesses, not real dictionary words, so they must not
        // count as a full-match signal nor receive the promote.
        // Pure-kana check: a "jukugo" entry that is actually all kana (感叹/
        // 寒暄 like えっ / ありがとう, present in the hand TSV) is NOT a real
        // kanji compound.
        let is_pure_kana = |w: &str| w.chars().all(|ch| ('\u{3040}'..='\u{30FF}').contains(&ch));
        let full_match = self.engine.candidates().iter().any(|c| {
            !c.composed
                && c.proximity_milli >= 1000 // exact, not a prefix prediction
                && c.kind == KanaKind::Kanji
                && c.word.chars().count() > 1
                && c.freq > 0
                && !is_pure_kana(&c.word)
        });
        let promote = if full_match { scoring::LIKELIHOOD_JP_FULL_MATCH_PROMOTE } else { 1.0 };
        self.engine
            .candidates()
            .iter()
            .filter(|c| is_jp_clean(&c.word))
            .filter(|c| !(short_buffer && c.composed))
            .map(|c| {
                // compose_sentence products score below real Chinese words
                // (so 時へ時 never pollutes the top of jieji/jieshou) and are
                // never promoted. Two tiers, split by whether the product is
                // pure kanji: a "jukugo+suffix" compound like 東京都 is a
                // high-confidence kanji conversion → LIKELIHOOD_JP_COMPOSED_KANJI_BASE
                // (above kana so it leads in JP mode); a particle-bearing
                // compose like 時へ時 / 東京と (carries kana) stays at the low
                // LIKELIHOOD_JP_COMPOSED_BASE. Both still surface when there's no
                // Chinese competition (watashiwa→私は) and neither promotes.
                if c.composed {
                    let pure_kanji = c.word.chars().all(|ch| ('一'..='鿿').contains(&ch));
                    let s = if pure_kanji {
                        scoring::LIKELIHOOD_JP_COMPOSED_KANJI_BASE
                    } else {
                        scoring::LIKELIHOOD_JP_COMPOSED_BASE
                    };
                    // bigram_links=0 — JP compose is mechanical, no bigram
                    // chain support (cf. pinyin which gates compose on
                    // ≥1 bigram link).
                    let mt = inputx_scoring::MatchType::Composed { bigram_links: 0 };
                    // v1.4.7 A2 step 3 orthodox decomposition: compose
                    // products have no raw corpus freq (mechanical
                    // jukugo+suffix / particle splice), so log_prior_q4 = 0
                    // by construction; the per-tier base is purely a
                    // likelihood signal (how confident the engine is in
                    // *this kind* of composed structure). Pattern mirrors
                    // wubi/pinyin exact-path A2 step 1+2: pure-data axis
                    // emitted at the source, no synth helper indirection.
                    let log_likelihood_q4 =
                        (s.max(1.0).ln() * inputx_scoring::Q4 as f64).round() as i32;
                    // v1.7.4: compose products with no raw corpus freq
                    // get the freq-0 floor `log_prob_corpus_from_freq(0,
                    // total)` so they sit at the bottom of the prior
                    // axis (matching the legacy below-real-entries
                    // intent). Reuse the jukugo total since JP compose
                    // products are jukugo-shaped (multi-char kanji
                    // compounds + particle splices).
                    let log_prior_q4 = inputx_scoring::log_prob_corpus_from_freq(
                        0,
                        inputx_nihongo_data_jukugo::nihongo_jukugo_corpus_total(),
                    );
                    // WU-ψ: JP compose products → tier 4 (mechanical,
                    // less-confident than exact dict hits).
                    let components = super::merge::ScoreComponents::three_axis(
                        log_prior_q4, log_likelihood_q4, mt,
                    ).with_tier(4);
                    return (c.word.clone(), s, Some(components));
                }
                // base = per-kind floor; freq-weighted add lifts high-freq
                // JP above rare Chinese (per user rule: JP base < wubi/
                // pinyin base, but JP-high-freq > 中文难检字/生僻词组).
                // Top JP jukugo (freq 100) lands at 200k + 100*3000 = 500k,
                // safely above pinyin rare (~410k) and wubi Auto (~70k),
                // but below pinyin top (480k) and wubi simcodes (600k+).
                let base = match c.kind {
                    KanaKind::Kanji => {
                        // A pure-kana "jukugo" (えっ / ありがとう — kana 感叹/
                        // 寒暄 in the hand TSV) is not a real kanji compound;
                        // it must not get the jukugo base. Drop it to the
                        // single-kanji tier so えっ doesn't rank like a real
                        // 熟语 (user 2026-05-26: えっ at #3 for single `e`).
                        if is_pure_kana(&c.word) {
                            scoring::LIKELIHOOD_JP_SINGLE_KANJI_BASE
                        } else if c.word.chars().count() > 1 {
                            scoring::LIKELIHOOD_JP_JUKUGO_BASE
                        } else {
                            scoring::LIKELIHOOD_JP_SINGLE_KANJI_BASE
                        }
                    }
                    // Foreign-syllable buffer swap (user 2026-05-27, famiriaare):
                    // when the romaji buffer contains a foreign-loanword syllable
                    // (fa/va/wi/ti via thi/dhi/etc.), the user is typing a foreign
                    // word — katakana (ファミリアアレ) is the conventional written
                    // form, hiragana (ふぁみりああれ) is rare / unnatural. Swap
                    // the two bases so katakana leads hiragana in this regime,
                    // but stay below jukugo / kanji. Native-romaji buffers
                    // (nihon→にほん) keep hiragana > katakana as before.
                    KanaKind::Hiragana => {
                        if buffer_is_foreign_romaji(self.engine.preedit()) {
                            scoring::LIKELIHOOD_JP_KATAKANA_BASE
                        } else {
                            scoring::LIKELIHOOD_JP_HIRAGANA_BASE
                        }
                    }
                    KanaKind::Katakana => {
                        if buffer_is_foreign_romaji(self.engine.preedit()) {
                            scoring::LIKELIHOOD_JP_HIRAGANA_BASE
                        } else {
                            scoring::LIKELIHOOD_JP_KATAKANA_BASE
                        }
                    }
                };
                // Prefix-prediction proximity decay via shared `predict_score`
                // helper: an exact candidate has proximity 1.0 (no decay); a
                // predicted one (shinjuk→新宿, 0.875) decays its freq
                // contribution by proximity^K so it sits above simpdy noise
                // but below the eventual full match, and rises as the user
                // types closer. The helper unifies pinyin (CP-B), wubi
                // (CP-C), and JP (CP-A) prefix scoring around the
                // `base + freq·freq_mult·proximity^K` shape. See
                // PLAN-prefix-prediction §4 and PLAN-probabilistic-model.
                let proximity = c.proximity_milli as f64 / 1000.0;
                // v1.7.4: JP corpus_total picks the engine matching
                // the candidate's origin:
                //   * multi-char kanji (jukugo) → jukugo.idf total
                //   * single-char kanji         → kanji.idf total
                //   * Hiragana/Katakana renders → jukugo.idf total
                //     (kana have no native corpus signal but the kana
                //     `c.freq` carries the underlying kanji's freq for
                //     ranking; the much-larger jukugo total presses
                //     their log_prior into the bottom band where kana
                //     belongs in the merge, below real kanji entries).
                let corpus_total = match c.kind {
                    KanaKind::Kanji
                        if c.word.chars().count() > 1 && !is_pure_kana(&c.word) =>
                    {
                        inputx_nihongo_data_jukugo::nihongo_jukugo_corpus_total()
                    }
                    KanaKind::Kanji => {
                        inputx_nihongo_data_kanji::nihongo_kanji_corpus_total()
                    }
                    KanaKind::Hiragana | KanaKind::Katakana => {
                        inputx_nihongo_data_jukugo::nihongo_jukugo_corpus_total()
                    }
                };
                let (pre_promote, mut components) = scoring::predict_score_with_components(
                    base,
                    c.freq as u64,
                    scoring::PRIOR_FREQ_MULT_JP,
                    proximity,
                    corpus_total,
                );
                // Predictions (proximity < 1) never ride the full-match promote.
                let mult = if c.proximity_milli >= 1000 { promote } else { 1.0 };
                let score = pre_promote * mult;
                // v1.4.2 WU-γ: full-match promote folds into log_likelihood
                // (multiplicative in linear space → additive in log space).
                // Without this, score_q4() would not equal the legacy sort
                // key in rank order at the post-promote tier; the cement
                // layer cutover (v1.4.5+) needs the promote represented
                // in the log-space view. The v1.3 (base, prior, likelihood)
                // legacy view intentionally stays UN-promoted — the
                // user-visible `score` then carries the promote implicitly
                // (`score / (base + prior · likelihood) == promote`).
                if mult > 1.0 {
                    let delta_q4 =
                        (mult.ln() * inputx_scoring::Q4 as f64).round() as i32;
                    components.log_likelihood_q4 =
                        components.log_likelihood_q4.saturating_add(delta_q4);
                }
                // Tier assignment for JP candidates:
                //   - prediction (proximity < 1000) → 7 (specialty)
                //   - exact match by kind:
                //     - Hiragana / Katakana matching buffer → 4 (mechanical
                //       rendering — no dict signal, fallback only)
                //     - Jukugo (multi-char kanji)          → 1 (real dict)
                //     - Single kanji                       → 2 (real dict)
                //
                // Phase C (2026-06-03): mechanical kana rendering (the
                // `romaji::to_hiragana` / `to_katakana` fallback in
                // inputx-nihongo/src/engine.rs lines 246-265) used to
                // sit at tier 1 — that meant typing `tuijian` surfaced
                // ついじあん / ツイジアン above pinyin tier-2 推荐 in
                // Mixed+jp mode.  Demote mechanical kana to tier 4 so
                // pinyin/wubi real candidates lead in Mixed; in JP-only
                // mode the merge has no other engine so mechanical kana
                // still surfaces (just below any real dict jukugo /
                // single-kanji hits, which is correct ordering).
                //
                // Real dict basic kana (も in jukugo TSV freq=95, で
                // freq=100, etc.) are emitted by `jukugo::lookup_by_
                // reading` as KanaKind::Kanji + pure_kana, so they fall
                // into the KanaKind::Kanji branch below and keep their
                // tier 2 — they're not affected by this change.
                // Mechanical kana buffer-length split: short buffers
                // (≤ 4 chars like sai, mo, ka) are plausible JP intent
                // — user often types `sai` wanting さい — so mechanical
                // kana sits at tier 2 (cohabits with pinyin tier-2 single
                // chars, still ceded to pinyin tier 1 via engine offset).
                // Long buffers (≥ 5 chars like tuijian, kaopu, nihao,
                // jieji) are almost certainly Chinese input — the
                // mechanical kana rendering is just engine noise — so
                // they get tier 4 (well below pinyin tier-2 phrase
                // candidates).  The 4-char threshold mirrors inputx-
                // nihongo's own `kana_freq` knee (engine.rs line 246
                // sets kana_freq=100 for ≤2 chars / 30 for >2) — we
                // widen the kana-is-plausible band to 4 for tier
                // purposes because the user 2026-06-02 sai screenshot
                // pinned さい to top-10 even at 3-char buffer.
                let short_buffer_for_kana = self.engine.preedit().chars().count() <= 4;
                let tier_jp: u8 = if c.proximity_milli < 1000 {
                    7
                } else {
                    match c.kind {
                        // Mechanical romaji→kana rendering (engine.rs
                        // `to_hiragana` / `to_katakana` fallback) — no
                        // dict signal, must not lead pinyin/wubi real
                        // candidates on long buffers.  See Phase C
                        // 2026-06-03 comment above for full rationale.
                        KanaKind::Hiragana | KanaKind::Katakana => {
                            if short_buffer_for_kana { 2 } else { 4 }
                        }
                        KanaKind::Kanji => {
                            let multi = c.word.chars().count() > 1;
                            let pure_kana = is_pure_kana(&c.word);
                            match (multi, pure_kana) {
                                // Real multi-char kanji jukugo (新宿,
                                // 中国 etc.) — top-confidence dict hit.
                                (true,  false) => 1,
                                // Pure-kana multi-char "jukugo" (えっ,
                                // ありがとう) — kana 感叹/寒暄 in the
                                // hand TSV, not real 熟语.  Demoted to
                                // single-kanji tier per user 2026-05-26
                                // ("えっ at #3 for single `e` is wrong").
                                (true,  true)  => 2,
                                // Single basic kana from dict (も で
                                // を に — jukugo TSV entries of one
                                // char pure_kana).  Phase C 2026-06-03:
                                // promoted from tier 2 to tier 1 so
                                // typing `mo` surfaces も before the
                                // long tail of pinyin tier-2 mo-rhymes.
                                // Mechanical kana (also single-char pure
                                // kana from a romaji buffer) stays at
                                // tier 4 above — only DICT entries get
                                // tier 1.
                                (false, true)  => 1,
                                // Single kanji (a kanji char emitted by
                                // kanji::lookup_by_reading — `e` →
                                // 似/絵 etc.).
                                (false, false) => 2,
                            }
                        }
                    }
                };
                let components = components.with_tier(tier_jp);
                (c.word.clone(), score, Some(components))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gkih_does_not_emit_mixed_kana_garbage() {
        // User-reported (2026-05-22): `gkih` produced `整` (wubi) plus
        // `gきh` / `gキh` from JP. The mixed-Latin kana strings are
        // garbage — filter them out at the adapter boundary.
        let mut jp = JapaneseAdapter::new();
        for b in b"gkih" {
            jp.handle_letter(*b);
        }
        for cand in jp.candidates() {
            assert!(
                is_jp_clean(&cand),
                "candidate `{}` contains ASCII letters — should have been filtered",
                cand
            );
        }
    }

    #[test]
    fn clean_kana_input_still_works() {
        // Sanity: clean romaji still produces clean kana.
        let mut jp = JapaneseAdapter::new();
        for b in b"konnichiwa" {
            jp.handle_letter(*b);
        }
        let cands = jp.candidates();
        assert!(
            !cands.is_empty(),
            "expected JP candidates for konnichiwa, got empty"
        );
    }

    #[test]
    fn woyao_drops_particle_kana_led_kanji_garbage() {
        // User-reported (2026-05-25): `woyao --mode mixed --jp` surfaced
        // をや小 / をや尾 / をや和 (particle を+や leading an `o`-reading
        // single kanji) above 我要. These are spliced junk from
        // compose_sentence's bare-tail path — drop them at the boundary.
        let mut jp = JapaneseAdapter::new();
        for b in b"woyao" {
            jp.handle_letter(*b);
        }
        for cand in jp.candidates() {
            assert!(
                is_jp_clean(&cand),
                "candidate `{}` is particle-kana-led kanji garbage — should be filtered",
                cand
            );
        }
        // The clean kana renderings (をやお / ヲヤオ) must still survive so
        // JP isn't left empty for this buffer.
        assert!(
            jp.candidates().iter().any(|c| c.chars().all(|ch| !is_kanji(ch))),
            "expected at least one pure-kana candidate to survive, got {:?}",
            jp.candidates()
        );
    }

    #[test]
    fn kanji_led_candidates_survive_filter() {
        // Guard against over-filtering: content-word-led conversions that
        // legitimately carry trailing kana — particle composition (私は
        // 学生), okurigana (食べる), copula (日本です) — must NOT be
        // dropped, and pure kana / pure kanji stay clean.
        assert!(is_jp_clean("私は学生"), "kanji-led particle composition");
        assert!(is_jp_clean("食べる"), "okurigana: kanji + trailing kana");
        assert!(is_jp_clean("日本です"), "kanji-led + copula kana");
        assert!(is_jp_clean("日本"), "pure kanji");
        assert!(is_jp_clean("をやお"), "pure hiragana");
        assert!(is_jp_clean("ヲヤオ"), "pure katakana");
        // The garbage forms must be rejected.
        assert!(!is_jp_clean("をや小"), "particle-kana-led kanji");
        assert!(!is_jp_clean("をや尾"), "particle-kana-led kanji");
    }
}
