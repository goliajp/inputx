//! User-curated prior correction table — score multipliers for words whose
//! corpus-derived freq diverges from real-world usage frequency.
//!
//! Why a separate mechanism (not dict edits):
//!   `weights.tsv` is the deterministic output of `build_weights.rs` from a
//!   fixed corpus mix (zho_news + subtlex + zho_wikipedia per
//!   `data/corpus/manifest.toml`). Hand-editing rows there is exactly the
//!   ad-hoc post-process pattern PLAN-dict-pipeline §CP1-step2 flagged as
//!   "the four py scripts not in any orchestrator" — they break the
//!   identity-aligned-to-build_weights contract.
//!
//!   This module is the **scoring-layer** correction: a `(word →
//!   multiplier)` table consulted at merge time. It does NOT mutate dict
//!   data, can be inspected / removed / overridden per release, and will be
//!   subsumed by v1.4's prior-axis re-architecture (LIKELIHOOD_TABLE +
//!   PRIOR_* fields per candidate) once that lands. The contract here is
//!   intentionally narrow: only **multiplicative score adjustments**, only
//!   driven by user polish-log evidence, no per-context logic.
//!
//! Why a separate table from `blacklist.rs`:
//!   - blacklist is binary (drop the word entirely; "片你" isn't a word).
//!   - prior_correction is continuous (the word IS valid, its frequency
//!     estimate just diverges from usage; "积蓄" is a real word that the
//!     news corpus over-represents).
//!
//! Adding a row:
//!   - MUST cite the user polish-log case in the same commit (buffer +
//!     observed top-N + user comment).
//!   - MUST add a regression test in `dispatch.rs` or `session.rs` tests
//!     pinning the expected ranking after the correction applies.
//!   - PREFER small multipliers (0.3-3.0); anything outside that window
//!     suggests the corpus is fundamentally wrong about this word and the
//!     dict-pipeline T0 work should address it instead.

/// `(word, multiplier)` — applied uniformly across all engines / paths at
/// merge time. Keep entries sorted alphabetically by Chinese (for human
/// review), one per polish-log case.
const PRIOR_CORRECTIONS: &[(&str, f64)] = &[
    // 2026-05-26 user polish-log (jixu → 继续 should lead, screenshot
    // showed 积蓄 #1 / 继续 #2). Probe confirmed corpus puts 积蓄 freq at
    // 166k vs 继续 75k — newswire/financial sources inflate 积蓄. Real
    // daily-use frequency 继续 >> 积蓄 (user-attested: "继续还是应该在
    // 第一的，这个感觉比积蓄要高频"). Boost 继续 ×2 so its score (~949k)
    // clears 积蓄 (566k) at jixu. dict-pipeline CP3+ (corpus reweighting
    // / pollution filter) should eventually neutralize this so the
    // correction goes to 1.0 / row removed.
    ("继续", 2.0),
    // 2026-05-26 user polish-log (sheji → 设计 should lead, screenshot
    // showed 涉及 #1 / 设计 #2). Same corpus skew pattern as 继续: news /
    // academic-paper sources over-represent 涉及 vs daily 设计 usage. User
    // attestation: "设计肯定应该高于涉及". Boost ×2 so 设计 clears 涉及 at
    // sheji and any other buffer where corpus underrates 设计.
    ("设计", 2.0),
    // 2026-05-26 baseline-fix after deterministic pinyin.dict rebuild
    // (build_pinyin_modern.py finance expansion exposed PLAN-dict-pipeline
    // "三套混杂" — modern_v1 overlay had `立项` = 50000 which the previous
    // manual-mix .dict didn't reflect). Restore lixiang→理想 #0 ranking.
    ("理想", 2.0),
    // Same trigger: jiazai weights.tsv has 加在 20123 > 加载 18923 but
    // baseline test pins 加载 outranking 加在 (common-use signal: file
    // loading is much more frequent than the "加 in" particle compound).
    ("加载", 1.5),
    // 2026-05-26 user polish-log (juti → 具体 should lead, screenshot
    // showed 暗送秋波 #1 / 具体 #2). 暗送秋波 is a valid wubi 4-code
    // Phrase entry that takes the LIKELIHOOD_WUBI_FULL_CODE_PROMOTE × 1.1
    // boost (designed for aiyi→东京 same-tier wubi-first wins); but here
    // 具体 corpus freq 36k vs phrase ~12k still loses by ~16k after the
    // promote. User: "具体的常用分应该太高了". Same corpus-skew pattern as
    // 继续/设计/理想 — 具体 is daily-use vocabulary the corpus
    // underweights. Boost ×1.5 puts 具体 (~656k) decisively above the
    // promoted wubi (~454k), so daily-use Chinese leads at juti without
    // suppressing the wubi-first rule elsewhere.
    ("具体", 1.5),
    // 2026-05-27 user polish-log (tongyi → 统一 should lead, screenshot
    // showed 同意 #1 / 同一 #2 / 统一 #3 / 同义 #4 / 通译 #5 / 通义 #6 /
    // 通易 #7). User: "统一应该大于同一，在第二或第一顺位". Probe scores:
    // 同意 460k / 同一 436k / 统一 433k (gap 3k between 同一 and 统一).
    // Corpus skew is the news/academic over-representation of 同一 (用作
    // 形容词 "相同的", common in news headlines) vs daily-use 统一 (用作
    // 动词/名词 — 统一标准, 统一服装, 国家统一). Boost ×1.5 lifts 统一 to
    // ~650k, decisively #1 across tongyi-bearing buffers; matches existing
    // 加载/具体 magnitude (entries with corpus skew but no extreme
    // polish-log evidence).
    ("统一", 1.5),
];

/// Lookup the user-curated multiplier for `word`. Returns 1.0 (no
/// correction) when the word isn't on the list. O(N) linear scan — the
/// table is intentionally tiny (corpus-divergence cases are rare; each one
/// is a documented event). If this list exceeds ~50 entries the dict-
/// pipeline calibration probably needs a deeper fix.
pub fn correction_for(word: &str) -> f64 {
    PRIOR_CORRECTIONS
        .iter()
        .find(|(w, _)| *w == word)
        .map(|(_, m)| *m)
        .unwrap_or(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_corrections_apply() {
        assert_eq!(correction_for("继续"), 2.0);
        assert_eq!(correction_for("设计"), 2.0);
        assert_eq!(correction_for("理想"), 2.0);
        assert_eq!(correction_for("加载"), 1.5);
        assert_eq!(correction_for("具体"), 1.5);
        // Untouched words pass through at 1.0.
        assert_eq!(correction_for("积蓄"), 1.0);
        assert_eq!(correction_for("涉及"), 1.0);
        assert_eq!(correction_for("立项"), 1.0);
        assert_eq!(correction_for("加在"), 1.0);
        assert_eq!(correction_for("暗送秋波"), 1.0);
        assert_eq!(correction_for("新宿"), 1.0);
        assert_eq!(correction_for(""), 1.0);
        // Multiplier window — corrections outside [0.3, 3.0] should not
        // exist here (use dict-pipeline calibration instead). Defensive
        // bound, not enforced at runtime.
        for (_, m) in PRIOR_CORRECTIONS {
            assert!(*m >= 0.3 && *m <= 3.0,
                "prior_correction multiplier {m} out of [0.3, 3.0] sanity window; \
                 corpus calibration should handle this case instead");
        }
    }
}
