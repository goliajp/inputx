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
    /// list and just needs a flat slice.
    pub fn candidates(&self) -> Vec<String> {
        self.engine.candidates().iter().map(|c| c.word.clone()).collect()
    }

    /// JP candidates whose `kind == Kanji` (jukugo compound or single-
    /// char by on/kun reading). These are the "high-conviction" JP
    /// outputs — the user typing romaji that resolves to a known kanji
    /// form clearly meant Japanese. Merged BEFORE pinyin in the host's
    /// dispatch so 山 / 日本 / 私 / etc. rank prominently rather than
    /// landing below a wall of pinyin fuzzy matches.
    pub fn kanji_candidates(&self) -> Vec<String> {
        use inputx_jp::KanaKind;
        self.engine
            .candidates()
            .iter()
            .filter(|c| c.kind == KanaKind::Kanji)
            .map(|c| c.word.clone())
            .collect()
    }

    /// JP candidates whose `kind` is Hiragana or Katakana. Lower
    /// conviction (the kana form is mechanically derivable from any
    /// romaji input — present even when there's no semantic JP word),
    /// merged AFTER pinyin in the host's dispatch as the "always-
    /// available fallback".
    pub fn kana_candidates(&self) -> Vec<String> {
        use inputx_jp::KanaKind;
        self.engine
            .candidates()
            .iter()
            .filter(|c| c.kind != KanaKind::Kanji)
            .map(|c| c.word.clone())
            .collect()
    }

    /// Commit by index into the engine's candidate list.
    pub fn commit_index(&mut self, i: usize) -> Option<String> {
        self.engine.commit_index(i)
    }
}
