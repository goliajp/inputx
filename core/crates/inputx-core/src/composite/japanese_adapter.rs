//! Thin wrapper around [`inputx_jp::JapaneseEngine`] so the composite
//! layer can plug it next to `PinyinAdapter` with a matching shape:
//! `handle_letter` / `backspace` / `escape` / `clear_all` / `is_composing`
//! / `buffer_str` / `candidates` / `commit_index`.
//!
//! Kept deliberately thin — no policy or merging here. The engine itself
//! is allocation-light and tests cleanly in isolation; the adapter exists
//! to let `composite/dispatch.rs` and `composite/engine.rs` hold all
//! three engines uniformly without inputx-core taking a hard compile-
//! time switch on whether JP is in the build.

use inputx_jp::JapaneseEngine;

/// Filter: drop candidates that aren't usable IME output. Two rejection
/// classes, both products of the engine's mechanical rendering rather than
/// of real conversion:
///
/// 1. **Residual ASCII letters.** `inputx_jp`'s Hepburn romaji→kana state
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
        use inputx_jp::KanaKind;
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
        use inputx_jp::KanaKind;
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
    ///   * Single-kanji (whole-buffer on/kun reading) → scoring::JP_SINGLE_KANJI_SCORE
    ///   * Hiragana (mechanical kana rendering)        → scoring::JP_HIRAGANA_SCORE
    ///   * Katakana                                    → scoring::JP_KATAKANA_SCORE
    /// plus scoring::JP_FREQ_MULTIPLIER × freq (mechanical kana renders carry
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
    pub fn candidates_with_scores(&self) -> Vec<(String, f64)> {
        use inputx_jp::KanaKind;
        use crate::composite::scoring;
        // Full-match signal: a real full-buffer jukugo (multi-char kanji
        // with freq > 0) means the entire romaji buffer maps to a genuine
        // Japanese word — high-confidence "user is typing Japanese". In
        // that case the whole JP group is promoted so a high-freq jukugo
        // (新宿) beats the Chinese forced-composition fallback and kana
        // (esp. katakana) surfaces. See scoring::JP_FULL_MATCH_PROMOTE.
        let full_match = self.engine.candidates().iter().any(|c| {
            c.kind == KanaKind::Kanji && c.word.chars().count() > 1 && c.freq > 0
        });
        let promote = if full_match { scoring::JP_FULL_MATCH_PROMOTE } else { 1.0 };
        self.engine
            .candidates()
            .iter()
            .filter(|c| is_jp_clean(&c.word))
            .map(|c| {
                // base = per-kind floor; freq-weighted add lifts high-freq
                // JP above rare Chinese (per user rule: JP base < wubi/
                // pinyin base, but JP-high-freq > 中文难检字/生僻词组).
                // Top JP jukugo (freq 100) lands at 200k + 100*3000 = 500k,
                // safely above pinyin rare (~410k) and wubi Auto (~70k),
                // but below pinyin top (480k) and wubi simcodes (600k+).
                let base = match c.kind {
                    KanaKind::Kanji => {
                        if c.word.chars().count() > 1 {
                            scoring::JP_JUKUGO_SCORE
                        } else {
                            scoring::JP_SINGLE_KANJI_SCORE
                        }
                    }
                    KanaKind::Hiragana => scoring::JP_HIRAGANA_SCORE,
                    KanaKind::Katakana => scoring::JP_KATAKANA_SCORE,
                };
                let score = (base + scoring::JP_FREQ_MULTIPLIER * c.freq as f64) * promote;
                (c.word.clone(), score)
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
