//! JP engine state machine — parallel in shape to `inputx-wubi`'s
//! `WubiEngine` and `inputx-pinyin`'s segmenter so the composite layer
//! can plug it in next to the existing engines without bespoke wiring.
//!
//! State per session:
//!   - `buffer`: raw ASCII romaji as the user types (preedit source)
//!   - `candidates`: rebuilt on every keystroke, ordered hiragana →
//!     katakana → kanji-matching-current-on-yomi
//!
//! No auto-commit / freq-tuning / pin learning — those are wubi/pinyin
//! concerns and would only add ambiguity to a passthrough-style JP
//! engine at this MVP stage.

use crate::jukugo;
use crate::kanji;
use crate::romaji;

/// One JP candidate with its sub-source category, so the host UI can
/// optionally render a hint (kana vs kanji) on top of the W/P/J source
/// dot at the composite layer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub word: String,
    pub kind: KanaKind,
    /// 0–100 frequency score. Higher = more common in modern JP. Used
    /// by the composite-layer scoring (`japanese_adapter::candidates_with_scores`)
    /// to lift high-frequency JP entries above rare Chinese candidates
    /// per the user's design rule: JP-base < wubi/pinyin bases, but
    /// JP-high-freq must beat 中文难检字 + 生僻词组. For kana entries
    /// (mechanical romaji → kana rendering) freq is 0.
    pub freq: u32,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum KanaKind {
    Hiragana,
    Katakana,
    Kanji,
}

/// JP engine. One per session. Cheap to construct — all data is in
/// const tables in [`crate::romaji`] and [`crate::kanji`].
pub struct JapaneseEngine {
    buffer: Vec<u8>,
    candidates: Vec<Candidate>,
}

impl Default for JapaneseEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl JapaneseEngine {
    pub fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(8),
            candidates: Vec::with_capacity(8),
        }
    }

    /// Push one ASCII letter into the buffer and re-render candidates.
    /// Returns `true` if the letter was accepted (a-z, A-Z); non-letters
    /// are rejected without altering state.
    pub fn handle_letter(&mut self, c: u8) -> bool {
        if !c.is_ascii_alphabetic() {
            return false;
        }
        self.buffer.push(c.to_ascii_lowercase());
        self.refresh_candidates();
        true
    }

    /// Pop one byte off the buffer. Returns `true` if a byte was
    /// removed (i.e. buffer was non-empty), `false` if there was
    /// nothing to pop.
    pub fn backspace(&mut self) -> bool {
        if self.buffer.is_empty() {
            return false;
        }
        self.buffer.pop();
        self.refresh_candidates();
        true
    }

    /// Drop the buffer entirely. Returns `true` if something was
    /// dropped, `false` if buffer was already empty.
    pub fn escape(&mut self) -> bool {
        if self.buffer.is_empty() {
            return false;
        }
        self.buffer.clear();
        self.candidates.clear();
        true
    }

    /// Read-only buffer view as a string (for the preedit display).
    pub fn preedit(&self) -> &str {
        // Safe: buffer only ever contains ASCII bytes per handle_letter.
        std::str::from_utf8(&self.buffer).unwrap_or("")
    }

    pub fn is_composing(&self) -> bool {
        !self.buffer.is_empty()
    }

    pub fn candidates(&self) -> &[Candidate] {
        &self.candidates
    }

    /// Commit the candidate at `index`. Returns the committed text and
    /// clears the buffer. `None` if `index` is out of range (state
    /// unchanged).
    pub fn commit_index(&mut self, index: usize) -> Option<String> {
        let text = self.candidates.get(index)?.word.clone();
        self.buffer.clear();
        self.candidates.clear();
        Some(text)
    }

    fn refresh_candidates(&mut self) {
        self.candidates.clear();
        let s = std::str::from_utf8(&self.buffer).unwrap_or("");

        // Order within the JP block (the host appends after wubi/pinyin
        // via the composite layer; this is the *relative* order JP
        // candidates land in within the merged list):
        //
        //   1. 熟語 compound match (whole-buffer, e.g. nihon → 日本)
        //   2. Single-kanji by on/kun reading (e.g. watashi → 私)
        //   3. Hiragana of the full buffer
        //   4. Katakana of the full buffer
        //
        // Kanji forms come first because that's the actual conversion
        // target most users care about — kana is always available via
        // the row 3 / 4 fallback, but if 日本 appears at position 9
        // (after both kana variants) users rightly feel it's buried.
        // Standard JP IME workflow is: type romaji, see kanji
        // candidates immediately, fall back to kana via the lower
        // slots if none of the kanji matches intent.

        // Jukugo + single-kanji come WITH freq from the data tables.
        // Sort each group by freq desc so within-group ordering reflects
        // commonality (高 freq 88 before 香 freq 35 even though both
        // match "kou"). Cross-group ordering is then "jukugo, then
        // single-kanji, then kana" — the high-conviction kinds first.
        let mut jukugo_hits: Vec<(&str, u32)> = jukugo::lookup_by_reading(s).collect();
        jukugo_hits.sort_by(|a, b| b.1.cmp(&a.1));
        for (compound, freq) in jukugo_hits {
            self.candidates.push(Candidate {
                word: compound.to_string(),
                kind: KanaKind::Kanji,
                freq,
            });
        }

        let mut kanji_hits: Vec<(char, u32)> = kanji::lookup_by_reading(s).collect();
        kanji_hits.sort_by(|a, b| b.1.cmp(&a.1));
        for (kanji_char, freq) in kanji_hits {
            self.candidates.push(Candidate {
                word: kanji_char.to_string(),
                kind: KanaKind::Kanji,
                freq,
            });
        }

        let h = romaji::to_hiragana(s);
        if !h.is_empty() && h != s {
            self.candidates.push(Candidate {
                word: h.clone(),
                kind: KanaKind::Hiragana,
                freq: 0,
            });
        }
        let k = romaji::to_katakana(s);
        if !k.is_empty() && k != s && k != h {
            self.candidates.push(Candidate {
                word: k,
                kind: KanaKind::Katakana,
                freq: 0,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_letter_a_gives_hiragana_katakana() {
        let mut e = JapaneseEngine::new();
        assert!(e.handle_letter(b'a'));
        let cands = e.candidates();
        assert!(cands.iter().any(|c| c.word == "あ" && c.kind == KanaKind::Hiragana));
        assert!(cands.iter().any(|c| c.word == "ア" && c.kind == KanaKind::Katakana));
    }

    #[test]
    fn kanji_match_on_full_buffer_on_yomi() {
        let mut e = JapaneseEngine::new();
        for b in b"kou" {
            e.handle_letter(*b);
        }
        let cands = e.candidates();
        // First should be hiragana of "kou" → こう
        assert_eq!(cands[0].word, "こう");
        assert_eq!(cands[0].kind, KanaKind::Hiragana);
        // 高 must appear in the kanji portion.
        assert!(
            cands.iter().any(|c| c.word == "高" && c.kind == KanaKind::Kanji),
            "expected 高 in candidates for 'kou', got {:?}",
            cands
        );
    }

    #[test]
    fn multi_syllable_no_kanji() {
        // 'nihon' isn't a single-kanji on-yomi; the engine should produce
        // only the kana renderings (compound conversion is not v0.1 scope).
        let mut e = JapaneseEngine::new();
        for b in b"nihon" {
            e.handle_letter(*b);
        }
        let cands = e.candidates();
        assert!(cands.iter().any(|c| c.word == "にほん"));
        assert!(cands.iter().any(|c| c.word == "ニホン"));
        assert!(
            !cands.iter().any(|c| c.kind == KanaKind::Kanji),
            "no kanji expected for multi-syllable 'nihon', got {:?}", cands
        );
    }

    #[test]
    fn backspace_pops_and_re_renders() {
        let mut e = JapaneseEngine::new();
        for b in b"kou" {
            e.handle_letter(*b);
        }
        assert!(e.backspace());
        assert_eq!(e.preedit(), "ko");
        // 'ko' renders to こ + コ + any 'ko' on-yomi kanji.
        let cands = e.candidates();
        assert!(cands.iter().any(|c| c.word == "こ"));
    }

    #[test]
    fn backspace_on_empty_is_false() {
        let mut e = JapaneseEngine::new();
        assert!(!e.backspace());
    }

    #[test]
    fn escape_drops_buffer() {
        let mut e = JapaneseEngine::new();
        e.handle_letter(b'k');
        assert!(e.is_composing());
        assert!(e.escape());
        assert!(!e.is_composing());
        assert_eq!(e.preedit(), "");
    }

    #[test]
    fn commit_clears_state() {
        let mut e = JapaneseEngine::new();
        for b in b"kou" {
            e.handle_letter(*b);
        }
        let committed = e.commit_index(0).expect("commit");
        assert_eq!(committed, "こう");
        assert!(!e.is_composing());
        assert_eq!(e.candidates().len(), 0);
    }

    #[test]
    fn commit_out_of_range_returns_none() {
        let mut e = JapaneseEngine::new();
        e.handle_letter(b'a');
        assert!(e.commit_index(999).is_none());
        // State unchanged.
        assert!(e.is_composing());
    }

    #[test]
    fn non_letter_rejected_without_state_change() {
        let mut e = JapaneseEngine::new();
        assert!(!e.handle_letter(b'5'));
        assert!(!e.handle_letter(b' '));
        assert_eq!(e.preedit(), "");
    }
}
