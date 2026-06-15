//! Phase-5 CP-5.2 cell-dict pack — user-installable domain vocab.
//!
//! A cell-dict is a TOML file shipping a small list of `(pinyin, word,
//! freq)` entries the user wants to bias their candidate list with.
//! Examples that motivate the feature: an IT person wants `weifuwu` to
//! always lead with 微服务, a finance person wants `peizhi` to lead
//! with 配置 over the more frequent 陪侄, etc.
//!
//! Packs are loaded at runtime via [`crate::PinyinDict::load_cell_dict`]
//! — there's no compiled-in cell-dict. This keeps the engine binary
//! free of domain-specific data while letting any user install whichever
//! packs match their work. Multiple `load_cell_dict` calls accumulate;
//! [`crate::PinyinDict::clear_cell_dict`] wipes the layer.
//!
//! # Schema
//!
//! ```toml
//! [meta]
//! name = "IT 术语"
//! version = 1
//! author = "Inputx team"
//! description = "Common IT / cloud / DevOps vocabulary"
//! license = "CC0-1.0"
//!
//! [[entry]]
//! pinyin = "weifuwu"
//! word = "微服务"
//! freq = 500000      # same scale as embedded L1 freq_score
//!
//! [[entry]]
//! pinyin = "yunjisuan"
//! word = "云计算"
//! freq = 500000
//! ```
//!
//! `freq` is on the SAME numeric scale as the embedded dict's
//! `freq_score` (corpus-derived, 0..1_000_000-ish). A pack author who
//! wants their term to top a competing L1 entry needs to pick a `freq`
//! that beats the L1 score — `500_000` is a safe "high-priority"
//! default for a curated pack.
//!
//! # Validation
//!
//! The loader does not normalize pinyin (caller's job to write valid
//! syllables) and does not validate words against L1 — cell-dict
//! entries deliberately ADD vocabulary the embedded dict doesn't have,
//! so an L1-existence check would be wrong. Bad TOML returns
//! [`ParseError`]; everything else is accepted as-is.

#[cfg(feature = "cell-dict")]
use serde::Deserialize;

/// Parsed cell-dict file: metadata + entry list.
#[cfg(feature = "cell-dict")]
#[derive(Debug, Deserialize)]
pub struct CellDict {
    /// Pack metadata — name, version, author, description, license.
    pub meta: Meta,
    /// All `(pinyin, word, freq)` mappings the pack ships. TOML side
    /// uses singular `[[entry]]`; we keep the Rust field plural to
    /// read fluently.
    #[serde(default, rename = "entry")]
    pub entries: Vec<Entry>,
}

/// Optional metadata block. Only `name` is required; everything else
/// defaults to an empty string / 0 so older packs stay loadable as the
/// schema grows.
#[cfg(feature = "cell-dict")]
#[derive(Debug, Deserialize)]
pub struct Meta {
    /// Display name of the pack (shown in Settings).
    pub name: String,
    /// Pack schema version (default 0). Reserved for future migrations.
    #[serde(default)]
    pub version: u32,
    /// Free-text author / maintainer identifier.
    #[serde(default)]
    pub author: String,
    /// One-line summary of what the pack contains.
    #[serde(default)]
    pub description: String,
    /// SPDX license identifier (e.g. "CC0-1.0", "MIT"). Optional.
    #[serde(default)]
    pub license: String,
}

/// A single `(pinyin, word, freq)` mapping. `freq` defaults to 0 when
/// omitted — entries with freq 0 still surface in lookups but won't
/// outrank any non-trivial L1 entry, which is usually what the pack
/// author intended when they don't bother setting it.
#[cfg(feature = "cell-dict")]
#[derive(Debug, Deserialize)]
pub struct Entry {
    /// Lowercase pinyin lookup key (e.g. `"yunjisuan"`). Passed
    /// through [`crate::normalize_lookup_key`] at load time, so
    /// `lue/nue` alias forms are accepted.
    pub pinyin: String,
    /// The candidate word the user wants surfaced for `pinyin`
    /// (e.g. `"云计算"`).
    pub word: String,
    /// Frequency on the same scale as the embedded dict's
    /// `freq_score` (corpus-derived, roughly 0..1_000_000). Defaults
    /// to 0.
    #[serde(default)]
    pub freq: u64,
}

#[cfg(feature = "cell-dict")]
impl CellDict {
    /// Parse a TOML string. Returns [`ParseError`] for invalid TOML or
    /// missing required fields.
    pub fn from_toml_str(s: &str) -> Result<Self, ParseError> {
        toml::from_str(s).map_err(|e| ParseError(e.to_string()))
    }
}

/// Parse / load failure. Wraps the underlying toml error string —
/// callers should not need to introspect it programmatically, only
/// log / surface.
#[cfg(feature = "cell-dict")]
#[derive(Debug)]
pub struct ParseError(/// Human-readable error message (verbatim from `toml`).
                     pub String);

#[cfg(feature = "cell-dict")]
impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "cell-dict parse error: {}", self.0)
    }
}

#[cfg(feature = "cell-dict")]
impl std::error::Error for ParseError {}

#[cfg(all(test, feature = "cell-dict"))]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal() {
        let toml = r#"
[meta]
name = "test"

[[entry]]
pinyin = "rgb"
word = "RGB"
freq = 500000
"#;
        let cd = CellDict::from_toml_str(toml).unwrap();
        assert_eq!(cd.meta.name, "test");
        assert_eq!(cd.entries.len(), 1);
        assert_eq!(cd.entries[0].pinyin, "rgb");
        assert_eq!(cd.entries[0].word, "RGB");
        assert_eq!(cd.entries[0].freq, 500000);
    }

    #[test]
    fn freq_defaults_to_zero() {
        let toml = r#"
[meta]
name = "test"

[[entry]]
pinyin = "abc"
word = "XYZ"
"#;
        let cd = CellDict::from_toml_str(toml).unwrap();
        assert_eq!(cd.entries[0].freq, 0);
    }

    #[test]
    fn no_entries_is_valid() {
        let toml = r#"
[meta]
name = "empty pack"
"#;
        let cd = CellDict::from_toml_str(toml).unwrap();
        assert!(cd.entries.is_empty());
    }

    #[test]
    fn invalid_toml_returns_parse_error() {
        let toml = r#"this is = not [valid toml"#;
        let err = CellDict::from_toml_str(toml).unwrap_err();
        assert!(err.0.contains("expected") || err.0.contains("invalid") || !err.0.is_empty());
    }

    #[test]
    fn missing_required_field_returns_error() {
        // `pinyin` is required on Entry, omit it.
        let toml = r#"
[meta]
name = "bad"

[[entry]]
word = "RGB"
"#;
        assert!(CellDict::from_toml_str(toml).is_err());
    }
}
