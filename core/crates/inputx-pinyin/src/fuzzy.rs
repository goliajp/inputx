//! Fuzzy syllable expansion — `z↔zh`, `c↔ch`, `s↔sh`, `n↔l`, `f↔h`, `r↔l`,
//! `in↔ing`, `en↔eng`, `an↔ang`. Toggleable per-pair so users can match
//! their own dialect / typing habits. Expansion happens at lookup time;
//! the dictionary stays canonical (no bloat).
//!
//! v0.1 wires the config + expansion machinery; the engine consults it at
//! lookup. The expansion currently produces at most one alternate form per
//! input syllable (a single rule application). Multi-rule cascades (e.g.,
//! `zin → zhin → zhing`) are out of scope for v0.1; the long tail is
//! addressed in v0.3 once L0 ranking can demote noise.

/// Toggleable fuzzy-pair flags. All off by default.
#[derive(Debug, Clone, Copy, Default)]
pub struct FuzzyConfig {
    /// `z` ↔ `zh`
    pub z_zh: bool,
    /// `c` ↔ `ch`
    pub c_ch: bool,
    /// `s` ↔ `sh`
    pub s_sh: bool,
    /// `n` ↔ `l` (initial only)
    pub n_l: bool,
    /// `f` ↔ `h`
    pub f_h: bool,
    /// `r` ↔ `l` (initial only)
    pub r_l: bool,
    /// `in` ↔ `ing` (final)
    pub in_ing: bool,
    /// `en` ↔ `eng` (final)
    pub en_eng: bool,
    /// `an` ↔ `ang` (final)
    pub an_ang: bool,
}

impl FuzzyConfig {
    /// All pairs off — strict pinyin matching.
    pub const fn strict() -> Self {
        Self {
            z_zh: false,
            c_ch: false,
            s_sh: false,
            n_l: false,
            f_h: false,
            r_l: false,
            in_ing: false,
            en_eng: false,
            an_ang: false,
        }
    }

    /// All pairs on — common dialect-tolerant default for southern speakers.
    pub const fn permissive() -> Self {
        Self {
            z_zh: true,
            c_ch: true,
            s_sh: true,
            n_l: true,
            f_h: true,
            r_l: true,
            in_ing: true,
            en_eng: true,
            an_ang: true,
        }
    }

    /// Returns the canonical syllable plus any fuzzy alternates this config
    /// permits. The result always begins with `syl` itself; alternates are
    /// only added when the rule applies.
    ///
    /// Single-rule expansion only — does not cascade. `s.iter().count()`
    /// is between 1 and ~3 in practice.
    pub fn expand(&self, syl: &str) -> Vec<String> {
        let mut out = vec![syl.to_string()];

        // Initial-position swaps: produce the partner form, leaving the rest
        // of the syllable intact.
        let initial_swaps: &[(bool, &str, &str)] = &[
            (self.z_zh, "zh", "z"),
            (self.z_zh, "z", "zh"),
            (self.c_ch, "ch", "c"),
            (self.c_ch, "c", "ch"),
            (self.s_sh, "sh", "s"),
            (self.s_sh, "s", "sh"),
            (self.n_l, "n", "l"),
            (self.n_l, "l", "n"),
            (self.f_h, "f", "h"),
            (self.f_h, "h", "f"),
            (self.r_l, "r", "l"),
            (self.r_l, "l", "r"),
        ];
        for (on, from, to) in initial_swaps {
            if *on && let Some(rest) = syl.strip_prefix(from) {
                // Avoid the n↔l double-rule producing nonsense by only
                // applying if the prefix isn't simultaneously the partner of
                // another active rule (e.g., r→l shouldn't fire on "ri" as if
                // "li" were a more natural read; both are valid syllables).
                let mut alt = String::with_capacity(syl.len());
                alt.push_str(to);
                alt.push_str(rest);
                if alt != syl && !out.contains(&alt) {
                    out.push(alt);
                }
            }
        }

        // Final-position swaps.
        let final_swaps: &[(bool, &str, &str)] = &[
            (self.in_ing, "ing", "in"),
            (self.in_ing, "in", "ing"),
            (self.en_eng, "eng", "en"),
            (self.en_eng, "en", "eng"),
            (self.an_ang, "ang", "an"),
            (self.an_ang, "an", "ang"),
        ];
        for (on, from, to) in final_swaps {
            if *on && syl.ends_with(from) {
                let head = &syl[..syl.len() - from.len()];
                let mut alt = String::with_capacity(syl.len());
                alt.push_str(head);
                alt.push_str(to);
                if alt != syl && !out.contains(&alt) {
                    out.push(alt);
                }
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_strict() {
        let f = FuzzyConfig::default();
        assert_eq!(f.expand("zhong"), vec!["zhong"]);
    }

    #[test]
    fn z_zh_swap_both_directions() {
        let f = FuzzyConfig {
            z_zh: true,
            ..FuzzyConfig::default()
        };
        let out = f.expand("zhong");
        assert!(out.contains(&"zhong".to_string()));
        assert!(out.contains(&"zong".to_string()));

        let out = f.expand("zai");
        assert!(out.contains(&"zhai".to_string()));
    }

    #[test]
    fn final_swaps_independent() {
        let f = FuzzyConfig {
            in_ing: true,
            ..FuzzyConfig::default()
        };
        assert!(f.expand("xing").contains(&"xin".to_string()));
        assert!(f.expand("xin").contains(&"xing".to_string()));
        // Other finals untouched.
        assert_eq!(f.expand("xian"), vec!["xian"]);
    }

    #[test]
    fn permissive_includes_canonical_first() {
        let f = FuzzyConfig::permissive();
        let out = f.expand("zhong");
        assert_eq!(out[0], "zhong");
        // Should produce z- alternate (z_zh) and -ong stays.
        assert!(out.contains(&"zong".to_string()));
    }

    #[test]
    fn initial_and_final_compose_only_singly() {
        // v0.1 explicitly does single-rule expansion only — no cascading.
        let f = FuzzyConfig::permissive();
        let out = f.expand("zin");
        // Single z→zh produces "zhin"; single in→ing produces "zing". Both ok.
        assert!(
            out.contains(&"zhin".to_string()),
            "expected single z→zh: {out:?}"
        );
        assert!(
            out.contains(&"zing".to_string()),
            "expected single in→ing: {out:?}"
        );
        // Cascade (z→zh THEN in→ing) would produce "zhing" — disallowed.
        assert!(
            !out.contains(&"zhing".to_string()),
            "cascade leaked: {out:?}"
        );
    }
}
