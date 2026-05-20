//! Reverse lookup — `char → Vec<pinyin>`.
//!
//! Powered by traversing the embedded dict on-demand. v0.1 builds the
//! reverse index lazily on first call (small bootstrap → cheap); v0.2 will
//! materialize it at compile time once the dict grows.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::dict::PinyinDict;

/// A single character's pinyin readings, in the order they were first
/// encountered while traversing the dict.
type CharReadings = Vec<String>;

/// Lazy global reverse index. Populated on first call; never invalidated
/// (the embedded dict is immutable in v0.1).
static REVERSE: OnceLock<HashMap<char, CharReadings>> = OnceLock::new();

fn reverse_index() -> &'static HashMap<char, CharReadings> {
    REVERSE.get_or_init(|| {
        let dict = PinyinDict::embedded();
        // We can't access the FST through the public PinyinDict API after
        // construction, so re-walk the same bytes. The embedded ctor is
        // cheap; this only runs once.
        let mut map: HashMap<char, CharReadings> = HashMap::new();
        let raw = build_walk(&dict);
        for (pinyin, word) in raw {
            // Single-char entries contribute the most reliable readings.
            // Multi-char entries' readings are only assigned via segmentation,
            // which is out of scope for v0.1's reverse index.
            let mut chars = word.chars();
            let (Some(first), None) = (chars.next(), chars.next()) else {
                continue;
            };
            let readings = map.entry(first).or_default();
            if !readings.contains(&pinyin) {
                readings.push(pinyin);
            }
        }
        map
    })
}

fn build_walk(dict: &PinyinDict) -> Vec<(String, String)> {
    let mut out = Vec::new();
    // Re-open the embedded bytes through the public prefix("") to get the full set.
    // (PinyinDict doesn't expose `Map` directly to keep the surface minimal.)
    out.extend(dict.prefix(""));
    // Sanity: prefix("") returns everything sorted by (pinyin, word).
    out
}

/// Pinyin readings for a single Han character. Returns an empty `Vec` if the
/// character isn't in the bootstrap dict (most chars won't be in v0.1; v0.2
/// expands coverage to ~67k via Unihan + corpus pipeline).
pub fn char_to_pinyin(c: char) -> Vec<String> {
    reverse_index().get(&c).cloned().unwrap_or_default()
}

/// Number of Han characters with at least one reading in the reverse index.
/// Useful for sanity checks; not a meaningful coverage metric in v0.1.
pub fn covered_char_count() -> usize {
    reverse_index().len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_chars_have_readings() {
        for (c, want) in [('我', "wo"), ('你', "ni"), ('好', "hao"), ('中', "zhong")] {
            let readings = char_to_pinyin(c);
            assert!(
                readings.iter().any(|p| p == want),
                "{c} should include reading {want:?}, got {readings:?}"
            );
        }
    }

    #[test]
    fn unknown_char_yields_empty() {
        // Use a Private Use Area codepoint — guaranteed never in Unihan
        // (PUA is reserved for application-specific assignments). The full
        // v0.2 dict covers Ext B-G so previously-archaic CJK codepoints
        // (e.g., 𤴓 U+24D13) now have readings.
        assert!(char_to_pinyin('\u{E000}').is_empty());
        assert!(char_to_pinyin('\u{F8FF}').is_empty());
    }

    #[test]
    fn covered_count_reasonable() {
        let n = covered_char_count();
        assert!(n >= 50, "expected ≥50 single-char entries, got {n}");
    }
}
