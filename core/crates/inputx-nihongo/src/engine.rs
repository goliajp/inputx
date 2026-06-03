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
    /// `true` for `compose_sentence` products — mechanical (content +
    /// particle/copula) sentence guesses like 私は or, for non-Japanese
    /// romaji, junk like 時へ時. These are LOW confidence: unlike a real
    /// dictionary 熟語 (新宿) they must never be treated as jukugo nor
    /// trigger the full-match promote, and must score below real Chinese
    /// words so they don't pollute Chinese pinyin input. See
    /// `japanese_adapter::candidates_with_scores`.
    pub composed: bool,
    /// Prefix-prediction proximity in thousandths: 1000 = the buffer is the
    /// full reading (a normal/exact candidate); < 1000 = this is a PREDICTED
    /// candidate whose reading the buffer is only a prefix of (shinjuk → 新宿
    /// at 7/8 = 875). `japanese_adapter` decays the freq contribution by
    /// `(proximity/1000)^K` and excludes < 1000 from the full-match promote
    /// (the buffer isn't the complete word yet). See PLAN-prefix-prediction.md.
    pub proximity_milli: u16,
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
    /// Returns `true` if the letter was accepted; rejected letters leave
    /// state unchanged so the IME controller can route the byte elsewhere
    /// (locale punct mapping, raw passthrough).
    ///
    /// Accepted bytes:
    ///   - `a-z`, `A-Z` (the romaji alphabet — lowercased on push)
    ///   - `-` *only when the buffer is non-empty* — chōonpu (ー / long-vowel
    ///     mark). `romaji::TABLE` maps `-` to `ー` so a buffer of `"koohi-"`
    ///     renders as `コーヒー`. Empty-buffer `-` is rejected so a stranded
    ///     hyphen still routes to the host as ASCII punctuation rather than
    ///     becoming a leading `ー` with no preceding syllable. User-reported
    ///     2026-05-27: `-` must be typeable as JP chōonpu, otherwise long-
    ///     vowel words like コーヒー are unreachable.
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

        // Sentence-level segmentation FIRST: try splitting buffer into
        // (content-word prefix + particle/copula suffix). When both
        // halves match the data tables, emit a composed candidate like
        // "私は" for buffer "watashiwa". This is what makes JP feel
        // like a real IME instead of a romaji→kana renderer.
        for composed in compose_sentence(s) {
            self.candidates.push(composed);
        }

        // Jukugo + single-kanji come WITH freq from the data tables.
        // Sort each group by freq desc so within-group ordering reflects
        // commonality (高 freq 88 before 香 freq 35 even though both
        // match "kou"). Cross-group ordering is then "jukugo, then
        // single-kanji, then kana" — the high-conviction kinds first.
        let mut jukugo_hits: Vec<(&str, u32)> = jukugo::lookup_by_reading(s).collect();
        jukugo_hits.sort_by(|a, b| b.1.cmp(&a.1));
        let had_exact_jukugo = !jukugo_hits.is_empty();
        for (compound, freq) in jukugo_hits {
            self.candidates.push(Candidate {
                word: compound.to_string(),
                kind: KanaKind::Kanji,
                freq,
                composed: false,
                proximity_milli: 1000,
            });
        }

        // Prefix PREDICTION (PLAN-prefix-prediction.md CP-A): the user is
        // mid-typing toward a jukugo — shinjuk → 新宿 (しんじゅく). Fire only
        // when there's NO exact jukugo (an exact match means the word is
        // complete; adding its extensions would be noise — same principle as
        // pinyin's lianxiang→联想), and only for buffers long enough that
        // proximity carries signal. Each predicted candidate records its
        // proximity so japanese_adapter can decay the freq by proximity^K and
        // keep it below an exact/full match.
        if !had_exact_jukugo && s.len() >= 3 {
            let mut pred: Vec<(&str, u32, usize)> =
                jukugo::lookup_by_reading_prefix(s).collect();
            pred.sort_by(|a, b| b.1.cmp(&a.1));
            for (kanji, freq, reading_len) in pred.into_iter().take(8) {
                let proximity_milli = ((s.len() * 1000) / reading_len.max(1)) as u16;
                self.candidates.push(Candidate {
                    word: kanji.to_string(),
                    kind: KanaKind::Kanji,
                    freq,
                    composed: false,
                    proximity_milli,
                });
            }
        }

        let mut kanji_hits: Vec<(char, u32)> = kanji::lookup_by_reading(s).collect();
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

        // Hiragana / katakana fallback. Freq drives where these land in
        // the cross-engine merge (composite::japanese_adapter uses
        // `base + freq * multiplier`). Short buffers (1-2 chars) get
        // high freq — typing `e` or `ka` should surface `え` / `か` in
        // the visible top, not bury them under every pinyin variant.
        // Long buffers get lower freq — for `konnichi`, the user
        // probably wants `今日` not `こんにち`.
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

// SENTENCE_SUFFIXES / KANJI_SUFFIXES retired 2026-06-03 as Rust constants
// per project policy `.claude/RANKING-MODEL-INVARIANTS.md` ("no special
// lists in code"). Data lives in `tools/scoring/data/jp_sentence_
// suffixes_v1.tsv` and `jp_kanji_suffixes_v1.tsv`; both are parsed once
// at first use via OnceLock and exposed through public accessors so the
// composite-side carve-out (`crates/inputx-core/src/japanese/compose.rs`)
// can share the single source of truth.

const SENTENCE_SUFFIXES_TSV: &str = include_str!(
    "../../../../tools/scoring/data/jp_sentence_suffixes_v1.tsv"
);
const KANJI_SUFFIXES_TSV: &str = include_str!(
    "../../../../tools/scoring/data/jp_kanji_suffixes_v1.tsv"
);

/// Parse `<a>\t<b>[\t# comment]` rows, skipping blank lines and
/// comment-only lines. Returns `&'static` slices because the input is
/// embedded by `include_str!` (lives forever in the binary).
fn parse_pairs(src: &'static str) -> Vec<(&'static str, &'static str)> {
    let mut out = Vec::new();
    for raw in src.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        let mut parts = line.splitn(3, '\t');
        let (Some(a), Some(b)) = (parts.next(), parts.next()) else { continue };
        let a = a.trim();
        let b = b.trim();
        if a.is_empty() || b.is_empty() { continue; }
        out.push((a, b));
    }
    out
}

/// Sentence-final particles / copulas for greedy suffix stripping in
/// `compose_one_segment`. Order matters: longest first (encoded in the
/// TSV). Used by composite-side composer too via
/// `inputx_nihongo::engine::sentence_suffixes`.
pub fn sentence_suffixes() -> &'static [(&'static str, &'static str)] {
    static CACHE: std::sync::OnceLock<Vec<(&'static str, &'static str)>>
        = std::sync::OnceLock::new();
    CACHE.get_or_init(|| parse_pairs(SENTENCE_SUFFIXES_TSV)).as_slice()
}

/// Productive category-suffix kanji for "jukugo + suffix" composition
/// (東京+都 = 東京都). Whitelisted to keep the composition from emitting
/// junk like 東京渡 (渡 also reads `to`).
pub fn kanji_suffixes() -> &'static [(&'static str, &'static str)] {
    static CACHE: std::sync::OnceLock<Vec<(&'static str, &'static str)>>
        = std::sync::OnceLock::new();
    CACHE.get_or_init(|| parse_pairs(KANJI_SUFFIXES_TSV)).as_slice()
}

/// Single-segment compose: (content_word, particle/copula_suffix).
/// Returns (composed_word_string, content_freq) pairs.
///
/// For `watashiwa`: prefix=`watashi`, suffix=`wa` → 私+は.
/// For `nihondesu`:  prefix=`nihon`,   suffix=`desu` → 日本+です.
fn compose_one_segment(buffer: &str) -> Vec<(String, u32)> {
    let mut out: Vec<(String, u32)> = Vec::new();
    for (s_reading, s_kana) in sentence_suffixes() {
        if let Some(prefix) = buffer.strip_suffix(s_reading) {
            if prefix.is_empty() {
                continue;
            }
            for (compound, freq) in jukugo::lookup_by_reading(prefix) {
                out.push((format!("{compound}{s_kana}"), freq));
            }
            for (ch, freq) in kanji::lookup_by_reading(prefix) {
                out.push((format!("{ch}{s_kana}"), freq));
            }
        }
    }
    out
}

/// Try every (prefix, suffix) split of `buffer`. Returns sentence
/// candidates from both 1-segment and 2-segment compositions:
///
///   * 1-segment: (content + particle/copula). Handles "watashiwa".
///   * 2-segment: split buffer at every position; left → 1seg, right →
///     1seg; concatenate. Handles "watashiwagakuseidesu" → 私は + 学生
///     です = 私は学生です.
///
/// Capped at 30 results; freq for multi-segment compositions is min
/// of segment freqs × 0.7 (multi-segment penalty so single-word direct
/// matches still rank higher when present).
fn compose_sentence(buffer: &str) -> Vec<Candidate> {
    let mut hits: Vec<(String, u32)> = Vec::new();

    // 1-segment
    for (word, freq) in compose_one_segment(buffer) {
        hits.push((word, freq));
    }

    // jukugo + category-suffix kanji (東京+都 = 東京都). Productive admin /
    // category compounds the dict doesn't (and shouldn't) enumerate. Prefix
    // MUST be a real jukugo so single-kanji noise (都+市) can't form; suffix
    // is whitelisted (`kanji_suffixes()`) so 東京渡-style junk can't form either.
    for (sfx_read, sfx_kanji) in kanji_suffixes() {
        if let Some(prefix) = buffer.strip_suffix(sfx_read) {
            if prefix.is_empty() {
                continue;
            }
            for (compound, freq) in jukugo::lookup_by_reading(prefix) {
                hits.push((format!("{compound}{sfx_kanji}"), freq));
            }
        }
    }

    // 1-segment WITH bare content tail (no final particle/copula).
    // Handles "watashinonihon": (私+の) + 日本 where the right half is
    // a bare content word, no suffix. Look up every split where left
    // is a 1-seg compose AND right is directly a kanji/jukugo entry.
    for split in 2..buffer.len() {
        let left = &buffer[..split];
        let right = &buffer[split..];
        let lefts = compose_one_segment(left);
        if lefts.is_empty() {
            continue;
        }
        // Right: direct jukugo or kanji lookup (no particle suffix).
        let mut right_hits: Vec<(String, u32)> = Vec::new();
        for (compound, freq) in jukugo::lookup_by_reading(right) {
            right_hits.push((compound.to_string(), freq));
        }
        for (ch, freq) in kanji::lookup_by_reading(right) {
            right_hits.push((ch.to_string(), freq));
        }
        for (lw, lf) in &lefts {
            for (rw, rf) in &right_hits {
                let combined = format!("{lw}{rw}");
                let combined_freq = ((*lf.min(rf) as f64) * 0.65) as u32;
                hits.push((combined, combined_freq));
            }
        }
    }

    // 2-segment: walk every split point. Only proceed when both halves
    // produce SOMETHING — bails early on barren splits to keep cost
    // bounded for nonsense input.
    if buffer.len() >= 4 {
        // Skip first/last few chars — single-letter halves can't form
        // a meaningful (content + particle) composition.
        for split in 2..buffer.len() - 1 {
            let left = &buffer[..split];
            let right = &buffer[split..];
            let lefts = compose_one_segment(left);
            if lefts.is_empty() {
                continue;
            }
            let rights = compose_one_segment(right);
            if rights.is_empty() {
                continue;
            }
            for (lw, lf) in &lefts {
                for (rw, rf) in &rights {
                    let combined = format!("{lw}{rw}");
                    let combined_freq =
                        ((*lf.min(rf) as f64) * 0.7) as u32;
                    hits.push((combined, combined_freq));
                }
            }
        }
    }

    // Dedup by word, keeping highest freq.
    let mut best: std::collections::HashMap<String, u32> =
        std::collections::HashMap::new();
    for (w, f) in hits {
        let entry = best.entry(w).or_insert(0);
        if f > *entry {
            *entry = f;
        }
    }
    let mut sorted: Vec<(String, u32)> = best.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    sorted.truncate(30);
    sorted
        .into_iter()
        .map(|(word, freq)| Candidate {
            word,
            kind: KanaKind::Kanji,
            // Composed candidates already include their own penalty
            // (0.7 for multi-seg, raw for 1-seg). Apply a final 0.85
            // mild penalty here so a direct-jukugo whole-buffer match
            // (no compose, raw freq from data) still wins ties.
            freq: ((freq as f64) * 0.85) as u32,
            composed: true,
            proximity_milli: 1000,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chouonpu_hyphen_extends_buffer_when_composing() {
        // Mozc-standard romaji for コーヒー is `ko-hi-` (chōonpu enters
        // literally as `-` → ー). Buffer "ko-hi-" renders コーヒー /
        // こーひー via romaji::TABLE entry e("-", "ー", "ー").
        // Bare-letter `koohi` would render コオヒー (literal oo→オオ),
        // which is correct kana for that romaji but not the chōonpu form.
        let mut e = JapaneseEngine::new();
        for b in b"ko" { assert!(e.handle_letter(*b)); }
        assert!(e.handle_letter(b'-'), "`-` must be accepted as chouonpu mid-composition");
        for b in b"hi" { assert!(e.handle_letter(*b)); }
        assert!(e.handle_letter(b'-'), "trailing `-` also accepted");
        let cands = e.candidates();
        assert!(cands.iter().any(|c| c.word == "コーヒー"),
            "expected コーヒー (katakana with chouonpu) among candidates, got {:?}",
            cands.iter().map(|c| &c.word).collect::<Vec<_>>());
        assert!(cands.iter().any(|c| c.word == "こーひー"),
            "expected こーひー (hiragana with chouonpu) among candidates");
    }

    #[test]
    fn chouonpu_hyphen_literal_oo_no_auto_chouonpu() {
        // Sanity guard for the chōonpu rule: `koo` does NOT auto-convert
        // double-o to ー (matches mozc/Google IME behavior — chōonpu must
        // be entered explicitly via `-`). User typing `koohi` gets
        // コオヒ / こおひ, not コーヒ.
        let mut e = JapaneseEngine::new();
        for b in b"koo" { assert!(e.handle_letter(*b)); }
        let cands = e.candidates();
        assert!(cands.iter().any(|c| c.word == "コオ"),
            "expected コオ for `koo`; got {:?}", cands.iter().map(|c| &c.word).collect::<Vec<_>>());
        assert!(!cands.iter().any(|c| c.word.contains("コー") && c.word.chars().count() <= 2),
            "double-o must not auto-convert to chōonpu");
    }

    #[test]
    fn chouonpu_hyphen_rejected_on_empty_buffer() {
        // Empty-buffer `-` must reject so the IME controller routes it as
        // punctuation, not as a stranded ー.
        let mut e = JapaneseEngine::new();
        assert!(!e.handle_letter(b'-'), "empty-buffer `-` must reject");
        assert_eq!(e.preedit(), "", "rejected `-` must leave buffer empty");
    }

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
        // 高 must appear in the kanji portion.
        assert!(
            cands.iter().any(|c| c.word == "高" && c.kind == KanaKind::Kanji),
            "expected 高 in candidates for 'kou', got {:?}",
            cands
        );
        // Hiragana fallback こう must always be available.
        assert!(
            cands.iter().any(|c| c.word == "こう" && c.kind == KanaKind::Hiragana),
            "expected こう (hiragana) in candidates for 'kou', got {:?}",
            cands
        );
    }

    #[test]
    fn multi_syllable_finds_jukugo() {
        // 'nihon' is a high-freq jukugo (日本); kanji form should appear
        // alongside the kana renderings.
        let mut e = JapaneseEngine::new();
        for b in b"nihon" {
            e.handle_letter(*b);
        }
        let cands = e.candidates();
        assert!(
            cands.iter().any(|c| c.word == "日本" && c.kind == KanaKind::Kanji),
            "expected 日本 (kanji jukugo) for 'nihon', got {:?}",
            cands
        );
        assert!(cands.iter().any(|c| c.word == "にほん"));
        assert!(cands.iter().any(|c| c.word == "ニホン"));
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

    /// Counter-word coverage — the Round-8 additions. Each input should
    /// produce the corresponding counter-word kanji *as a direct jukugo
    /// hit* (not via sentence segmentation), which means it has to be in
    /// the jukugo table after the counters TSV is merged.
    #[test]
    fn counter_words_round8() {
        let cases: &[(&[u8], &str)] = &[
            (b"ikko", "一個"),
            (b"hitori", "一人"),
            (b"futari", "二人"),
            (b"sannin", "三人"),
            (b"yonin", "四人"),
            (b"mikka", "三日"),
            (b"yokka", "四日"),
            (b"tsuitachi", "一日"),
            (b"futsuka", "二日"),
            (b"ippon", "一本"),
            (b"sanbon", "三本"),
            (b"ichimai", "一枚"),
            (b"sanbiki", "三匹"),
            (b"ittou", "一頭"),
            (b"ikkai", "一回"),
            (b"isshuukan", "一週間"),
            (b"ikkagetsu", "ヶ月"), // checks the ヶ月 suffix (一ヶ月 below)
            (b"yoji", "四時"),
            (b"kuji", "九時"),
            (b"ippun", "一分"),
            (b"juppun", "十分"),
            (b"ippai", "一杯"),
            (b"issatsu", "一冊"),
            (b"hitotsu", "一つ"),
            (b"mittsu", "三つ"),
            (b"daiichi", "第一"),
            (b"hatachi", "二十歳"),
        ];
        for (input, expected_substr) in cases {
            let mut e = JapaneseEngine::new();
            for b in *input {
                e.handle_letter(*b);
            }
            let cands = e.candidates();
            assert!(
                cands.iter().any(|c| c.word.contains(expected_substr)),
                "expected a candidate containing `{}` for input `{}`, got {:?}",
                expected_substr,
                std::str::from_utf8(input).unwrap(),
                cands.iter().map(|c| &c.word).collect::<Vec<_>>()
            );
        }
    }

    /// The full kanji form (一ヶ月) should appear specifically, not only the
    /// suffix — the test above accepts substring "ヶ月" so it doesn't
    /// false-pass on bare suffix; verify the leading 一 explicitly here.
    #[test]
    fn ikkagetsu_renders_full_kanji() {
        let mut e = JapaneseEngine::new();
        for b in b"ikkagetsu" {
            e.handle_letter(*b);
        }
        let cands = e.candidates();
        assert!(
            cands.iter().any(|c| c.word == "一ヶ月"),
            "expected `一ヶ月` for `ikkagetsu`, got {:?}",
            cands.iter().map(|c| &c.word).collect::<Vec<_>>()
        );
    }

    /// Brand / chain / SNS / IT-platform coverage — the Round-9 wrap-up
    /// to land Inputx at "basically on par with Simeji" on the data side.
    /// Each input must surface the corresponding form somewhere in the
    /// candidate list.
    #[test]
    fn brands_round9() {
        let cases: &[(&[u8], &str)] = &[
            (b"sutaba", "スタバ"),
            (b"yunikuro", "ユニクロ"),
            (b"amazon", "アマゾン"),
            (b"sebun", "セブン"),
            (b"famima", "ファミマ"),
            (b"rouson", "ローソン"),
            (b"donki", "ドンキ"),
            (b"toyota", "トヨタ"),
            (b"sonii", "ソニー"),
            (b"nintendou", "任天堂"),
            (b"yuuchuubu", "ユーチューブ"),
            (b"insuta", "インスタ"),
            (b"tikkutokku", "ティックトック"),
            (b"merukari", "メルカリ"),
            (b"peipei", "ペイペイ"),
            (b"rain", "ライン"),
            (b"netofuri", "ネトフリ"),
            (b"shinkansen", "新幹線"),
            (b"makku", "マック"),
            (b"yoshinoya", "吉野家"),
            (b"sukiya", "すき家"),
            (b"ichiran", "一蘭"),
            (b"jiburi", "ジブリ"),
            (b"pokemon", "ポケモン"),
            (b"suika", "スイカ"),
        ];
        for (input, expected) in cases {
            let mut e = JapaneseEngine::new();
            for b in *input {
                e.handle_letter(*b);
            }
            let cands = e.candidates();
            assert!(
                cands.iter().any(|c| c.word == *expected),
                "expected `{}` for `{}`, got {:?}",
                expected,
                std::str::from_utf8(input).unwrap(),
                cands.iter().map(|c| &c.word).collect::<Vec<_>>()
            );
        }
    }

    /// Counter words must rank ABOVE the bare kana renderings — typing
    /// `hitori` should surface 一人 at the top, not the hiragana ひとり as
    /// candidate #0. Without this, the polish round is invisible to users
    /// who hit Space (which commits #0).
    #[test]
    fn counter_word_outranks_kana() {
        let mut e = JapaneseEngine::new();
        for b in b"hitori" {
            e.handle_letter(*b);
        }
        let cands = e.candidates();
        let one_person_idx = cands.iter().position(|c| c.word == "一人");
        let hiragana_idx = cands.iter().position(|c| c.word == "ひとり");
        assert!(
            one_person_idx.is_some(),
            "no `一人` candidate for `hitori`, got {:?}",
            cands.iter().map(|c| &c.word).collect::<Vec<_>>()
        );
        if let (Some(o), Some(h)) = (one_person_idx, hiragana_idx) {
            assert!(
                o < h,
                "`一人` (#{o}) should rank above `ひとり` (#{h}), got {:?}",
                cands.iter().map(|c| &c.word).collect::<Vec<_>>()
            );
        }
    }
}
