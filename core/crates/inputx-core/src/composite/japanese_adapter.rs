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

    /// Materialize the engine's candidates into `Vec<String>` — the
    /// shape the merge layer expects. Cheap (≤8 small allocations
    /// per call; the engine caps at hiragana + katakana + ≤6 kanji).
    pub fn candidates(&self) -> Vec<String> {
        self.engine.candidates().iter().map(|c| c.word.clone()).collect()
    }

    /// Commit by index into the engine's candidate list.
    pub fn commit_index(&mut self, i: usize) -> Option<String> {
        self.engine.commit_index(i)
    }
}
