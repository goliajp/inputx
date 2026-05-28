//! Composite-side JapaneseEngine — v1.5.1 WU-κ carve-out.
//!
//! API surface intentionally matches `inputx_nihongo::JapaneseEngine`
//! so `crate::composite::japanese_adapter::JapaneseAdapter` can swap
//! the engine type with no other changes. The facade engine remains
//! intact for direct-facade consumers (notably `inputx-nihongo-wasm`).
//!
//! Difference vs the facade: jukugo / kanji corpus lookups route
//! through the cement IDF readers (`inputx_nihongo_cement::
//! nihongo_{jukugo,kanji}_idf_reader`) instead of the facade
//! `JUKUGO_TABLE` / `KANJI_TABLE` const-table iteration. The IDF
//! blobs are byte-equivalent to the const tables (same
//! `idf-from-nihongo-*` build tooling sources both), so baseline
//! fixture diff stays at zero.

use inputx_nihongo::{romaji, Candidate, KanaKind};

use super::compose::compose_sentence;
use super::lookup::{
    lookup_jukugo_by_reading, lookup_jukugo_by_reading_prefix,
    lookup_kanji_by_reading,
};

/// Composite-side JapaneseEngine. Cheap to construct — cement IDF
/// readers are process-global `OnceLock` instances; engine state is
/// only an owned buffer + candidate `Vec`.
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

    /// Push one ASCII letter / chōonpu `-` into the buffer and re-
    /// render candidates. Same contract as
    /// `inputx_nihongo::JapaneseEngine::handle_letter` — see that
    /// crate's docs for the chōonpu semantics + accepted byte set.
    pub fn handle_letter(&mut self, c: u8) -> bool {
        if c == b'-' {
            if self.buffer.is_empty() {
                return false;
            }
            self.buffer.push(b'-');
            self.refresh_candidates();
            return true;
        }
        if !c.is_ascii_alphabetic() {
            return false;
        }
        self.buffer.push(c.to_ascii_lowercase());
        self.refresh_candidates();
        true
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
        true
    }

    pub fn preedit(&self) -> &str {
        std::str::from_utf8(&self.buffer).unwrap_or("")
    }

    pub fn is_composing(&self) -> bool {
        !self.buffer.is_empty()
    }

    pub fn candidates(&self) -> &[Candidate] {
        &self.candidates
    }

    pub fn commit_index(&mut self, index: usize) -> Option<String> {
        let text = self.candidates.get(index)?.word.clone();
        self.buffer.clear();
        self.candidates.clear();
        Some(text)
    }

    /// Re-emit the full candidate list. Mirrors the facade's
    /// `refresh_candidates` block-for-block:
    ///
    ///   1. `compose_sentence` (multi-segment particle/copula glue)
    ///   2. exact jukugo by reading (Kanji kind, freq from IDF)
    ///   3. jukugo prefix-prediction (CP-A; only when no exact
    ///      jukugo AND buffer ≥3 chars; top 8 by freq with
    ///      proximity_milli)
    ///   4. exact single-kanji by reading
    ///   5. hiragana of full buffer (freq 100 ≤2 chars else 30)
    ///   6. katakana of full buffer (same freq; skip if identical
    ///      to hiragana — pure-katakana romaji edge case)
    fn refresh_candidates(&mut self) {
        self.candidates.clear();
        let s = std::str::from_utf8(&self.buffer).unwrap_or("");

        for composed in compose_sentence(s) {
            self.candidates.push(composed);
        }

        let mut jukugo_hits: Vec<(String, u32)> = lookup_jukugo_by_reading(s);
        jukugo_hits.sort_by(|a, b| b.1.cmp(&a.1));
        let had_exact_jukugo = !jukugo_hits.is_empty();
        for (compound, freq) in jukugo_hits {
            self.candidates.push(Candidate {
                word: compound,
                kind: KanaKind::Kanji,
                freq,
                composed: false,
                proximity_milli: 1000,
            });
        }

        if !had_exact_jukugo && s.len() >= 3 {
            let mut pred: Vec<(String, u32, usize)> =
                lookup_jukugo_by_reading_prefix(s);
            pred.sort_by(|a, b| b.1.cmp(&a.1));
            for (kanji, freq, reading_len) in pred.into_iter().take(8) {
                let proximity_milli = ((s.len() * 1000) / reading_len.max(1)) as u16;
                self.candidates.push(Candidate {
                    word: kanji,
                    kind: KanaKind::Kanji,
                    freq,
                    composed: false,
                    proximity_milli,
                });
            }
        }

        let mut kanji_hits: Vec<(char, u32)> = lookup_kanji_by_reading(s);
        kanji_hits.sort_by(|a, b| b.1.cmp(&a.1));
        for (kanji_char, freq) in kanji_hits {
            self.candidates.push(Candidate {
                word: kanji_char.to_string(),
                kind: KanaKind::Kanji,
                freq,
                composed: false,
                proximity_milli: 1000,
            });
        }

        let kana_freq: u32 = if s.len() <= 2 { 100 } else { 30 };
        let h = romaji::to_hiragana(s);
        if !h.is_empty() && h != s {
            self.candidates.push(Candidate {
                word: h.clone(),
                kind: KanaKind::Hiragana,
                freq: kana_freq,
                composed: false,
                proximity_milli: 1000,
            });
        }
        let k = romaji::to_katakana(s);
        if !k.is_empty() && k != s && k != h {
            self.candidates.push(Candidate {
                word: k,
                kind: KanaKind::Katakana,
                freq: kana_freq,
                composed: false,
                proximity_milli: 1000,
            });
        }
    }
}
