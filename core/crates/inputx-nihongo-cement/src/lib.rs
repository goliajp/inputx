//! `inputx-nihongo-cement` — Japanese-specific consumer-engine cement.
//!
//! Cement layer (see `.claude/PLAN-stones-extract.md` "Cement catalog")
//! built on top of the [`inputx_nihongo`] facade plus
//! [`inputx_dict_format`] (IDFv1 dict). Public surface (populated by
//! v1.4.6 sub-phase C work):
//!
//! - `NihongoIdfLookup` — IdfReader-driven jukugo / kanji lookup
//!   adapter that replaces direct `JUKUGO_TABLE` / `KANJI_TABLE`
//!   const-table scans in the composite japanese adapter.
//! - chouonpu plumbing helpers (jukugo/kanji-specific romaji-to-kana
//!   bridges that are nihongo-only).
//!
//! The split: [`inputx_nihongo`] facade ships data + primitives
//! (romaji / kanji / jukugo / brands / counters / engine / session
//! state machine) and is publish-quality for any JP IME implementer.
//! `inputx-nihongo-cement` is Inputx-specific plumbing.
//!
//! `japanese_adapter` (state machine driven by composite engine
//! keystrokes) lives in inputx-core/composite/ — it's **composite root
//! cement** because it shares cross-engine `mode` + `scoring` +
//! `merge` modules with wubi/pinyin paths.

// Skeleton — content populated by v1.4.6 sub-phase C work
// (PinyinIdfLookup pattern applied to nihongo: NihongoIdfLookup +
// jukugo prefix prediction helpers).

#[cfg(test)]
mod tests {
    #[test]
    fn skeleton_compiles() {
        // Placeholder until C-phase populates the lookup adapter.
    }
}
