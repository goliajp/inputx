//! Inputx pinyin engine **v2** — char-centric data model.
//!
//! This crate is the v2 of the pinyin engine, built parallel to the
//! legacy `inputx-pinyin` crate (v1). The v1/v2 switch resolves
//! through this precedence (first hit wins):
//!
//! 1. `INPUTX_PINYIN_VERSION` env var — `v2`/`V2`/`2` → v2; `v1`/`V1`/`1` → v1.
//! 2. `$XDG_CONFIG_HOME/inputx/pinyin-version` file (default
//!    `~/.config/inputx/pinyin-version`) — same accepted strings.
//! 3. `~/Library/Application Support/Inputx/pinyin-version` — macOS
//!    IME daemon reads this on launch (LaunchAgent can't easily pass
//!    env vars).
//! 4. **default = v1**.
//!
//! Probes / CLIs accept `--pinyin v1|v2` and set the env var before
//! resolution — same as 1.
//!
//! ## Phase 0 status
//!
//! Skeleton-only. [`populate`] returns [`Candidates::empty`]. With v2
//! selected the IME effectively runs literal-only on the pinyin side
//! — which matches the 2026-06-28 4-gate-off baseline so the wire is
//! observable + reversible without touching v1.
//!
//! ## Subsequent phases (see
//! `docs/pinyin-char-centric-rewrite-2026-06-29/PLAN.md`)
//!
//! 1. **Data layer** — `chars.tsv` + `readings.tsv` + `words.tsv`
//!    sourced from authoritative open data (通用规范汉字表 + Unihan
//!    kHanyuPinyin + CC-CEDICT). No corpus statistics.
//! 2. **Tier model** — tier桶 from 字表 一/二/三级 + HSK level (no
//!    raw freq field anywhere). Aligns with the existing 10-tier ×
//!    wx>px>nx orthogonal merge model — output contract unchanged.
//! 3. **Engine** — Path 1 lookup by code → reading_path → words.
//!    Forbids any reading_path that doesn't decompose into declared
//!    char readings (= 字字直拼 noise structurally impossible).

#![forbid(unsafe_code)]

use std::sync::OnceLock;

pub mod data;

fn parse_token(s: &str) -> Option<bool> {
    let s = s.trim();
    match s {
        "v2" | "V2" | "2" => Some(true),
        "v1" | "V1" | "1" => Some(false),
        _ => None,
    }
}

fn resolve_from_config_files() -> Option<bool> {
    let home = std::env::var_os("HOME")?;
    let home = home.to_string_lossy().into_owned();
    let xdg = std::env::var("XDG_CONFIG_HOME").ok();
    let candidates: [String; 3] = [
        xdg.map(|x| format!("{}/inputx/pinyin-version", x))
            .unwrap_or_else(|| format!("{}/.config/inputx/pinyin-version", home)),
        format!("{}/.config/inputx/pinyin-version", home),
        format!("{}/Library/Application Support/Inputx/pinyin-version", home),
    ];
    for path in candidates {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Some(v) = parse_token(&content) {
                return Some(v);
            }
        }
    }
    None
}

/// Resolve v1 / v2 selection via the precedence documented in
/// the crate-level docs. Read once and cache.
pub fn enabled() -> bool {
    static CACHED: OnceLock<bool> = OnceLock::new();
    *CACHED.get_or_init(|| {
        if let Ok(s) = std::env::var("INPUTX_PINYIN_VERSION") {
            if let Some(v) = parse_token(&s) {
                return v;
            }
        }
        if let Some(v) = resolve_from_config_files() {
            return v;
        }
        false
    })
}

/// V2 candidate set — the minimal shape the composite-layer caller
/// needs to slot in place of v1's populated state. Fields are by
/// design a subset of what v1's `PinyinAdapter` populates; v1 keeps
/// the full feature set during the transition.
#[derive(Debug, Clone, Default)]
pub struct Candidates {
    /// Ordered list of candidate words (highest-priority first).
    pub words: Vec<String>,
    /// True when at least one candidate is from a non-speculative
    /// path (exact match / dict lookup), not from K-best/fuzzy.
    pub has_non_speculative: bool,
    /// Optional composed full-sentence candidate (long-buffer Viterbi
    /// equivalent in v2 will be re-derived from word path probability).
    pub composed_sentence: Option<String>,
}

impl Candidates {
    pub fn empty() -> Self {
        Self::default()
    }
}

/// Phase 0 stub: returns empty for every buffer.
///
/// As phases 1-3 land, this function will resolve the buffer against
/// the char-centric chars/readings/words tables and return a non-empty
/// [`Candidates`] populated by Path-1-equivalent lookup.
pub fn populate(buffer: &str) -> Candidates {
    let _ = buffer;
    Candidates::empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enabled_defaults_to_false_when_env_unset() {
        // Can't easily test set/unset of env var without leaking to other
        // tests (OnceLock) — leave the env-driven branch to integration
        // tests with a fresh process. Just sanity-check the function
        // is callable.
        let _ = enabled();
    }

    #[test]
    fn parse_token_matches_accepted_strings() {
        assert_eq!(parse_token("v2"), Some(true));
        assert_eq!(parse_token("V2"), Some(true));
        assert_eq!(parse_token("2"), Some(true));
        assert_eq!(parse_token("v1"), Some(false));
        assert_eq!(parse_token("V1"), Some(false));
        assert_eq!(parse_token("1"), Some(false));
        assert_eq!(parse_token("  v2 \n"), Some(true), "trim whitespace");
        assert_eq!(parse_token("on"), None);
        assert_eq!(parse_token(""), None);
        assert_eq!(parse_token("3"), None);
    }

    #[test]
    fn stub_populate_returns_empty() {
        let c = populate("nihao");
        assert!(c.words.is_empty());
        assert!(!c.has_non_speculative);
        assert!(c.composed_sentence.is_none());
    }
}
