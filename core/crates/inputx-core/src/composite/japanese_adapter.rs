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

/// Filter: drop candidates that contain residual ASCII letters. Where they
/// come from: `inputx_jp`'s Hepburn romaji→kana state machine treats
/// unclaimed letters (e.g. `g` not followed by a vowel) as literal Latin
/// passthrough. For an input like `gkih`, the engine produces `g` →
/// (consumes `ki` as `き`) → `h` and emits `gきh` / `gキh` — mechanically
/// correct for the engine's contract, but useless as an IME candidate.
/// We drop anything with any ASCII alpha here so cross-engine merge never
/// sees garbage. (Pure kanji / pure kana stays.)
fn is_jp_clean(word: &str) -> bool {
    !word.chars().any(|c| c.is_ascii_alphabetic())
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
    ///   * Jukugo (whole-buffer compound match)       → 300_000
    ///   * Single-kanji (whole-buffer on/kun reading) → 200_000
    ///   * Hiragana (mechanical kana rendering)        → 100_000
    ///   * Katakana                                    →  90_000
    ///
    /// These sit *below* a typical pinyin top-phrase score (~445k =
    /// 400k base + 45k freq) so pinyin-resolvable inputs still rank
    /// Chinese first, but *above* wubi Auto-layer noise (~107k) so
    /// confident JP matches aren't drowned out. The user can override
    /// per-buffer via picking #2/#3 — that goes to PolishLog and gets
    /// rolled into next pipeline run.
    pub fn candidates_with_scores(&self) -> Vec<(String, f64)> {
        use inputx_jp::KanaKind;
        self.engine
            .candidates()
            .iter()
            .enumerate()
            .filter(|(_, c)| is_jp_clean(&c.word))
            .map(|(i, c)| {
                let base = match c.kind {
                    KanaKind::Kanji => {
                        if c.word.chars().count() > 1 {
                            300_000.0
                        } else {
                            200_000.0
                        }
                    }
                    KanaKind::Hiragana => 100_000.0,
                    KanaKind::Katakana => 90_000.0,
                };
                let decay = 0.99f64.powi(i as i32);
                (c.word.clone(), base * decay)
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
}
